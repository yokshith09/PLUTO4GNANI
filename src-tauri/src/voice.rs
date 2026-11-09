//! Voice commands (API_CONTRACTS §4.2) and the `voice://…` events (§5). Owner: **D1** (T-007).
//!
//! **What is real and what is a stub (T-007):**
//! - Real: `audio_push` (frames → `pluto_voice::audio::AudioRouter`), the key commands (OS keychain
//!   through `pluto_core::secrets`), `privacy_log_list` / `privacy_log_clear` (`voice_store`).
//! - Push-to-talk runs on the voice runtime (`pluto_voice::runtime`, T-301): the global hotkey
//!   (press **and** release) and the talk button both go Listening → Prisma Realtime → Thinking.
//!   With Cloud voice off nothing is sent anywhere: a press instead **records to a WAV file**
//!   (`<data>/voice-check/last-ptt.wav`) so the mic path can still be checked (T-011).
//! - `voice_speak` speaks with **Timbre** (T-102) when Cloud voice is on — nothing leaves the device
//!   otherwise (NF-02) — and plays a local 1 s test tone when it is off (kept for the mic check,
//!   T-011). `voice_stop_speaking` stops either (barge-in).
//! - The assistant (`pluto_voice::assistant`, T-303) answers each turn: reads at once, writes as a
//!   proposal that runs only after `voice_proposal_respond` (or a spoken "haan"). Decisions come
//!   from D2's decider (rules, then Gemini with Groq as fallback).
//! - `voice_usage` still returns the sample in `test-data/fixtures/voice/` (T-506).
//!
//! New commands return `VoiceError`, never core's `AppError` (API_CONTRACTS §0).

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use pluto_contracts::error::{VoiceError, VoiceErrorCode, VoiceResult};
use pluto_contracts::meeting::{SegmentSource, TranscriptSegment, TranscriptionJob};
use pluto_contracts::speech::{FrameSink, RtConfig, RtTranscript, TextToSpeech, TtsRequest};
use pluto_contracts::voice::{
    AssistantTurn, Destination, PrivacyLogEntry, Proposal, ProposalResponse, ToolResult,
    UsageCounters, VoiceMode, VoiceSettings, VoiceSettingsPatch, VoiceState, VoiceStatus, WakeClip,
    WakeStatus,
};
use pluto_core::clock::Clock;
use pluto_core::db::Database;
use pluto_core::secrets::{Account, KeychainSecrets, SecretStore};
use pluto_core::voice_store;
use pluto_gnani::batch::BatchClient;
use pluto_gnani::http::{
    ApiKeySource, CloudSwitch, GnaniHttp, OutboundLog, OutboundRecord, DEFAULT_BASE_URL,
};
use pluto_gnani::realtime::{self, RealtimeClient};
use pluto_gnani::tts::{TimbreClient, DEFAULT_WS_URL};
use pluto_intel::decide::DefaultDecider;
use pluto_intel::llm::gemini::{self, GeminiClient};
use pluto_intel::llm::groq::{self, GroqClient};
use pluto_intel::llm::router::Router;
use pluto_voice::assistant::{Assistant, AssistantEvents};
use pluto_voice::audio::{AudioRouter, WavSink};
use pluto_voice::meeting_jobs::{JobEvents, MeetingJobs};
use pluto_voice::privacy::CloudGated;
use pluto_voice::runtime::{RuntimeEvents, TurnHandler, VoiceRuntime};
use pluto_voice::wake::{
    self, EnrollSink, PlutoDetector, RuntimeTurns, WakePolicy, WakeSink, WakeStore,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tauri::ipc::{Channel, InvokeBody, InvokeResponseBody, Request};
use tauri::{AppHandle, Emitter, State};

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
    /// Always-on listening changed (`WakeStatus`): open or close the always-on microphone.
    pub const WAKE: &str = "voice://wake";
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
    #[cfg(test)]
    pub const TOOL_RESULT_OK: &str =
        include_str!("../../test-data/fixtures/voice/tool_result.ok.json");
    pub const USAGE_SAMPLE: &str =
        include_str!("../../test-data/fixtures/voice/usage_counters.sample.json");
    // Only the fixture test uses these now: the real commands read the database.
    #[cfg(test)]
    pub const TRANSCRIPTION_JOB: &str =
        include_str!("../../test-data/fixtures/meetings/transcription_job.completed.json");
    #[cfg(test)]
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

/// The Gnani key, read from the OS keychain on every call (so a newly saved key works at once).
struct KeychainKey(Arc<dyn SecretStore>);

impl ApiKeySource for KeychainKey {
    fn gnani_key(&self) -> Option<String> {
        self.0.get(Account::Gnani).ok().flatten()
    }
}

/// Writes outbound requests to `privacy_log` (T-PR-02). `record` must not block, so the write runs
/// on the async runtime.
struct DbLog {
    db: Arc<Database>,
    clock: Arc<dyn Clock>,
}

impl OutboundLog for DbLog {
    fn record(&self, record: OutboundRecord) {
        let Ok(serde_json::Value::String(destination)) = serde_json::to_value(record.destination)
        else {
            return;
        };
        let (db, at) = (self.db.clone(), self.clock.now_ms());
        let bytes = i64::try_from(record.bytes).unwrap_or(i64::MAX);
        tauri::async_runtime::spawn(async move {
            let written = db
                .write(move |tx| {
                    voice_store::log_outbound(
                        tx,
                        at,
                        &destination,
                        &record.purpose,
                        bytes,
                        record.ref_id.as_deref(),
                    )?;
                    Ok(((), vec![]))
                })
                .await;
            if let Err(e) = written {
                tracing::warn!(code = ?e.code, "couldn't write the privacy log");
            }
        });
    }
}

/// Sends every transcription-job update to the windows as `meeting://job`.
struct AppJobEvents(AppHandle);

impl JobEvents for AppJobEvents {
    fn job(&self, job: &TranscriptionJob) {
        emit(&self.0, events::MEETING_JOB, job);
        // A call recorded by auto mode is written into its meeting once it is transcribed.
        crate::auto_meeting::job_update(&self.0, job);
    }
}

/// Sends the runtime's reports to the windows (`voice://status`, `voice://transcript`,
/// `voice://error`).
struct AppRuntimeEvents(AppHandle);

impl RuntimeEvents for AppRuntimeEvents {
    fn status(&self, status: &VoiceStatus) {
        emit(&self.0, events::STATUS, status);
        // Dictation is over as soon as the turn is no longer listening (second press, error, off).
        if status.state != VoiceState::Listening {
            emit(&self.0, "voice://dictation", &false);
        }
        // The native microphone (beta) serves a turn the always-on microphone does not.
        use tauri::Manager;
        if let Some(shell) = self.0.try_state::<VoiceShell>() {
            match (&status.state, &status.session_id) {
                (VoiceState::Listening, Some(session)) if !shell.native.serving_wake() => {
                    crate::native_mic::start_if_enabled(&self.0, session.clone(), false);
                }
                (VoiceState::Listening, _) => {}
                _ => shell.native.stop_turn(),
            }
        }
    }

    fn transcript(&self, _session_id: &str, t: &RtTranscript) {
        let segment = TranscriptSegment {
            id: pluto_core::ids::new_id(),
            meeting_id: None,
            source: SegmentSource::Realtime,
            index: t.segment_index,
            start_ms: None,
            end_ms: None,
            speaker_label: None,
            participant: None,
            text: t.text.clone(),
            language: t.language.clone(),
            relevance: None,
        };
        emit(&self.0, events::TRANSCRIPT, &segment);
    }

    fn error(&self, error: &VoiceError) {
        emit(&self.0, events::ERROR, error);
    }
}

/// Sends assistant turns and proposals to the windows (`voice://turn`, `voice://proposal`).
struct AppAssistantEvents(AppHandle);

/// Whether the dashboard is open in front. When it isn't (you are in another app, or it is in
/// the tray), answers and approval questions come to the pet instead.
fn dashboard_in_front(app: &AppHandle) -> bool {
    use tauri::Manager;
    app.get_webview_window("main")
        .is_some_and(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false))
}

/// Approval questions shown on the pet: pet question id → proposal id.
static PET_PROPOSALS: Mutex<Vec<(u64, String)>> = Mutex::new(Vec::new());
static PET_ASK_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1 << 40);

fn pet_proposals() -> std::sync::MutexGuard<'static, Vec<(u64, String)>> {
    PET_PROPOSALS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Takes the pet's question about `proposal_id` back, if there is one.
fn withdraw_pet_ask(app: &AppHandle, proposal_id: &str) {
    let ids: Vec<u64> = {
        let mut list = pet_proposals();
        let ids = list
            .iter()
            .filter(|(_, p)| p == proposal_id)
            .map(|(i, _)| *i)
            .collect();
        list.retain(|(_, p)| p != proposal_id);
        ids
    };
    for id in ids {
        let _ = app.emit("pet://ask-done", id);
    }
}

fn pet_say(app: &AppHandle, text: &str, ms: u32) {
    let _ = app.emit(
        "pet://say",
        serde_json::json!({"text": text, "pose": "wave", "ms": ms}),
    );
}

impl AssistantEvents for AppAssistantEvents {
    fn turn(&self, turn: &AssistantTurn) {
        emit(&self.0, events::TURN, turn);
        let waiting = turn.proposal.is_some() && turn.result.is_none();
        // Resolved somewhere (voice, the dashboard, the pet): the pet's question goes away.
        if let (Some(p), true) = (&turn.proposal, turn.result.is_some()) {
            withdraw_pet_ask(&self.0, &p.id);
        }
        // Away from the dashboard the answer is shown on the pet, since you may not be looking at it.
        if !waiting && !dashboard_in_front(&self.0) {
            if let Some(text) = turn.reply_text.as_deref().filter(|t| !t.is_empty()) {
                pet_say(&self.0, text, 7000);
            }
        }
    }

    fn proposal(&self, proposal: &Proposal) {
        emit(&self.0, events::PROPOSAL, proposal);
        if dashboard_in_front(&self.0) {
            return;
        }
        // The same question on the pet, answerable by a click or by saying "Pluto, haan".
        let id = PET_ASK_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        pet_proposals().push((id, proposal.id.clone()));
        let _ = self.0.emit(
            "pet://ask",
            serde_json::json!({
                "id": id,
                "title": "Approve this change?",
                "detail": proposal.summary_text,
                "options": ["Approve", "Reject"],
                "kind": "proposal",
            }),
        );
        // The proposal lasts 30 s; so does the question.
        let (app, pid) = (self.0.clone(), proposal.id.clone());
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(31)).await;
            withdraw_pet_ask(&app, &pid);
        });
    }

    /// "Meetings kholo": the dashboard's existing `page:<name>` command (also used by the pet's
    /// menu) brings the window up and switches the page.
    fn navigate(&self, page: &str) {
        crate::pet::command(&self.0, &format!("page:{page}"));
    }
}

/// A button on the pet's approval question was pressed.
#[tauri::command]
pub async fn pet_proposal_answer(
    id: u64,
    choice: String,
    app: AppHandle,
    shell: State<'_, VoiceShell>,
) -> VoiceResult<()> {
    let proposal_id = pet_proposals()
        .iter()
        .find(|(i, _)| *i == id)
        .map(|(_, p)| p.clone());
    let Some(proposal_id) = proposal_id else {
        return Ok(()); // an old question
    };
    withdraw_pet_ask(&app, &proposal_id);
    let response = if choice == "Approve" {
        ProposalResponse::Approve
    } else {
        ProposalResponse::Reject
    };
    match shell.assistant.respond(&proposal_id, response).await {
        Ok(result) => pet_say(&app, &result.message, 6000),
        Err(e) => pet_say(&app, &e.message, 6000),
    }
    Ok(())
}

/// What the voice shell keeps between calls. Managed by Tauri (`app.manage`).
pub(crate) struct VoiceShell {
    pub(crate) router: Arc<AudioRouter>,
    secrets: Arc<dyn SecretStore>,
    tts: Arc<TimbreClient>,
    /// The shared Gnani client (one request in flight per family) and privacy log, also used by
    /// meeting transcription (T-201).
    http: Arc<GnaniHttp>,
    log: Arc<dyn OutboundLog>,
    /// The state machine: push-to-talk, Realtime, barge-in (T-301).
    pub(crate) runtime: Arc<VoiceRuntime>,
    /// Tools, proposals and the approval gate (T-303).
    assistant: Arc<Assistant>,
    /// Stub settings until the settings service keys land (T-105 / T-401). Shared with the
    /// runtime, which reads the language and format for each press.
    settings: Arc<Mutex<VoiceSettings>>,
    data_dir: PathBuf,
    app: AppHandle,
    /// The native microphone (beta); the web view captures otherwise.
    pub(crate) native: crate::native_mic::NativeMic,
    inner: Mutex<Inner>,
}

struct Inner {
    /// The mic-check recording (Cloud voice off) and its status.
    check: Option<(String, VoiceStatus)>,
    /// Always-on listening (T-705): its microphone session and the detection loop.
    wake: Option<(String, tauri::async_runtime::JoinHandle<()>)>,
    /// A "Pluto" being recorded while teaching.
    enroll: Option<(String, Arc<EnrollSink>)>,
    /// Teaching is open: always-on pauses, so saying "Pluto" doesn't start turns.
    teaching: bool,
}

const SETTINGS_FILE: &str = "voice-settings.json";

/// The saved voice settings, or the built-in defaults when there are none yet or the file is
/// unreadable (it is then left alone and overwritten by the next change). Key flags are not stored:
/// they come from the keychain.
fn load_settings(data_dir: &std::path::Path) -> VoiceResult<VoiceSettings> {
    let saved = std::fs::read_to_string(data_dir.join(SETTINGS_FILE))
        .ok()
        .and_then(|text| serde_json::from_str::<VoiceSettings>(&text).ok());
    match saved {
        Some(s) => Ok(VoiceSettings {
            has_gnani_key: false,
            has_gemini_key: false,
            has_groq_key: false,
            ..s
        }),
        None => fixture(fixtures::SETTINGS_DEFAULT),
    }
}

/// Writes the settings next to the database (written to a temporary file first, so a crash never
/// leaves half a file). A failure is logged: the change still applies for this session.
fn save_settings(data_dir: &std::path::Path, s: &VoiceSettings) {
    let saved = VoiceSettings {
        has_gnani_key: false,
        has_gemini_key: false,
        has_groq_key: false,
        ..s.clone()
    };
    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(data_dir)?;
        let tmp = data_dir.join(format!("{SETTINGS_FILE}.tmp"));
        std::fs::write(&tmp, serde_json::to_vec_pretty(&saved)?)?;
        std::fs::rename(&tmp, data_dir.join(SETTINGS_FILE))
    };
    if let Err(error) = write() {
        tracing::warn!(%error, "could not save the voice settings");
    }
}

/// Should Pluto be listening for "Pluto" right now? Only when the person turned always-on on,
/// Cloud voice is on with a Gnani key, and a teaching session is not using the microphone.
fn wake_wanted(always_on: bool, cloud_voice: bool, has_gnani_key: bool, teaching: bool) -> bool {
    always_on && cloud_voice && has_gnani_key && !teaching
}

fn lock_settings(s: &Mutex<VoiceSettings>) -> MutexGuard<'_, VoiceSettings> {
    s.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Prisma Realtime settings for a press, from the voice settings: the one chosen language, or
/// `indic-auto` when none or several are chosen (the live service refuses a comma-separated list,
/// seen live on 2026-11-06; Batch accepts one), and the output format.
fn rt_config(settings: &VoiceSettings) -> RtConfig {
    let langs: Vec<&str> = settings
        .languages
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    RtConfig {
        language_code: match langs.as_slice() {
            [one] => (*one).to_string(),
            _ => "indic-auto".into(),
        },
        format: settings.stt_format,
        ..RtConfig::default()
    }
}

/// Development builds also save every press to `<data>/voice-check/last-ptt.wav`, so what Pluto
/// actually received can be played back when a turn hears nothing. Release builds save nothing.
fn check_sinks(data_dir: PathBuf) -> impl Fn(&str) -> Vec<Arc<dyn FrameSink>> + Send + Sync {
    if cfg!(debug_assertions) {
        eprintln!(
            "[voice] dev build: each press is saved to {}",
            data_dir.join("voice-check").join("last-ptt.wav").display()
        );
    }
    move |_session| {
        if !cfg!(debug_assertions) {
            return Vec::new();
        }
        match ptt_wav(&data_dir).and_then(WavSink::create) {
            Ok(wav) => vec![Arc::new(wav) as Arc<dyn FrameSink>],
            Err(e) => {
                tracing::debug!(code = ?e.code, "couldn't open the voice-check WAV");
                Vec::new()
            }
        }
    }
}

impl VoiceShell {
    pub(crate) fn new(
        app: AppHandle,
        db: Arc<Database>,
        clock: Arc<dyn Clock>,
        data_dir: PathBuf,
    ) -> VoiceResult<Self> {
        let secrets: Arc<dyn SecretStore> = Arc::new(KeychainSecrets);
        let keys: Arc<dyn ApiKeySource> = Arc::new(KeychainKey(secrets.clone()));
        let log: Arc<dyn OutboundLog> = Arc::new(DbLog {
            db: db.clone(),
            clock: clock.clone(),
        });
        // Cloud voice off → nothing leaves the device (T-PR-04): every client checks this switch.
        let cloud = cloud_switch();
        let http = Arc::new(
            GnaniHttp::new(DEFAULT_BASE_URL, keys.clone(), log.clone())?.with_cloud(cloud.clone()),
        );
        let tts = Arc::new(TimbreClient::new(
            DEFAULT_WS_URL,
            http.clone(),
            keys.clone(),
            log.clone(),
        ));
        // Last session's choices (cloud voice, voice, languages, hotkey) come back after a restart.
        let settings = Arc::new(Mutex::new(load_settings(&data_dir)?));
        let router = Arc::new(AudioRouter::new());
        let stt = Arc::new(
            RealtimeClient::new(realtime::DEFAULT_URL, keys, log.clone()).with_cloud(cloud.clone()),
        );
        cloud.set(lock_settings(&settings).cloud_voice_enabled);
        let enabled = lock_settings(&settings).cloud_voice_enabled
            && secrets.has(Account::Gnani).unwrap_or(false);
        let gemini = GeminiClient::new(gemini::DEFAULT_MODEL, secrets.clone(), log.clone());
        let groq = GroqClient::new(groq::DEFAULT_MODEL, secrets.clone(), log.clone());
        // Nothing is sent while Cloud voice is off (T-PR-04): the decider and the meeting
        // knowledge share one gated router.
        let llm = Arc::new(CloudGated::new(
            Arc::new(Router::new(Arc::new(gemini), Arc::new(groq))),
            cloud,
        ));
        let knowledge = Arc::new(pluto_intel::meeting::service::MeetingAi::new(
            db.clone(),
            clock.clone(),
            llm.clone(),
        ));
        let assistant = Arc::new(
            Assistant::new(
                db,
                clock,
                Arc::new(DefaultDecider::new(llm)),
                Arc::new(AppAssistantEvents(app.clone())),
            )
            .with_knowledge(knowledge),
        );
        let handler: Arc<dyn TurnHandler> = Arc::new(assistant.clone());
        let for_config = settings.clone();
        let runtime = Arc::new(
            VoiceRuntime::new(
                stt,
                router.clone(),
                Arc::new(AppRuntimeEvents(app.clone())),
                handler,
                enabled,
            )
            .with_config(move || rt_config(&lock_settings(&for_config)))
            .with_extra_sinks(check_sinks(data_dir.clone()))
            .with_typer(Arc::new(crate::dictation::OsTyper))
            .with_handle({
                let tauri::async_runtime::RuntimeHandle::Tokio(h) = tauri::async_runtime::handle();
                h
            }),
        );
        assistant.attach(&runtime);
        // Replies saved by an older version may hold the user's own words: cleared once.
        pluto_voice::reply_cache::migrate(&data_dir.join("tts-cache"));
        Ok(Self {
            router,
            secrets,
            tts,
            http,
            log,
            runtime,
            assistant,
            settings,
            data_dir,
            app,
            native: crate::native_mic::NativeMic::default(),
            inner: Mutex::new(Inner {
                check: None,
                wake: None,
                enroll: None,
                teaching: false,
            }),
        })
    }

    /// Meeting transcription jobs on the shared Gnani client; updates go out as `meeting://job`.
    pub(crate) fn meeting_jobs(
        &self,
        app: AppHandle,
        db: Arc<Database>,
        clock: Arc<dyn Clock>,
        audio_dir: std::path::PathBuf,
    ) -> VoiceResult<Arc<MeetingJobs>> {
        let api = Arc::new(BatchClient::new(self.http.clone(), self.log.clone())?);
        Ok(Arc::new(MeetingJobs::new(
            db,
            clock,
            api,
            audio_dir,
            Arc::new(AppJobEvents(app)),
        )))
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The status the UI should show: the mic check's while it runs, else the runtime's.
    fn status(&self) -> VoiceStatus {
        match &self.lock().check {
            Some((_, status)) => status.clone(),
            None => self.runtime.status(),
        }
    }

    /// Is Cloud voice on, and is there a Gnani key? What recording a call needs.
    pub(crate) fn recording_readiness(&self) -> (bool, bool) {
        let s = self.settings();
        (s.cloud_voice_enabled, s.has_gnani_key)
    }

    /// Turns Cloud voice on (the person agreed in a question on the pet) and saves it.
    pub(crate) fn enable_cloud_voice(&self) {
        lock_settings(&self.settings).cloud_voice_enabled = true;
        self.refresh_enabled();
        save_settings(&self.data_dir, &lock_settings(&self.settings));
    }

    /// The speech engine, for saving a whole file (the recap as mp3).
    pub(crate) fn tts_client(&self) -> Arc<TimbreClient> {
        self.tts.clone()
    }

    /// The app handle, for events from code that only has the shell.
    pub(crate) fn app(&self) -> &AppHandle {
        &self.app
    }

    /// Where fixed spoken replies are saved on this computer.
    fn reply_cache_dir(&self) -> PathBuf {
        self.data_dir.join("tts-cache")
    }

    /// "Delete my data": saved reply audio goes too.
    pub(crate) fn clear_reply_cache(&self) {
        pluto_voice::reply_cache::clear(&self.reply_cache_dir());
    }

    /// Cloud voice on and a Gnani key saved → the runtime is ready; otherwise Disabled.
    fn refresh_enabled(&self) {
        let cloud = lock_settings(&self.settings).cloud_voice_enabled;
        cloud_switch().set(cloud);
        let enabled = cloud && self.has_key(Account::Gnani);
        self.runtime.set_enabled(enabled);
        self.refresh_wake();
    }

    // ---- always-on listening (T-705) ---------------------------------------------------------

    fn wake_store(&self) -> WakeStore {
        WakeStore::new(self.data_dir.join("wake"))
    }

    /// Tells the UI the current wake status (`voice://wake`) and returns it. Teaching changes
    /// whether always-on can be turned on even when listening itself doesn't start or stop.
    pub(crate) fn announce_wake(&self) -> WakeStatus {
        let status = self.wake_status();
        emit(&self.app, events::WAKE, &status);
        status
    }

    pub(crate) fn wake_status(&self) -> WakeStatus {
        let store = self.wake_store();
        WakeStatus {
            enrolled: store.enrolled(),
            clips: store.clips(),
            needed: wake::NEEDED,
            session_id: self.lock().wake.as_ref().map(|(id, _)| id.clone()),
        }
    }

    /// Listens for "Pluto" when always-on is on, the wake word is taught and Cloud voice is
    /// ready; otherwise stops. Tells the UI (`voice://wake`) when that changes.
    pub(crate) fn refresh_wake(&self) {
        let (always_on, cloud) = {
            let s = lock_settings(&self.settings);
            (s.always_on_enabled, s.cloud_voice_enabled)
        };
        let want = wake_wanted(
            always_on,
            cloud,
            self.has_key(Account::Gnani),
            self.lock().teaching,
        );
        let store = self.wake_store();
        let running = self.lock().wake.is_some();
        if want && !running {
            let Some(reference) = store.reference() else {
                return;
            };
            let detector = match PlutoDetector::new(reference) {
                Ok(d) => d,
                Err(e) => {
                    tracing::warn!(code = ?e.code, "couldn't start always-on listening");
                    return;
                }
            };
            let session = pluto_core::ids::new_id();
            // About 4 s of audio queued at most; the detector keeps up with far more.
            let (tx, rx) = tokio::sync::mpsc::channel(128);
            if self
                .router
                .open(&session, vec![Arc::new(WakeSink(tx))])
                .is_err()
            {
                return;
            }
            let turns = Arc::new(RuntimeTurns {
                runtime: self.runtime.clone(),
                router: self.router.clone(),
            });
            let task = tauri::async_runtime::spawn(wake::run(
                rx,
                Box::new(detector),
                turns,
                WakePolicy::default(),
            ));
            self.lock().wake = Some((session.clone(), task));
            crate::native_mic::start_if_enabled(&self.app, session, true);
            #[cfg(debug_assertions)]
            eprintln!("[voice] wake: always-on is listening for Pluto");
        } else if !want && running {
            if let Some((session, task)) = self.lock().wake.take() {
                self.native.stop(Some(&session));
                self.router.close(&session);
                task.abort();
                #[cfg(debug_assertions)]
                eprintln!("[voice] wake: always-on stopped");
            }
        } else {
            return;
        }
        emit(&self.app, events::WAKE, &self.wake_status());
    }

    /// Push-to-talk down (hotkey or button). Returns the session the UI captures for.
    pub(crate) fn ptt_down(&self) -> VoiceResult<String> {
        if self.runtime.state() != VoiceState::Disabled {
            return self.runtime.ptt_down();
        }
        // Cloud voice off: record the press locally for the mic check (T-011).
        if let Some((open, _)) = &self.lock().check {
            return Ok(open.clone());
        }
        let id = pluto_core::ids::new_id();
        let wav: Arc<dyn FrameSink> = Arc::new(WavSink::create(ptt_wav(&self.data_dir)?)?);
        self.router.open(&id, vec![wav])?;
        let status = VoiceStatus {
            mode: Some(VoiceMode::Assistant),
            session_id: Some(id.clone()),
            ..VoiceStatus::new(VoiceState::Listening)
        };
        self.lock().check = Some((id.clone(), status.clone()));
        emit(&self.app, events::STATUS, &status);
        crate::native_mic::start_if_enabled(&self.app, id.clone(), false);
        Ok(id)
    }

    /// Push-to-talk up.
    pub(crate) fn ptt_up(&self) {
        let check = self.lock().check.take();
        let Some((id, _)) = check else {
            self.runtime.ptt_up();
            return;
        };
        self.native.stop(Some(&id));
        if let Some((summary, errors)) = self.router.close(&id) {
            tracing::info!(
                frames = summary.frames,
                ms = summary.duration_ms,
                "push-to-talk recorded for the mic check"
            );
            for (_, e) in errors {
                emit(&self.app, events::ERROR, &e);
            }
        }
        emit(&self.app, events::STATUS, &self.runtime.status());
    }

    fn cancel(&self) {
        let check = self.lock().check.take();
        if let Some((id, _)) = check {
            self.router.close(&id);
        }
        self.runtime.stop_speaking();
        self.runtime.cancel();
        emit(&self.app, events::STATUS, &self.runtime.status());
    }

    fn has_key(&self, account: Account) -> bool {
        self.secrets.has(account).unwrap_or(false)
    }

    pub(crate) fn settings(&self) -> VoiceSettings {
        let mut s = lock_settings(&self.settings).clone();
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

// ---- always-on listening (T-705) -------------------------------------------------------------

#[tauri::command]
pub async fn voice_wake_status(shell: State<'_, VoiceShell>) -> VoiceResult<WakeStatus> {
    Ok(shell.wake_status())
}

/// Starts teaching Pluto the user's "Pluto" from scratch: earlier recordings and the old wake
/// word are removed, and always-on pauses until teaching ends.
#[tauri::command]
pub async fn voice_wake_teach_begin(shell: State<'_, VoiceShell>) -> VoiceResult<WakeStatus> {
    shell.lock().teaching = true;
    shell.refresh_wake();
    shell.wake_store().reset()?;
    Ok(shell.announce_wake())
}

/// Opens one teaching recording. The UI sends the microphone's frames for the returned session.
#[tauri::command]
pub async fn voice_wake_clip_start(shell: State<'_, VoiceShell>) -> VoiceResult<String> {
    let session = pluto_core::ids::new_id();
    let sink = Arc::new(EnrollSink::default());
    shell.router.open(&session, vec![sink.clone()])?;
    if let Some((old, _)) = shell.lock().enroll.replace((session.clone(), sink)) {
        shell.router.close(&old);
    }
    Ok(session)
}

/// Ends a teaching recording: keeps it if it is one clear "Pluto", or says what to do.
#[tauri::command]
pub async fn voice_wake_clip_stop(
    session_id: String,
    shell: State<'_, VoiceShell>,
) -> VoiceResult<WakeClip> {
    let taken = {
        let mut inner = shell.lock();
        match inner.enroll.take() {
            Some((id, sink)) if id == session_id => Some(sink),
            other => {
                inner.enroll = other;
                None
            }
        }
    };
    let Some(sink) = taken else {
        return Err(VoiceError::new(
            VoiceErrorCode::Core,
            "That recording already ended.",
        ));
    };
    shell.router.close(&session_id);
    let store = shell.wake_store();
    Ok(match wake::take_clip(&sink.take()) {
        Ok(word) => WakeClip {
            ok: true,
            message: "Got it.".into(),
            clips: store.add_clip(&word)?,
        },
        Err(why) => WakeClip {
            ok: false,
            message: why.into(),
            clips: store.clips(),
        },
    })
}

/// Builds the wake word from the recordings and starts listening if always-on is on.
#[tauri::command]
pub async fn voice_wake_teach_finish(shell: State<'_, VoiceShell>) -> VoiceResult<WakeStatus> {
    shell.wake_store().build()?;
    shell.lock().teaching = false;
    shell.refresh_wake();
    Ok(shell.announce_wake())
}

/// Stops teaching without a wake word, or forgets the one saved: recordings and reference are
/// deleted, and always-on stops.
#[tauri::command]
pub async fn voice_wake_forget(shell: State<'_, VoiceShell>) -> VoiceResult<WakeStatus> {
    if let Some((id, _)) = shell.lock().enroll.take() {
        shell.router.close(&id);
    }
    shell.wake_store().reset()?;
    shell.lock().teaching = false;
    shell.refresh_wake();
    Ok(shell.announce_wake())
}

/// Settings → Cloud voice, shared by every outbound client: Gnani (REST, Batch, Realtime, Timbre)
/// and the LLMs, for the assistant and for meetings. Off until the settings say otherwise.
pub(crate) fn cloud_switch() -> Arc<CloudSwitch> {
    static CLOUD: OnceLock<Arc<CloudSwitch>> = OnceLock::new();
    CLOUD.get_or_init(|| CloudSwitch::new(false)).clone()
}

/// Where push-to-talk presses are recorded for the mic check (T-011).
fn ptt_wav(data_dir: &std::path::Path) -> VoiceResult<PathBuf> {
    let dir = data_dir.join("voice-check");
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
    let hotkey_changed = {
        let mut s = lock_settings(&shell.settings);
        let before = s.hotkey.clone();
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
        s.hotkey != before
    };
    shell.refresh_enabled();
    save_settings(&shell.data_dir, &lock_settings(&shell.settings));
    if hotkey_changed {
        crate::shortcuts::reapply(&shell.app);
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
    shell.refresh_enabled();
    Ok(())
}

#[tauri::command]
pub async fn voice_key_clear(account: String, shell: State<'_, VoiceShell>) -> VoiceResult<()> {
    shell.secrets.clear(self::account(&account)?)?;
    shell.refresh_enabled();
    Ok(())
}

// ---- runtime -------------------------------------------------------------------------------

#[tauri::command]
pub async fn voice_status(shell: State<'_, VoiceShell>) -> VoiceResult<VoiceStatus> {
    Ok(shell.status())
}

/// Push-to-talk down (the talk button; the global hotkey does the same, see [`hotkey`]). Shows
/// Listening at once and returns the session id for `audio_push`. While Pluto speaks, this stops
/// it (barge-in); while a meeting records, it answers `Busy`. With Cloud voice off the press is
/// only recorded to `<data>/voice-check/last-ptt.wav` (mic check, T-011).
#[tauri::command]
pub async fn voice_ptt_start(shell: State<'_, VoiceShell>) -> VoiceResult<String> {
    shell.ptt_down()
}

/// Push-to-talk up: Realtime gets `commit`, and the final words go to the assistant (Thinking).
#[tauri::command]
pub async fn voice_ptt_stop(shell: State<'_, VoiceShell>) -> VoiceResult<()> {
    shell.ptt_up();
    Ok(())
}

#[tauri::command]
pub async fn voice_cancel(shell: State<'_, VoiceShell>) -> VoiceResult<()> {
    shell.cancel();
    Ok(())
}

/// The global push-to-talk hotkey (`shortcuts.rs`): press opens, release commits. An error goes
/// to `voice://error`, as nobody awaits a hotkey.
pub(crate) fn hotkey(app: &AppHandle, pressed: bool) {
    use tauri::Manager;
    let Some(shell) = app.try_state::<VoiceShell>() else {
        return;
    };
    in_runtime(|| {
        if pressed {
            if let Err(e) = shell.ptt_down() {
                if e.code != VoiceErrorCode::Busy {
                    // Busy was already reported by the runtime.
                    emit(app, events::ERROR, &e);
                }
            }
        } else {
            shell.ptt_up();
        }
    });
}

/// Runs `f` inside Tauri's async runtime. A global hotkey is reported on the operating system's
/// own thread, where there is no runtime; starting a voice turn spawns tasks and panics without one
/// (which closed the app on Ctrl+Alt+D).
fn in_runtime<R>(f: impl FnOnce() -> R) -> R {
    let tauri::async_runtime::RuntimeHandle::Tokio(handle) = tauri::async_runtime::handle();
    let _guard = handle.enter();
    f()
}

/// Where the hands-free key and the dictation key are, until they become settings: the first
/// press starts, a pause (hands-free) or the second press (dictation) ends.
pub(crate) const HANDS_FREE_HOTKEY: &str = "Ctrl+Alt+Space";
pub(crate) const DICTATION_HOTKEY: &str = "Ctrl+Alt+D";

/// Hands-free from the Talk button (native microphone): starts a turn that ends after a pause,
/// or ends the one that is running. Returns the session when one started.
#[tauri::command]
pub async fn voice_hands_free_toggle(shell: State<'_, VoiceShell>) -> VoiceResult<Option<String>> {
    shell.runtime.hands_free_toggle()
}

/// The hands-free hotkey: press once and talk; the command ends after a pause.
pub(crate) fn hotkey_hands_free(app: &AppHandle) {
    toggle_turn(app, false);
}

/// The dictation hotkey: press to start typing what you say, press again to stop.
pub(crate) fn hotkey_dictation(app: &AppHandle) {
    toggle_turn(app, true);
}

fn toggle_turn(app: &AppHandle, dictation: bool) {
    // A hotkey must never take the app down: whatever goes wrong here is logged and ignored.
    let app = app.clone();
    let name = if dictation { "dictation" } else { "hands-free" };
    #[cfg(debug_assertions)]
    eprintln!("[voice] hotkey: {name} pressed");
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        toggle_turn_inner(&app, dictation)
    }))
    .is_err()
    {
        tracing::error!(name, "the voice hotkey panicked; ignored");
        #[cfg(debug_assertions)]
        eprintln!("[voice] hotkey: {name} panicked (ignored)");
    }
}

fn toggle_turn_inner(app: &AppHandle, dictation: bool) {
    use tauri::Manager;
    let Some(shell) = app.try_state::<VoiceShell>() else {
        return;
    };
    let result = if dictation {
        shell.runtime.dictation_toggle()
    } else {
        shell.runtime.hands_free_toggle()
    };
    match result {
        Ok(started) => {
            if dictation {
                emit(app, "voice://dictation", &started.is_some());
            }
        }
        Err(e) => {
            if e.code != VoiceErrorCode::Busy {
                emit(app, events::ERROR, &e);
            }
        }
    }
}

/// The push-to-talk hotkey from the voice settings.
pub(crate) fn hotkey_combo(app: &AppHandle) -> Option<String> {
    use tauri::Manager;
    let shell = app.try_state::<VoiceShell>()?;
    let combo = lock_settings(&shell.settings).hotkey.trim().to_string();
    (!combo.is_empty()).then_some(combo)
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

/// The answer to a proposal (the card's Approve / Reject / Edit). The **only** path from a
/// proposal to a write: the store's gate lets it run once, and only inside its 30 s window.
#[tauri::command]
pub async fn voice_proposal_respond(
    proposal_id: String,
    response: ProposalResponse,
    shell: State<'_, VoiceShell>,
) -> VoiceResult<ToolResult> {
    shell.assistant.respond(&proposal_id, response).await
}

/// Speaks `text` to `playback` (PCM16 24 kHz chunks), reporting Speaking then Idle. With Cloud
/// voice on it is Timbre in the settings' voice; with it off, a local 1 s test tone (nothing is
/// sent anywhere). Returns when speech ends or `voice_stop_speaking` cuts it off. A Timbre failure
/// is returned and emitted as `voice://error` (the UI shows the reply as text instead, B5.7).
#[tauri::command]
pub async fn voice_speak(
    text: String,
    language: Option<String>,
    playback: Channel<InvokeResponseBody>,
    app: AppHandle,
    shell: State<'_, VoiceShell>,
) -> VoiceResult<()> {
    let settings = shell.settings();
    let request = TtsRequest {
        // The screen keeps dates short ("Tue, 10 Nov"); the voice says them in full.
        text: pluto_voice::speakable::speakable(&text),
        language: language.unwrap_or_else(|| "auto".into()),
        // Only a Timbre v3.0 voice works; an unknown name (the old "Arya" default) falls back.
        voice: Some(settings.tts_voice.trim().to_string())
            .filter(|v| pluto_gnani::tts::VOICES_V3.contains(&v.as_str())),
        // The client's documented default until the voice settings are real (T-105 / T-401).
        model: None,
        speed: None,
    };
    let tts: Arc<dyn TextToSpeech> = if settings.cloud_voice_enabled {
        // Fixed replies ("Opening Meetings.") play from this computer after the first time.
        Arc::new(pluto_voice::reply_cache::CachedTts::new(
            shell.tts.clone(),
            shell.reply_cache_dir(),
        ))
    } else {
        Arc::new(pluto_gnani::mock::MockTts::new())
    };
    let task = tokio::spawn(async move {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(16);
        let produce = tts.stream(request, tx);
        let forward = async {
            while let Some(chunk) = rx.recv().await {
                if playback.send(InvokeResponseBody::Raw(chunk)).is_err() {
                    break; // the page went away
                }
            }
        };
        let (spoken, ()) = tokio::join!(produce, forward);
        spoken
    });
    // Speaking → Idle through the runtime; a press stops it (barge-in), and a newer reply
    // replaces this one.
    let id = shell.runtime.speaking(task.abort_handle());
    // A cancellation (voice_stop_speaking / voice_cancel / barge-in) is not an error for the caller.
    let spoken = task.await.unwrap_or(Ok(()));
    shell.runtime.speaking_done(id);
    if let Err(e) = &spoken {
        emit(&app, events::ERROR, e);
    }
    spoken
}

#[tauri::command]
pub async fn voice_stop_speaking(shell: State<'_, VoiceShell>) -> VoiceResult<()> {
    shell.runtime.stop_speaking();
    Ok(())
}

// ---- history, usage, privacy ---------------------------------------------------------------

/// Stub: the sample counters. T-506 counts from the privacy log.
#[tauri::command]
pub async fn voice_usage() -> VoiceResult<UsageCounters> {
    fixture(fixtures::USAGE_SAMPLE)
}

/// The assistant's turns, newest first: this run's (with replies) and, after them, older ones from
/// the log (what was said, what was decided; replies are not kept).
#[tauri::command]
pub async fn voice_history(
    limit: u32,
    shell: State<'_, VoiceShell>,
    core: State<'_, Core>,
) -> VoiceResult<Vec<AssistantTurn>> {
    let limit = limit.clamp(1, 200);
    let mut turns = shell.assistant.history(limit as usize);
    if turns.len() < limit as usize {
        let rows = core
            .db
            .read(move |c| voice_store::recent_interactions(c, limit))
            .await?;
        for r in rows {
            if turns.iter().any(|t| t.interaction_id == r.id) {
                continue;
            }
            turns.push(past_turn(r));
            if turns.len() >= limit as usize {
                break;
            }
        }
    }
    Ok(turns)
}

/// An older turn rebuilt from its log row.
fn past_turn(r: voice_store::InteractionRow) -> AssistantTurn {
    use pluto_contracts::decide::{AssistantDecision, DecisionSource, Intent};
    let intent = r
        .intent
        .as_deref()
        .and_then(|i| serde_json::from_value::<Intent>(serde_json::Value::String(i.into())).ok())
        .unwrap_or(Intent::Noise);
    let source = if r.decision_source.as_deref() == Some("rules") {
        DecisionSource::Rules
    } else {
        DecisionSource::Llm
    };
    let result = r.outcome.as_deref().and_then(|o| {
        let (ok, message) = match o {
            "executed" => (true, "Done"),
            "rejected" => (false, "You said no"),
            "expired" => (false, "It ran out of time"),
            "failed" => (false, "It didn't work"),
            "cancelled" => (false, "Cancelled"),
            _ => return None,
        };
        Some(pluto_contracts::voice::ToolResult {
            tool: r.tool.clone().unwrap_or_default(),
            ok,
            message: message.into(),
            record_id: None,
        })
    });
    AssistantTurn {
        interaction_id: r.id,
        transcript: r.transcript,
        language: r.language,
        decision: AssistantDecision {
            intent,
            tool: r.tool,
            args: r
                .args_json
                .and_then(|a| serde_json::from_str(&a).ok())
                .unwrap_or_else(|| serde_json::json!({})),
            confidence: None,
            source,
            latency_ms: u32::try_from(r.llm_latency_ms.unwrap_or(0)).unwrap_or(0),
        },
        reply_text: None,
        proposal: None,
        result,
    }
}

/// Forgets the history shown on the Voice page and deletes its log. A change still waiting for
/// your answer is kept.
#[tauri::command]
pub async fn voice_history_clear(
    shell: State<'_, VoiceShell>,
    core: State<'_, Core>,
) -> VoiceResult<()> {
    core.db
        .write(|tx| Ok((voice_store::clear_interactions(tx)?, vec![])))
        .await?;
    shell.assistant.clear_history();
    Ok(())
}

/// A command typed instead of spoken. It goes through the same assistant as a spoken one: reads
/// answer at once, changes are proposed and wait for your approval.
#[tauri::command]
pub async fn voice_text_turn(text: String, shell: State<'_, VoiceShell>) -> VoiceResult<()> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Ok(());
    }
    let heard = pluto_voice::runtime::Heard {
        session_id: pluto_core::ids::new_id(),
        text,
        language: None,
    };
    shell.assistant.turn(heard).await.map(|_| ())
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
    #[test]
    fn realtime_gets_one_language_or_automatic_detection() {
        let mut s: VoiceSettings = fixture(fixtures::SETTINGS_DEFAULT).unwrap();
        assert_eq!(rt_config(&s).language_code, "indic-auto");
        s.languages = vec!["hi-IN".into()];
        assert_eq!(rt_config(&s).language_code, "hi-IN");
        // Gnani's live service refuses a list, so several languages mean automatic detection.
        s.languages = vec!["hi-IN".into(), "en-IN".into()];
        assert_eq!(rt_config(&s).language_code, "indic-auto");
    }

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
    fn listening_for_pluto_needs_all_four_things() {
        assert!(wake_wanted(true, true, true, false));
        assert!(!wake_wanted(false, true, true, false), "always-on is off");
        assert!(!wake_wanted(true, false, true, false), "Cloud voice is off");
        assert!(!wake_wanted(true, true, false, false), "no Gnani key");
        assert!(
            !wake_wanted(true, true, true, true),
            "teaching is using the microphone"
        );
    }

    #[test]
    fn voice_settings_survive_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        // Nothing saved yet: the defaults, with cloud voice off.
        let first = load_settings(dir.path()).unwrap();
        assert!(!first.cloud_voice_enabled);

        let changed = VoiceSettings {
            cloud_voice_enabled: true,
            tts_voice: "Avinash".into(),
            hotkey: "Ctrl+Alt+K".into(),
            has_gnani_key: true, // never stored
            ..first
        };
        save_settings(dir.path(), &changed);
        let back = load_settings(dir.path()).unwrap();
        assert!(back.cloud_voice_enabled);
        assert_eq!(back.tts_voice, "Avinash");
        assert_eq!(back.hotkey, "Ctrl+Alt+K");
        assert!(
            !back.has_gnani_key,
            "key flags come from the keychain, not the file"
        );
    }

    #[test]
    fn a_damaged_settings_file_falls_back_to_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(SETTINGS_FILE), "{ not json").unwrap();
        let s = load_settings(dir.path()).unwrap();
        assert!(!s.cloud_voice_enabled);
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
