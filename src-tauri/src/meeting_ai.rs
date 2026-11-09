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
use pluto_contracts::speech::FrameSink;
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
use tauri::ipc::{Channel, InvokeResponseBody, JavaScriptChannelId};
use tauri::{AppHandle, State, Webview};
use tauri_plugin_dialog::DialogExt;

use crate::voice::VoiceShell;
use crate::Core;

/// Recordings in progress, and the lazily built meeting-AI service.
pub(crate) struct MeetingShell {
    sessions: Mutex<HashMap<String, Arc<ActiveRecording>>>,
    recorder: Arc<Recorder>,
    /// The real transcript/extraction service, built on first use (keys come from the keychain).
    ai: OnceLock<Arc<MeetingAi>>,
}

impl MeetingShell {
    pub(crate) fn new(recorder: Arc<Recorder>) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            recorder,
            ai: OnceLock::new(),
        }
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
) -> VoiceResult<RecordingSession> {
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
    let sink: Arc<dyn FrameSink> = rec.sink.clone();
    if let Err(e) = voice.router.open(&rec.session.id, vec![sink]) {
        // Nothing is being captured: don't leave a meeting stuck in "recording".
        let _ = shell.recorder.stop(&rec).await;
        voice.runtime.recording_stopped();
        return Err(e);
    }
    let session = rec.session.clone();
    shell.lock().insert(session.id.clone(), Arc::new(rec));
    Ok(session)
}

/// Ends the recording, saves the last chunk and queues the transcription job (T-201).
pub(crate) async fn finish_recording(
    voice: &VoiceShell,
    jobs: &Arc<MeetingJobs>,
    shell: &MeetingShell,
    session_id: &str,
) -> VoiceResult<RecordingSession> {
    let rec = shell.get(session_id)?;
    // Closing the audio session ends the sink, which closes the last chunk.
    voice.router.close(session_id);
    // Voice is released first, so a failure while saving can't leave push-to-talk stuck on Busy.
    voice.runtime.recording_stopped();
    let session = shell.recorder.stop(&rec).await?;
    shell.lock().remove(session_id);
    jobs.queue(&session.meeting_id).await?;
    Ok(session)
}

#[tauri::command]
pub async fn meeting_rec_start(
    title: String,
    participants: Vec<String>,
    live_captions: bool,
    consent: bool,
    voice: State<'_, VoiceShell>,
    shell: State<'_, MeetingShell>,
) -> VoiceResult<RecordingSession> {
    begin_recording(&voice, &shell, &title, participants, live_captions, consent).await
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

/// Reading the recap aloud or exporting it needs the speech engine (T-102): until then this says so
/// instead of silently doing nothing. Without `speak` or `export` there is nothing to do.
///
/// `playback` is optional, which Tauri's `Channel` argument can't be, so it arrives as a channel
/// id and becomes a `Channel` on the calling window only when it is given.
#[tauri::command]
pub async fn meeting_recap(
    meeting_id: String,
    language: String,
    speak: bool,
    export: bool,
    playback: Option<JavaScriptChannelId>,
    webview: Webview,
) -> VoiceResult<Option<String>> {
    let playback: Option<Channel<InvokeResponseBody>> = playback.map(|id| id.channel_on(webview));
    let _ = (meeting_id, language, playback);
    if speak || export {
        return Err(VoiceError::new(
            VoiceErrorCode::Core,
            "Spoken recaps arrive with the voice engine. The written recap is ready now.",
        ));
    }
    Ok(None)
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
