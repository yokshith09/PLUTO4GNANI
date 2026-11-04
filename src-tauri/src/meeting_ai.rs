//! Meeting-AI commands (API_CONTRACTS §4.3). Stubs written by **D1** at T-007; owned by **D2**
//! from here on (WORKING_AGREEMENT §2), who replaces them with thin wrappers over `pluto-intel`.
//!
//! Every stub returns the shared fixtures in `test-data/fixtures/meetings/` (the AS-1 meeting), so
//! the Meetings UI can be built against the real shell. Two rules are already real because the
//! contract requires them everywhere: recording needs consent (`consent_required`), and
//! `meeting_extraction_apply` returns the real meeting (`pluto_core::meetings::get`).
//!
//! Not registered yet (P1, arrive with their tasks): `glossary_list` / `glossary_add` /
//! `glossary_delete` (T-505) and `compare_transcribe` (T-504).

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use pluto_contracts::error::{VoiceError, VoiceErrorCode, VoiceResult};
use pluto_contracts::meeting::{
    ApplyExtraction, Commitment, CommitmentStatus, ExtractedItem, ExtractionResult, RecallAnswer,
    RecordingSession, SpeakerMapping, TranscriptSegment, TranscriptionJob,
};
use pluto_core::clock::Clock;
use pluto_core::meetings::{self, Meeting};
use tauri::ipc::{Channel, InvokeResponseBody, JavaScriptChannelId};
use tauri::{State, Webview};

use crate::voice::{fixture, fixtures};
use crate::Core;

/// Open recording sessions (stub: in memory only).
#[derive(Default)]
pub(crate) struct MeetingShell {
    sessions: Mutex<HashMap<String, RecordingSession>>,
}

impl MeetingShell {
    fn lock(&self) -> MutexGuard<'_, HashMap<String, RecordingSession>> {
        self.sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
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

/// Stub: the completed AS-1 job. T-201 uploads the file to Gnani Batch.
#[tauri::command]
pub async fn meeting_import(
    title: String,
    participants: Vec<String>,
    path: String,
) -> VoiceResult<TranscriptionJob> {
    let _ = (title, participants, path);
    fixture(fixtures::TRANSCRIPTION_JOB)
}

#[tauri::command]
pub async fn meeting_job_get(meeting_id: String) -> VoiceResult<Option<TranscriptionJob>> {
    let _ = meeting_id;
    fixture(fixtures::TRANSCRIPTION_JOB).map(Some)
}

#[tauri::command]
pub async fn meeting_job_retry(meeting_id: String) -> VoiceResult<TranscriptionJob> {
    let _ = meeting_id;
    fixture(fixtures::TRANSCRIPTION_JOB)
}

#[tauri::command]
pub async fn meeting_transcript_get(meeting_id: String) -> VoiceResult<Vec<TranscriptSegment>> {
    let _ = meeting_id;
    fixture(fixtures::TRANSCRIPT_AS1)
}

#[tauri::command]
pub async fn meeting_speakers_set(
    meeting_id: String,
    mapping: Vec<SpeakerMapping>,
) -> VoiceResult<()> {
    let _ = (meeting_id, mapping);
    Ok(())
}

// ---- extraction, recall, recap -------------------------------------------------------------

#[tauri::command]
pub async fn meeting_extract(meeting_id: String) -> VoiceResult<ExtractionResult> {
    let _ = meeting_id;
    fixture(fixtures::EXTRACTION_AS1)
}

/// Stub: applies nothing yet; returns the meeting as it is (T-204).
#[tauri::command]
pub async fn meeting_extraction_apply(
    input: ApplyExtraction,
    core: State<'_, Core>,
) -> VoiceResult<Meeting> {
    Ok(meetings::get(&core.db, &input.meeting_id).await?)
}

#[tauri::command]
pub async fn meeting_items_get(meeting_id: String) -> VoiceResult<Vec<ExtractedItem>> {
    let _ = meeting_id;
    fixture::<ExtractionResult>(fixtures::EXTRACTION_AS1).map(|r| r.items)
}

#[tauri::command]
pub async fn meeting_recall(
    question: String,
    language: Option<String>,
) -> VoiceResult<RecallAnswer> {
    let _ = (question, language);
    fixture(fixtures::RECALL_OK)
}

/// Stub: nothing is spoken or exported yet (T-205); returns no file.
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
    let _ = (meeting_id, language, speak, export, playback);
    Ok(None)
}

#[tauri::command]
pub async fn commitments_list(
    owner: Option<String>,
    status: Option<CommitmentStatus>,
) -> VoiceResult<Vec<Commitment>> {
    let _ = (owner, status);
    Ok(Vec::new())
}
