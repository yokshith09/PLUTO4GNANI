//! Meeting-AI commands (API_CONTRACTS §4.3). Stubs written by **D1** at T-007; owned by **D2**
//! from here on (WORKING_AGREEMENT §2), who replaces them with thin wrappers over `pluto-intel`.
//!
//! Every stub returns the shared fixtures in `test-data/fixtures/meetings/` (the AS-1 meeting), so
//! the Meetings UI can be built against the real shell. Real now: recording needs consent,
//! import and transcription jobs run on `pluto_voice::meeting_jobs` (T-201), and transcript,
//! speakers, extraction, apply and items run on `pluto_intel::meeting::service`.
//!
//! Not registered yet (P1, arrive with their tasks): `glossary_list` / `glossary_add` /
//! `glossary_delete` (T-505) and `compare_transcribe` (T-504).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use pluto_contracts::error::{VoiceError, VoiceErrorCode, VoiceResult};
use pluto_contracts::meeting::{
    ApplyExtraction, Commitment, CommitmentStatus, ExtractedItem, ExtractionResult, RecallAnswer,
    RecordingSession, SpeakerMapping, TranscriptSegment, TranscriptionJob,
};
use pluto_contracts::speech::{FrameSink, TextToSpeech};
use pluto_core::meetings::Meeting;
use pluto_core::secrets::{KeychainSecrets, SecretStore};
use pluto_gnani::http::OutboundLog;
use pluto_intel::llm::gemini::{self, GeminiClient};
use pluto_intel::llm::groq::{self, GroqClient};
use pluto_intel::llm::privacy::DbOutboundLog;
use pluto_intel::llm::router::Router;
use pluto_intel::meeting::recorder::{ActiveRecording, Recorder};
use pluto_intel::meeting::service::MeetingAi;
use pluto_voice::meeting_jobs::MeetingJobs;
use pluto_voice::privacy::CloudGated;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::DialogExt;

use crate::voice::VoiceShell;
use crate::Core;

/// Recordings in progress, and the lazily built meeting-AI service.
pub(crate) struct MeetingShell {
    sessions: Mutex<HashMap<String, Arc<ActiveRecording>>>,
    /// Recordings that also capture the computer's sound; dropping one stops that capture.
    system: Mutex<HashMap<String, pluto_loopback::Loopback>>,
    /// The recording running now (any kind), so the pet can show it with a Stop button.
    current: Mutex<Option<crate::auto_meeting::AutoRec>>,
    recorder: Arc<Recorder>,
    /// The real transcript/extraction service, built on first use (keys come from the keychain).
    ai: OnceLock<Arc<MeetingAi>>,
}

impl MeetingShell {
    pub(crate) fn new(recorder: Arc<Recorder>) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            system: Mutex::new(HashMap::new()),
            current: Mutex::new(None),
            recorder,
            ai: OnceLock::new(),
        }
    }

    pub(crate) fn current_recording(&self) -> Option<crate::auto_meeting::AutoRec> {
        self.current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Records which recording runs and tells the pet (`pet://recording`; `None` clears it).
    fn set_current(&self, app: &AppHandle, rec: Option<crate::auto_meeting::AutoRec>) {
        *self
            .current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = rec.clone();
        let _ = app.emit("pet://recording", rec);
    }

    fn system_lock(&self) -> MutexGuard<'_, HashMap<String, pluto_loopback::Loopback>> {
        self.system
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, Arc<ActiveRecording>>> {
        self.sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The meeting-AI service: Gemini and Groq through the router, calls written to the privacy log.
    pub(crate) fn ai(&self, core: &Core) -> Arc<MeetingAi> {
        self.ai
            .get_or_init(|| {
                let secrets: Arc<dyn SecretStore> = Arc::new(KeychainSecrets);
                let log: Arc<dyn OutboundLog> =
                    Arc::new(DbOutboundLog::new(core.db.clone(), core.clock.clone()));
                let gemini = GeminiClient::new(gemini::DEFAULT_MODEL, secrets.clone(), log.clone());
                let groq = GroqClient::new(groq::DEFAULT_MODEL, secrets, log);
                // Nothing is sent while Cloud voice is off (T-PR-04).
                let llm = Arc::new(CloudGated::new(
                    Arc::new(Router::new(Arc::new(gemini), Arc::new(groq))),
                    crate::voice::cloud_switch(),
                ));
                Arc::new(MeetingAi::new(core.db.clone(), core.clock.clone(), llm))
            })
            .clone()
    }

    fn get(&self, session_id: &str) -> VoiceResult<Arc<ActiveRecording>> {
        self.lock().get(session_id).cloned().ok_or_else(|| {
            VoiceError::new(VoiceErrorCode::Core, "That recording is no longer running.")
        })
    }
}

// ---- recording -----------------------------------------------------------------------------

/// Whether a recording also captures the computer's sound (other people on a call).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SystemAudio {
    Off,
    /// Refuse to start if it cannot be captured (the person asked for it).
    Required,
    /// Record the microphone alone if it cannot be captured (auto mode).
    BestEffort,
}

/// Starts a recording: needs consent, creates the meeting, and opens the audio session the UI
/// streams microphone frames into (`audio_push`) so they land in chunked WAV files (T-207).
/// Shared by the Record button and by auto mode (Google Meet).
pub(crate) async fn begin_recording(
    voice: &VoiceShell,
    shell: &MeetingShell,
    title: &str,
    participants: Vec<String>,
    live_captions: bool,
    consent: bool,
    system_audio: SystemAudio,
) -> VoiceResult<(RecordingSession, bool)> {
    // Push-to-talk answers Busy while a meeting records; refused if Pluto is mid-turn.
    voice.runtime.try_recording_started()?;
    let rec = match shell
        .recorder
        .start(title, participants, live_captions, consent)
        .await
    {
        Ok(rec) => rec,
        Err(e) => {
            voice.runtime.recording_stopped();
            return Err(e);
        }
    };
    let out: Arc<dyn FrameSink> = rec.sink.clone();
    // The computer's sound is added to the microphone's in time by a mixer (pluto_voice::mix).
    let mut sink = out.clone();
    let mut capture = None;
    if system_audio != SystemAudio::Off {
        let mixer = pluto_voice::mix::Mixer::new(out, &rec.session.id);
        let feed = mixer.clone();
        match pluto_loopback::start(move |pcm| feed.push_system(pcm)) {
            Ok(handle) => {
                mixer.spawn_ticker();
                sink = mixer.mic_sink();
                capture = Some(handle);
            }
            Err(e) if system_audio == SystemAudio::Required => {
                let _ = shell.recorder.stop(&rec).await;
                voice.runtime.recording_stopped();
                return Err(VoiceError::new(
                    VoiceErrorCode::Core,
                    format!("{e} Untick \"Include the computer's sound\" to record the microphone alone."),
                ));
            }
            Err(e) => tracing::warn!(error = %e, "recording the microphone alone"),
        }
    }
    if let Err(e) = voice.router.open(&rec.session.id, vec![sink]) {
        // Nothing is being captured: don't leave a meeting stuck in "recording".
        drop(capture);
        let _ = shell.recorder.stop(&rec).await;
        voice.runtime.recording_stopped();
        return Err(e);
    }
    let session = rec.session.clone();
    let with_system = capture.is_some();
    if let Some(c) = capture {
        shell.system_lock().insert(session.id.clone(), c);
    }
    shell.lock().insert(session.id.clone(), Arc::new(rec));
    shell.set_current(
        voice.app(),
        Some(crate::auto_meeting::AutoRec {
            session_id: session.id.clone(),
            meeting_id: session.meeting_id.clone(),
            started_at: session.started_at,
        }),
    );
    Ok((session, with_system))
}

/// Ends the recording, saves the last chunk and queues the transcription job (T-201).
pub(crate) async fn finish_recording(
    voice: &VoiceShell,
    jobs: &Arc<MeetingJobs>,
    shell: &MeetingShell,
    session_id: &str,
) -> VoiceResult<RecordingSession> {
    let rec = shell.get(session_id)?;
    // The computer's sound stops first, then the audio session closes and the mixer writes the rest.
    shell.system_lock().remove(session_id);
    // Closing the audio session ends the sink, which closes the last chunk.
    voice.router.close(session_id);
    // Voice is released first, so a failure while saving can't leave push-to-talk stuck on Busy.
    voice.runtime.recording_stopped();
    let session = shell.recorder.stop(&rec).await?;
    shell.lock().remove(session_id);
    shell.set_current(voice.app(), None);
    jobs.queue(&session.meeting_id).await?;
    Ok(session)
}

#[tauri::command]
pub async fn meeting_rec_start(
    title: String,
    participants: Vec<String>,
    live_captions: bool,
    consent: bool,
    system_audio: Option<bool>,
    voice: State<'_, VoiceShell>,
    shell: State<'_, MeetingShell>,
) -> VoiceResult<RecordingSession> {
    let system = if system_audio.unwrap_or(false) {
        SystemAudio::Required
    } else {
        SystemAudio::Off
    };
    begin_recording(
        &voice,
        &shell,
        &title,
        participants,
        live_captions,
        consent,
        system,
    )
    .await
    .map(|(session, _)| session)
}

#[tauri::command]
pub async fn meeting_rec_pause(
    session_id: String,
    shell: State<'_, MeetingShell>,
) -> VoiceResult<RecordingSession> {
    let rec = shell.get(&session_id)?;
    shell.recorder.pause(&rec).await
}

#[tauri::command]
pub async fn meeting_rec_resume(
    session_id: String,
    shell: State<'_, MeetingShell>,
) -> VoiceResult<RecordingSession> {
    let rec = shell.get(&session_id)?;
    Ok(shell.recorder.resume(&rec))
}

#[tauri::command]
pub async fn meeting_rec_stop(
    session_id: String,
    voice: State<'_, VoiceShell>,
    jobs: State<'_, Arc<MeetingJobs>>,
    shell: State<'_, MeetingShell>,
) -> VoiceResult<RecordingSession> {
    finish_recording(&voice, &jobs, &shell, &session_id).await
}

// ---- import and transcription --------------------------------------------------------------

/// Opens the system "open" dialog for an audio file and returns the chosen path, or `None` if the
/// person cancelled. The path comes only from the dialog, never from the webview (like
/// `data_import`), so the UI cannot ask Pluto to read an arbitrary file.
#[tauri::command]
pub async fn meeting_audio_pick(app: AppHandle) -> VoiceResult<Option<String>> {
    let chosen = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("Audio", &["wav", "mp3", "m4a", "flac", "ogg"])
            .blocking_pick_file()
    })
    .await
    .map_err(|_| {
        VoiceError::new(
            VoiceErrorCode::Core,
            "The open dialog stopped unexpectedly.",
        )
    })?;
    let Some(chosen) = chosen else {
        return Ok(None);
    };
    let path = chosen
        .into_path()
        .map_err(|_| VoiceError::new(VoiceErrorCode::Core, "Pluto can't open that file."))?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

/// Creates the meeting, copies the file into the data folder and starts the Gnani Batch job
/// (T-201, D1). Progress arrives as `meeting://job`. `path` must come from `meeting_audio_pick`.
#[tauri::command]
pub async fn meeting_import(
    title: String,
    participants: Vec<String>,
    path: String,
    jobs: State<'_, Arc<MeetingJobs>>,
) -> VoiceResult<TranscriptionJob> {
    jobs.import(&title, &participants, std::path::Path::new(&path))
        .await
}

#[tauri::command]
pub async fn meeting_job_get(
    meeting_id: String,
    jobs: State<'_, Arc<MeetingJobs>>,
) -> VoiceResult<Option<TranscriptionJob>> {
    jobs.job(&meeting_id).await
}

/// Transcribes the meeting's audio again (after a failure or gaps) as a new job.
#[tauri::command]
pub async fn meeting_job_retry(
    meeting_id: String,
    jobs: State<'_, Arc<MeetingJobs>>,
) -> VoiceResult<TranscriptionJob> {
    jobs.retry(&meeting_id).await
}

/// The transcript from the database, with each speaker's name once mapped.
#[tauri::command]
pub async fn meeting_transcript_get(
    meeting_id: String,
    core: State<'_, Core>,
    shell: State<'_, MeetingShell>,
) -> VoiceResult<Vec<TranscriptSegment>> {
    shell.ai(&core).transcript(&meeting_id).await
}

/// One line of a transcript that matches a search, with the meeting it is in.
#[derive(serde::Serialize)]
pub struct TranscriptHit {
    pub meeting_id: String,
    pub meeting_title: String,
    pub meeting_starts_at: i64,
    pub segment_id: String,
    pub start_ms: i64,
    /// Who said it: the mapped participant, or "Speaker N".
    pub speaker: Option<String>,
    pub text: String,
}

/// Search as you type across every transcript (Meetings → search). Best matches first.
#[tauri::command]
pub async fn meeting_transcript_search(
    query: String,
    core: State<'_, Core>,
) -> VoiceResult<Vec<TranscriptHit>> {
    let q = query.clone();
    let rows = core
        .db
        .read(move |c| pluto_core::meeting_ai_store::search_segments_prefix(c, &q, 40))
        .await?;
    let mut meetings: HashMap<String, Option<Meeting>> = HashMap::new();
    let mut out = Vec::new();
    for r in rows {
        if !meetings.contains_key(&r.meeting_id) {
            let m = pluto_core::meetings::get(&core.db, &r.meeting_id)
                .await
                .ok();
            meetings.insert(r.meeting_id.clone(), m);
        }
        let Some(Some(m)) = meetings.get(&r.meeting_id) else {
            continue; // the meeting was deleted
        };
        let speaker = r
            .participant_position
            .and_then(|p| m.participants.get(p as usize).cloned())
            .or_else(|| r.speaker_label.map(|l| format!("Speaker {l}")));
        out.push(TranscriptHit {
            meeting_id: r.meeting_id,
            meeting_title: m.title.clone(),
            meeting_starts_at: m.starts_at,
            segment_id: r.id,
            start_ms: r.start_ms,
            speaker,
            text: r.text,
        });
    }
    Ok(out)
}

#[tauri::command]
pub async fn meeting_speakers_set(
    meeting_id: String,
    mapping: Vec<SpeakerMapping>,
    core: State<'_, Core>,
    shell: State<'_, MeetingShell>,
) -> VoiceResult<()> {
    shell.ai(&core).set_speakers(&meeting_id, &mapping).await
}

// ---- extraction, recall, recap -------------------------------------------------------------

/// Real: reads the transcript, asks Gemini (Groq as fallback), checks every item (T-203).
#[tauri::command]
pub async fn meeting_extract(
    meeting_id: String,
    core: State<'_, Core>,
    shell: State<'_, MeetingShell>,
) -> VoiceResult<ExtractionResult> {
    shell.ai(&core).extract(&meeting_id).await
}

/// Real: writes the approved items, their provenance and the chosen tasks (T-204).
#[tauri::command]
pub async fn meeting_extraction_apply(
    input: ApplyExtraction,
    core: State<'_, Core>,
    shell: State<'_, MeetingShell>,
) -> VoiceResult<Meeting> {
    Ok(shell.ai(&core).apply(&input).await?.meeting)
}

#[tauri::command]
pub async fn meeting_items_get(
    meeting_id: String,
    core: State<'_, Core>,
    shell: State<'_, MeetingShell>,
) -> VoiceResult<Vec<ExtractedItem>> {
    shell.ai(&core).items(&meeting_id).await
}

/// Real: searches the transcripts and answers only from what it finds, with citations (T-305).
#[tauri::command]
pub async fn meeting_recall(
    question: String,
    language: Option<String>,
    core: State<'_, Core>,
    shell: State<'_, MeetingShell>,
) -> VoiceResult<RecallAnswer> {
    shell.ai(&core).recall(&question, language.as_deref()).await
}

/// Real: a short written recap from the applied items (T-205).
#[tauri::command]
pub async fn meeting_recap_text(
    meeting_id: String,
    language: String,
    core: State<'_, Core>,
    shell: State<'_, MeetingShell>,
) -> VoiceResult<String> {
    shell.ai(&core).recap_text(&meeting_id, &language).await
}

/// Saves the recap as an mp3 in the recap's language (Timbre), to a place the person picks.
/// Returns the saved path, or `None` if they cancelled the save dialog. Reading aloud is done by
/// the page (`voice_speak`), so `speak` with nothing to export is a no-op here.
///
/// The text is written first (Gemini or Groq), then spoken (Timbre): both need Cloud voice on.
#[tauri::command]
pub async fn meeting_recap(
    meeting_id: String,
    language: String,
    export: bool,
    app: AppHandle,
    core: State<'_, Core>,
    shell: State<'_, MeetingShell>,
    voice: State<'_, VoiceShell>,
) -> VoiceResult<Option<String>> {
    if !export {
        return Ok(None);
    }
    let settings = voice.settings();
    if !settings.cloud_voice_enabled {
        return Err(VoiceError::new(
            VoiceErrorCode::CloudVoiceDisabled,
            "Turn on Cloud voice in Settings to save the recap as audio.",
        ));
    }
    let text = shell.ai(&core).recap_text(&meeting_id, &language).await?;
    let request = pluto_contracts::speech::TtsRequest {
        text: pluto_voice::speakable::speakable(&text),
        language: language.clone(),
        voice: Some(settings.tts_voice.trim().to_string())
            .filter(|v| pluto_gnani::tts::VOICES_V3.contains(&v.as_str())),
        model: None,
        speed: None,
    };
    let audio = voice
        .tts_client()
        .synthesize(request, pluto_contracts::speech::AudioContainer::Mp3)
        .await?;
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("MP3 audio", &["mp3"])
            .set_file_name("meeting-recap.mp3")
            .blocking_save_file()
    })
    .await
    .map_err(|_| VoiceError::new(VoiceErrorCode::Core, "The save dialog could not open."))?;
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    std::fs::write(&path, audio).map_err(|_| {
        VoiceError::new(
            VoiceErrorCode::Core,
            "Pluto couldn't save the file there. Pick another folder.",
        )
    })?;
    Ok(Some(path.display().to_string()))
}

/// Real: who promised what across all meetings, and what is late (T-503).
#[tauri::command]
pub async fn commitments_list(
    owner: Option<String>,
    status: Option<CommitmentStatus>,
    core: State<'_, Core>,
    shell: State<'_, MeetingShell>,
) -> VoiceResult<Vec<Commitment>> {
    shell.ai(&core).commitments(owner.as_deref(), status).await
}
