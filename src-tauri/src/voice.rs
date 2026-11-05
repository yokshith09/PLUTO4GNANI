//! Voice commands (API_CONTRACTS §4.2) and the `voice://…` events (§5). Owner: **D1** (T-007).
//!
//! **What is real and what is a stub (T-007):**
//! - Real: `audio_push` (frames → `pluto_voice::audio::AudioRouter`), the key commands (OS keychain
//!   through `pluto_core::secrets`), `privacy_log_list` / `privacy_log_clear` (`voice_store`).
//! - Push-to-talk is a stub that **records each press to a WAV file** (`<data>/voice-check/
//!   last-ptt.wav`) so the mic path can be checked end to end (T-011); it goes Listening → Idle.
//! - `voice_speak` streams a 1 s test tone (PCM16 24 kHz) over the `playback` Channel instead of
//!   Timbre (T-102 replaces it); `voice_stop_speaking` stops it (barge-in).
//! - Everything else returns the shared fixtures in `test-data/fixtures/voice/`, so the UI can be
//!   built against the real shell before the runtime (T-301) and assistant (T-303) land.
//!
//! New commands return `VoiceError`, never core's `AppError` (API_CONTRACTS §0).

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use pluto_contracts::error::{VoiceError, VoiceErrorCode, VoiceResult};
use pluto_contracts::speech::{FrameSink, TextToSpeech, TtsRequest};
use pluto_contracts::voice::{
    AssistantTurn, Destination, PrivacyLogEntry, ProposalResponse, ToolResult, UsageCounters,
    VoiceMode, VoiceSettings, VoiceSettingsPatch, VoiceState, VoiceStatus,
};
use pluto_core::secrets::{Account, KeychainSecrets, SecretStore};
use pluto_core::voice_store;
use pluto_voice::audio::{AudioRouter, WavSink};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tauri::ipc::{Channel, InvokeBody, InvokeResponseBody, Request};
use tauri::{AppHandle, Emitter, State};

use crate::ipc::AppPaths;
use crate::Core;

// ---- events --------------------------------------------------------------------------------

/// Event names (API_CONTRACTS §5). The UI listens with `onNamed` (`src/ipc/namedEvents.ts`).
/// The single home of the names: the runtime (T-301), assistant (T-303) and Batch poller (T-201)
/// emit the ones the stubs don't use yet.
#[allow(dead_code)]
pub(crate) mod events {
    pub const STATUS: &str = "voice://status";
    pub const TRANSCRIPT: &str = "voice://transcript";
    pub const TURN: &str = "voice://turn";
    pub const PROPOSAL: &str = "voice://proposal";
    pub const ERROR: &str = "voice://error";
    pub const MEETING_CAPTION: &str = "meeting://caption";
    pub const MEETING_TICKER: &str = "meeting://ticker";
    pub const MEETING_JOB: &str = "meeting://job";
}

/// Emits a `voice://…` / `meeting://…` event to every window. A failure only means no window is
/// listening, so it is logged, never returned.
pub(crate) fn emit<T: Serialize + Clone>(app: &AppHandle, name: &str, payload: &T) {
    if let Err(error) = app.emit(name, payload.clone()) {
        tracing::debug!(%error, event = name, "couldn't emit a voice event");
    }
}

// ---- fixtures (stubs) ----------------------------------------------------------------------

/// Reads `data` out of a shared fixture (`{ "contract_version": 1, "data": … }`). The fixtures are
/// compiled in and checked against their types by `pluto-contracts/tests/fixtures.rs`.
pub(crate) fn fixture<T: DeserializeOwned>(json: &'static str) -> VoiceResult<T> {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()
        .and_then(|doc| serde_json::from_value(doc.get("data")?.clone()).ok())
        .ok_or_else(|| VoiceError::new(VoiceErrorCode::Core, "A built-in sample is damaged."))
}

pub(crate) mod fixtures {
    pub const SETTINGS_DEFAULT: &str =
        include_str!("../../test-data/fixtures/voice/voice_settings.default.json");
    pub const TOOL_RESULT_OK: &str =
        include_str!("../../test-data/fixtures/voice/tool_result.ok.json");
    pub const USAGE_SAMPLE: &str =
        include_str!("../../test-data/fixtures/voice/usage_counters.sample.json");
    #[cfg(test)]
    pub const TRANSCRIPTION_JOB: &str =
        include_str!("../../test-data/fixtures/meetings/transcription_job.completed.json");
    // Only the fixture test uses these two now: the real commands read the database.
    pub const TRANSCRIPT_AS1: &str =
        include_str!("../../test-data/fixtures/meetings/transcript_segments.as1.json");
    #[cfg(test)]
    pub const EXTRACTION_AS1: &str =
        include_str!("../../test-data/fixtures/meetings/extraction_result.as1.json");
    #[cfg(test)]
    pub const RECALL_OK: &str =
        include_str!("../../test-data/fixtures/meetings/recall_answer.ok.json");
}

// ---- state ---------------------------------------------------------------------------------

/// What the voice shell keeps between calls. Managed by Tauri (`app.manage`).
pub(crate) struct VoiceShell {
    pub(crate) router: Arc<AudioRouter>,
    secrets: Arc<dyn SecretStore>,
    inner: Mutex<Inner>,
}

struct Inner {
    status: VoiceStatus,
    /// Stub settings until the settings service keys land (T-105 / T-401).
    settings: VoiceSettings,
    /// The open push-to-talk session.
    ptt: Option<String>,
    /// The running `voice_speak`, so `voice_stop_speaking` can cut it off.
    speaking: Option<tokio::task::AbortHandle>,
}

impl VoiceShell {
    pub(crate) fn new() -> VoiceResult<Self> {
        Ok(Self {
            router: Arc::new(AudioRouter::new()),
            secrets: Arc::new(KeychainSecrets),
            inner: Mutex::new(Inner {
                status: VoiceStatus::new(VoiceState::Idle),
                settings: fixture(fixtures::SETTINGS_DEFAULT)?,
                ptt: None,
                speaking: None,
            }),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn set_status(&self, app: &AppHandle, status: VoiceStatus) {
        self.lock().status = status.clone();
        emit(app, events::STATUS, &status);
    }

    fn has_key(&self, account: Account) -> bool {
        self.secrets.has(account).unwrap_or(false)
    }

    fn settings(&self) -> VoiceSettings {
        let mut s = self.lock().settings.clone();
        s.has_gnani_key = self.has_key(Account::Gnani);
        s.has_gemini_key = self.has_key(Account::Gemini);
        s.has_groq_key = self.has_key(Account::Groq);
        s
    }
}

fn account(name: &str) -> VoiceResult<Account> {
    Account::parse(name).map_err(|_| {
        VoiceError::new(
            VoiceErrorCode::MissingApiKey,
            "That isn't a service Pluto has a key for.",
        )
    })
}

/// Where push-to-talk presses are recorded for the mic check (T-011).
fn ptt_wav(paths: &AppPaths) -> VoiceResult<PathBuf> {
    let dir = paths.data_dir.join("voice-check");
    std::fs::create_dir_all(&dir).map_err(|_| {
        VoiceError::new(
            VoiceErrorCode::Core,
            "Couldn't create the voice check folder.",
        )
    })?;
    Ok(dir.join("last-ptt.wav"))
}

// ---- settings and keys ---------------------------------------------------------------------

#[tauri::command]
pub async fn voice_settings_get(shell: State<'_, VoiceShell>) -> VoiceResult<VoiceSettings> {
    Ok(shell.settings())
}

/// Stub: kept in memory until the `voice.*` settings keys land (T-105 / T-401).
#[tauri::command]
pub async fn voice_settings_set(
    patch: VoiceSettingsPatch,
    shell: State<'_, VoiceShell>,
) -> VoiceResult<VoiceSettings> {
    {
        let mut inner = shell.lock();
        let s = &mut inner.settings;
        if let Some(v) = patch.cloud_voice_enabled {
            s.cloud_voice_enabled = v;
        }
        if let Some(v) = patch.languages {
            s.languages = v;
        }
        if let Some(v) = patch.stt_format {
            s.stt_format = v;
        }
        if let Some(v) = patch.tts_voice {
            s.tts_voice = v;
        }
        if let Some(v) = patch.tts_model {
            s.tts_model = v;
        }
        if let Some(v) = patch.hotkey {
            s.hotkey = v;
        }
        if let Some(v) = patch.audio_tags {
            s.audio_tags = v;
        }
        if let Some(v) = patch.always_on_enabled {
            s.always_on_enabled = v;
        }
    }
    Ok(shell.settings())
}

/// Real: the key goes straight to the OS keychain and is never returned or logged.
#[tauri::command]
pub async fn voice_key_set(
    account: String,
    secret: String,
    shell: State<'_, VoiceShell>,
) -> VoiceResult<()> {
    let acct = self::account(&account)?;
    let secret = pluto_core::secrets::validate(&secret)?;
    shell.secrets.set(acct, &secret)?;
    Ok(())
}

#[tauri::command]
pub async fn voice_key_clear(account: String, shell: State<'_, VoiceShell>) -> VoiceResult<()> {
    shell.secrets.clear(self::account(&account)?)?;
    Ok(())
}

// ---- runtime -------------------------------------------------------------------------------

#[tauri::command]
pub async fn voice_status(shell: State<'_, VoiceShell>) -> VoiceResult<VoiceStatus> {
    Ok(shell.lock().status.clone())
}

/// Stub: opens a capture session that records to `<data>/voice-check/last-ptt.wav` (overwritten
/// each press) and reports Listening. Returns the session id for `audio_push`.
#[tauri::command]
pub async fn voice_ptt_start(
    app: AppHandle,
    shell: State<'_, VoiceShell>,
    paths: State<'_, AppPaths>,
) -> VoiceResult<String> {
    if let Some(old) = shell.lock().ptt.take() {
        shell.router.close(&old);
    }
    let id = pluto_core::ids::new_id();
    let wav: Arc<dyn FrameSink> = Arc::new(WavSink::create(ptt_wav(&paths)?)?);
    shell.router.open(&id, vec![wav])?;
    shell.lock().ptt = Some(id.clone());
    shell.set_status(
        &app,
        VoiceStatus {
            mode: Some(VoiceMode::Assistant),
            session_id: Some(id.clone()),
            ..VoiceStatus::new(VoiceState::Listening)
        },
    );
    Ok(id)
}

/// Stub: closes the session (the WAV is finalised) and goes back to Idle. T-301 sends the
/// Realtime `commit` here and moves to Thinking.
#[tauri::command]
pub async fn voice_ptt_stop(app: AppHandle, shell: State<'_, VoiceShell>) -> VoiceResult<()> {
    let session = shell.lock().ptt.take();
    if let Some(id) = session {
        if let Some((summary, errors)) = shell.router.close(&id) {
            tracing::info!(
                frames = summary.frames,
                ms = summary.duration_ms,
                "push-to-talk recorded"
            );
            for (_, e) in errors {
                emit(&app, events::ERROR, &e);
            }
        }
    }
    shell.set_status(&app, VoiceStatus::new(VoiceState::Idle));
    Ok(())
}

#[tauri::command]
pub async fn voice_cancel(app: AppHandle, shell: State<'_, VoiceShell>) -> VoiceResult<()> {
    let (session, speaking) = {
        let mut inner = shell.lock();
        (inner.ptt.take(), inner.speaking.take())
    };
    if let Some(id) = session {
        shell.router.close(&id);
    }
    if let Some(task) = speaking {
        task.abort();
    }
    shell.set_status(&app, VoiceStatus::new(VoiceState::Idle));
    Ok(())
}

/// Real: one 1,024-byte PCM16 16 kHz frame as the raw body, the session in `x-session-id`
/// (API_CONTRACTS §4.2). Frames for a closed session are dropped silently.
#[tauri::command]
pub async fn audio_push(
    request: Request<'_>,
    app: AppHandle,
    shell: State<'_, VoiceShell>,
) -> VoiceResult<()> {
    let session = request
        .headers()
        .get("x-session-id")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            VoiceError::new(
                VoiceErrorCode::UnsupportedAudio,
                "Audio arrived without its session.",
            )
        })?;
    let InvokeBody::Raw(pcm) = request.body() else {
        return Err(VoiceError::new(
            VoiceErrorCode::UnsupportedAudio,
            "Audio must be sent as raw bytes.",
        ));
    };
    let outcome = shell.router.push(session, pcm.clone())?;
    for (_, e) in outcome.new_failures {
        emit(&app, events::ERROR, &e);
    }
    Ok(())
}

/// Stub: always the sample "ok" result. T-303 runs the proposal gate here.
#[tauri::command]
pub async fn voice_proposal_respond(
    proposal_id: String,
    response: ProposalResponse,
) -> VoiceResult<ToolResult> {
    let _ = (proposal_id, response);
    fixture(fixtures::TOOL_RESULT_OK)
}

/// Stub: streams a 1 s 440 Hz test tone (PCM16 24 kHz) to `playback`, reporting Speaking then
/// Idle. Returns when the tone ends or `voice_stop_speaking` cuts it off. T-102 swaps in Timbre.
#[tauri::command]
pub async fn voice_speak(
    text: String,
    language: Option<String>,
    playback: Channel<InvokeResponseBody>,
    app: AppHandle,
    shell: State<'_, VoiceShell>,
) -> VoiceResult<()> {
    if let Some(old) = shell.lock().speaking.take() {
        old.abort();
    }
    shell.set_status(&app, VoiceStatus::new(VoiceState::Speaking));
    let request = TtsRequest {
        text,
        language: language.unwrap_or_else(|| "auto".into()),
        voice: None,
        model: None,
        speed: None,
    };
    let task = tokio::spawn(async move {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(16);
        let tts = pluto_gnani::mock::MockTts::new();
        let produce = tts.stream(request, tx);
        let forward = async {
            while let Some(chunk) = rx.recv().await {
                if playback.send(InvokeResponseBody::Raw(chunk)).is_err() {
                    break; // the page went away
                }
            }
        };
        let (_, ()) = tokio::join!(produce, forward);
    });
    let id = task.id();
    shell.lock().speaking = Some(task.abort_handle());
    // Ends when the tone finishes, or with a cancellation error when voice_stop_speaking or
    // voice_cancel aborts it. Either way it is not an error for the caller.
    let _ = task.await;
    let was_current = {
        let mut inner = shell.lock();
        let current = inner.speaking.as_ref().is_some_and(|t| t.id() == id);
        if current {
            inner.speaking = None;
        }
        current
    };
    if was_current {
        shell.set_status(&app, VoiceStatus::new(VoiceState::Idle));
    }
    Ok(())
}

#[tauri::command]
pub async fn voice_stop_speaking(app: AppHandle, shell: State<'_, VoiceShell>) -> VoiceResult<()> {
    let speaking = shell.lock().speaking.take();
    if let Some(task) = speaking {
        task.abort();
        shell.set_status(&app, VoiceStatus::new(VoiceState::Idle));
    }
    Ok(())
}

// ---- history, usage, privacy ---------------------------------------------------------------

/// Stub: the sample counters. T-506 counts from the privacy log.
#[tauri::command]
pub async fn voice_usage() -> VoiceResult<UsageCounters> {
    fixture(fixtures::USAGE_SAMPLE)
}

/// Stub: empty until the assistant records turns (T-303).
#[tauri::command]
pub async fn voice_history(limit: u32) -> VoiceResult<Vec<AssistantTurn>> {
    let _ = limit;
    Ok(Vec::new())
}

/// Real: newest first from `privacy_log`.
#[tauri::command]
pub async fn privacy_log_list(
    limit: u32,
    core: State<'_, Core>,
) -> VoiceResult<Vec<PrivacyLogEntry>> {
    let rows = core
        .db
        .read(move |c| voice_store::privacy_log_list(c, limit))
        .await?;
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            let destination: Destination =
                serde_json::from_value(serde_json::Value::String(r.destination)).ok()?;
            Some(PrivacyLogEntry {
                id: r.id,
                at: r.at,
                destination,
                purpose: r.purpose,
                bytes: r.bytes,
                ref_id: r.ref_id,
            })
        })
        .collect())
}

#[tauri::command]
pub async fn privacy_log_clear(core: State<'_, Core>) -> VoiceResult<()> {
    core.db
        .write(|tx| Ok((voice_store::privacy_log_clear(tx).map(|_| ())?, vec![])))
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pluto_contracts::meeting::{
        ExtractionResult, RecallAnswer, TranscriptSegment, TranscriptionJob,
    };

    #[test]
    fn every_compiled_in_fixture_parses_into_its_type() {
        fixture::<VoiceSettings>(fixtures::SETTINGS_DEFAULT).unwrap();
        fixture::<ToolResult>(fixtures::TOOL_RESULT_OK).unwrap();
        fixture::<UsageCounters>(fixtures::USAGE_SAMPLE).unwrap();
        fixture::<TranscriptionJob>(fixtures::TRANSCRIPTION_JOB).unwrap();
        fixture::<Vec<TranscriptSegment>>(fixtures::TRANSCRIPT_AS1).unwrap();
        fixture::<ExtractionResult>(fixtures::EXTRACTION_AS1).unwrap();
        fixture::<RecallAnswer>(fixtures::RECALL_OK).unwrap();
    }

    #[test]
    fn a_damaged_fixture_is_a_voice_error_not_a_panic() {
        let err = fixture::<VoiceSettings>("{ not json").unwrap_err();
        assert_eq!(err.code, VoiceErrorCode::Core);
        assert!(fixture::<VoiceSettings>(r#"{"contract_version":1}"#).is_err());
    }

    #[test]
    fn event_names_match_api_contracts() {
        assert_eq!(events::STATUS, "voice://status");
        assert_eq!(events::MEETING_JOB, "meeting://job");
    }

    #[test]
    fn unknown_key_accounts_are_refused() {
        assert!(account("gnani").is_ok());
        assert_eq!(
            account("openai").unwrap_err().code,
            VoiceErrorCode::MissingApiKey
        );
    }
}
