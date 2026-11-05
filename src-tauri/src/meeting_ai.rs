//! Meeting-AI commands (API_CONTRACTS §4.3). Stubs written by **D1** at T-007; owned by **D2**
//! from here on (WORKING_AGREEMENT §2), who replaces them with thin wrappers over `pluto-intel`.
//!
//! Every stub returns the shared fixtures in `test-data/fixtures/meetings/` (the AS-1 meeting), so
//! the Meetings UI can be built against the real shell. Real now: recording needs consent,
//! and transcript, speakers, extraction, apply and items run on `pluto_intel::meeting::service`.
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
use pluto_core::clock::Clock;
use pluto_core::meetings::Meeting;
use pluto_core::secrets::{KeychainSecrets, SecretStore};
use pluto_gnani::http::OutboundLog;
use pluto_intel::llm::gemini::{self, GeminiClient};
use pluto_intel::llm::groq::{self, GroqClient};
use pluto_intel::llm::privacy::DbOutboundLog;
use pluto_intel::llm::router::Router;
use pluto_intel::meeting::service::MeetingAi;
use tauri::ipc::{Channel, InvokeResponseBody, JavaScriptChannelId};
use tauri::{AppHandle, State, Webview};
use tauri_plugin_dialog::DialogExt;

use crate::voice::{fixture, fixtures};
use crate::Core;

/// Open recording sessions (stub: in memory only).
#[derive(Default)]
pub(crate) struct MeetingShell {
    sessions: Mutex<HashMap<String, RecordingSession>>,
    /// The real transcript/extraction service, built on first use (keys come from the keychain).
    ai: OnceLock<Arc<MeetingAi>>,
}

impl MeetingShell {
    fn lock(&self) -> MutexGuard<'_, HashMap<String, RecordingSession>> {
        self.sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The meeting-AI service: Gemini and Groq through the router, calls written to the privacy log.
    fn ai(&self, core: &Core) -> Arc<MeetingAi> {
        self.ai
            .get_or_init(|| {
                let secrets: Arc<dyn SecretStore> = Arc::new(KeychainSecrets);
                let log: Arc<dyn OutboundLog> =
                    Arc::new(DbOutboundLog::new(core.db.clone(), core.clock.clone()));
                let gemini = GeminiClient::new(gemini::DEFAULT_MODEL, secrets.clone(), log.clone());
                let groq = GroqClient::new(groq::DEFAULT_MODEL, secrets, log);
                let llm = Arc::new(Router::new(Arc::new(gemini), Arc::new(groq)));
                Arc::new(MeetingAi::new(core.db.clone(), core.clock.clone(), llm))
            })
            .clone()
    }

    fn get(&self, session_id: &str) -> VoiceResult<RecordingSession> {
        self.lock().get(session_id).cloned().ok_or_else(|| {
            VoiceError::new(VoiceErrorCode::Core, "That recording is no longer running.")
        })
    }
}

// ---- recording -----------------------------------------------------------------------------

/// Stub: no audio is captured yet (T-207). Refuses without consent, as the contract requires.
#[tauri::command]
pub async fn meeting_rec_start(
    title: String,
    participants: Vec<String>,
    live_captions: bool,
    consent: bool,
    core: State<'_, Core>,
    shell: State<'_, MeetingShell>,
) -> VoiceResult<RecordingSession> {
    let _ = (title, participants);
    if !consent {
        return Err(VoiceError::new(
            VoiceErrorCode::ConsentRequired,
            "Confirm that everyone agreed to be recorded before you start.",
        ));
    }
    let session = RecordingSession {
        id: pluto_core::ids::new_id(),
        meeting_id: pluto_core::ids::new_id(),
        started_at: core.clock.now_ms(),
        live_captions,
        chunk_count: 0,
    };
    shell.lock().insert(session.id.clone(), session.clone());
    Ok(session)
}

#[tauri::command]
pub async fn meeting_rec_pause(
    session_id: String,
    shell: State<'_, MeetingShell>,
) -> VoiceResult<RecordingSession> {
    shell.get(&session_id)
}

#[tauri::command]
pub async fn meeting_rec_resume(
    session_id: String,
    shell: State<'_, MeetingShell>,
) -> VoiceResult<RecordingSession> {
    shell.get(&session_id)
}

/// Stub: ends the session. The real one also queues the batch job (T-207 + T-201).
#[tauri::command]
pub async fn meeting_rec_stop(
    session_id: String,
    shell: State<'_, MeetingShell>,
) -> VoiceResult<RecordingSession> {
    let session = shell.get(&session_id)?;
    shell.lock().remove(&session_id);
    Ok(session)
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

/// The audio types Gnani accepts, and the biggest file we send (API_CONTRACTS section 2.3).
const AUDIO_EXTENSIONS: [&str; 5] = ["wav", "mp3", "m4a", "flac", "ogg"];
const MAX_IMPORT_BYTES: u64 = 10 * 1024 * 1024;

/// Checks the file for real (type, size), makes the meeting, and, **until Gnani Batch (T-201) is
/// connected**, gives it the AS-1 sample transcript instead of uploading the audio. Nothing leaves
/// the computer. D1's T-201 replaces the sample step with the real upload and transcript.
#[tauri::command]
pub async fn meeting_import(
    title: String,
    participants: Vec<String>,
    path: String,
    core: State<'_, Core>,
    shell: State<'_, MeetingShell>,
) -> VoiceResult<TranscriptionJob> {
    let file = std::path::Path::new(&path);
    let ext = file
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if !AUDIO_EXTENSIONS.contains(&ext.as_str()) {
        return Err(VoiceError::new(
            VoiceErrorCode::UnsupportedAudio,
            "Choose a WAV, MP3, M4A, FLAC or OGG file.",
        ));
    }
    let size = std::fs::metadata(file)
        .map_err(|_| VoiceError::new(VoiceErrorCode::Core, "Pluto can't open that file."))?
        .len();
    if size > MAX_IMPORT_BYTES {
        return Err(VoiceError::new(VoiceErrorCode::FileTooLarge, "That recording is over 10 MB. Cut it into shorter parts and import them one at a time."));
    }
    let sample: Vec<TranscriptSegment> = fixture(fixtures::TRANSCRIPT_AS1)?;
    shell
        .ai(&core)
        .import_with_sample(&title, participants, sample)
        .await
}

/// The newest job of a meeting from the database; `None` for a meeting typed by hand.
#[tauri::command]
pub async fn meeting_job_get(
    meeting_id: String,
    core: State<'_, Core>,
    shell: State<'_, MeetingShell>,
) -> VoiceResult<Option<TranscriptionJob>> {
    shell.ai(&core).job(&meeting_id).await
}

/// Retrying a transcription needs the real Gnani Batch job (T-201).
#[tauri::command]
pub async fn meeting_job_retry(meeting_id: String) -> VoiceResult<TranscriptionJob> {
    let _ = meeting_id;
    Err(VoiceError::new(
        VoiceErrorCode::Core,
        "Retrying a transcription arrives with the Gnani connection.",
    ))
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
