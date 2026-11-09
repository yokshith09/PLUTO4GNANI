//! Auto mode for Google Meet (the shell side of `pluto_intel::meeting::auto`).
//!
//! Every few seconds, while the person has switched it on (`meetings.auto_offer`), the browser in
//! front is read for a Meet call. When one is found a question appears on the pet. Only a "yes"
//! starts recording: the page captures the microphone (it already does for push-to-talk) and the
//! shell saves it as chunks. When the call ends, or Stop is pressed on the pet, the recording is
//! queued for Gnani, and once it is transcribed the decisions, action items and a short summary
//! are written into the meeting by itself.
//!
//! Unverified on a real PC: reading the browser tab (UI Automation on Windows, AppleScript on
//! macOS) and the microphone capture in the page.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use pluto_contracts::meeting::{JobStatus, TranscriptionJob};
use pluto_core::settings_service::SettingsService;
use pluto_intel::meeting::auto::{self, Action, Answer, AutoMeet, Event, Observation, TabSeen};
use pluto_voice::meeting_jobs::MeetingJobs;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::meeting_ai::{begin_recording, finish_recording, MeetingShell, SystemAudio};
use crate::voice::VoiceShell;
use crate::Core;

/// How often the browser is looked at.
const TICK: Duration = Duration::from_secs(3);

/// What each button of a question means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Choice {
    Record,
    TurnOnAndRecord,
    OpenSettings,
    NotNow,
}

struct PendingAsk {
    id: u64,
    options: Vec<(String, Choice)>,
}

/// The recording auto mode is running.
#[derive(Debug, Clone, Serialize)]
pub struct AutoRec {
    pub session_id: String,
    pub meeting_id: String,
    pub started_at: i64,
}

#[derive(Default)]
struct Inner {
    machine: AutoMeet,
    ask: Option<PendingAsk>,
    recording: Option<AutoRec>,
    /// Meetings recorded by auto mode that are waiting to be transcribed, then written up.
    to_process: HashSet<String>,
}

pub(crate) struct AutoMeetShell {
    inner: Mutex<Inner>,
    next_id: AtomicU64,
}

impl AutoMeetShell {
    pub(crate) fn new() -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
            next_id: AtomicU64::new(1),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Starts the loop that looks for a call. Runs for the life of the app.
pub(crate) fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(TICK).await;
            tick(&app).await;
        }
    });
}

async fn enabled(app: &AppHandle) -> bool {
    let Some(settings) = app.try_state::<Arc<SettingsService>>() else {
        return false;
    };
    settings
        .get_all()
        .await
        .ok()
        .and_then(|all| all.get("meetings.auto_offer").and_then(Value::as_bool))
        .unwrap_or(false)
}

fn observation(tabs: &[pluto_core::sites::OpenTab]) -> Observation {
    let seen: Vec<TabSeen> = tabs
        .iter()
        .map(|t| TabSeen {
            title: t.title.clone(),
            domain: t.domain.clone(),
            active: t.active,
        })
        .collect();
    auto::observe(&seen)
}

async fn tick(app: &AppHandle) {
    let shell = app.state::<AutoMeetShell>();
    if !enabled(app).await {
        // Switched off: no question is left hanging. A recording already running carries on.
        let withdraw = {
            let mut inner = shell.lock();
            let had = inner.ask.take();
            if had.is_some() {
                inner.machine = AutoMeet::new();
            }
            had
        };
        if let Some(ask) = withdraw {
            let _ = app.emit("pet://ask-done", ask.id);
        }
        return;
    }
    let tabs = tauri::async_runtime::spawn_blocking(crate::tabs::read_front_tabs)
        .await
        .unwrap_or_default();
    let action = shell.lock().machine.on(Event::Saw(observation(&tabs)));
    act(app, action).await;
}

/// Carries out what the machine decided.
async fn act(app: &AppHandle, action: Action) {
    match action {
        Action::Nothing => {}
        Action::Offer { code } => offer(app, &code),
        Action::WithdrawOffer => {
            let shell = app.state::<AutoMeetShell>();
            let gone = shell.lock().ask.take();
            if let Some(ask) = gone {
                let _ = app.emit("pet://ask-done", ask.id);
            }
        }
        Action::StartRecording { code } => start_recording(app, &code).await,
        Action::StopRecording => stop_recording(app).await,
    }
}

/// Puts the question on the pet. What it offers depends on what recording needs.
fn offer(app: &AppHandle, code: &str) {
    let voice = app.state::<VoiceShell>();
    let (cloud, key) = voice.recording_readiness();
    let (detail, options) = if cloud && key {
        (
            format!("Meeting {code}. Everyone in it should know it is being recorded."),
            vec![
                ("Record".to_owned(), Choice::Record),
                ("Not now".to_owned(), Choice::NotNow),
            ],
        )
    } else if key {
        (
            "Cloud voice is off. Turn it on to record? The audio is sent to Gnani to be transcribed.".to_owned(),
            vec![("Turn on and record".to_owned(), Choice::TurnOnAndRecord), ("Not now".to_owned(), Choice::NotNow)],
        )
    } else {
        (
            "Add your Gnani key in Settings → Voice & privacy to record meetings.".to_owned(),
            vec![
                ("Open Settings".to_owned(), Choice::OpenSettings),
                ("Not now".to_owned(), Choice::NotNow),
            ],
        )
    };
    let shell = app.state::<AutoMeetShell>();
    let id = shell.next_id.fetch_add(1, Ordering::SeqCst);
    let labels: Vec<String> = options.iter().map(|(l, _)| l.clone()).collect();
    shell.lock().ask = Some(PendingAsk { id, options });
    let _ = app.emit(
        "pet://ask",
        json!({"id": id, "title": "Google Meet is open. Record this meeting?", "detail": detail, "options": labels}),
    );
}

async fn start_recording(app: &AppHandle, code: &str) {
    let voice = app.state::<VoiceShell>();
    let meetings = app.state::<MeetingShell>();
    let started = begin_recording(
        &voice,
        &meetings,
        &format!("Google Meet {code}"),
        vec![],
        false,
        true,
        SystemAudio::BestEffort,
    )
    .await;
    let shell = app.state::<AutoMeetShell>();
    match started {
        Ok((session, with_system)) => {
            let rec = AutoRec {
                session_id: session.id.clone(),
                meeting_id: session.meeting_id.clone(),
                started_at: session.started_at,
            };
            {
                let mut inner = shell.lock();
                inner.to_process.insert(rec.meeting_id.clone());
                inner.recording = Some(rec.clone());
            }
            // The page opens the microphone for this session; the pet shows that it is recording.
            let _ = app.emit_to(
                "main",
                "meeting://auto-capture",
                json!({"session_id": rec.session_id, "on": true}),
            );
            let _ = app.emit("pet://recording", Some(&rec));
            if !with_system {
                say(
                    app,
                    "Recording your microphone only. Other people are heard only if your mic picks them up.",
                    7000,
                );
            }
        }
        Err(e) => {
            let _ = shell.lock().machine.on(Event::StartFailed);
            say(
                app,
                &format!("I couldn't start recording: {}", e.message),
                7000,
            );
        }
    }
}

async fn stop_recording(app: &AppHandle) {
    let shell = app.state::<AutoMeetShell>();
    let Some(rec) = shell.lock().recording.take() else {
        return;
    };
    // The page stops its microphone first, then the recording is closed and queued.
    let _ = app.emit_to(
        "main",
        "meeting://auto-capture",
        json!({"session_id": rec.session_id, "on": false}),
    );
    let _ = app.emit("pet://recording", Option::<&AutoRec>::None);
    tokio::time::sleep(Duration::from_millis(400)).await;
    let voice = app.state::<VoiceShell>();
    let jobs = app.state::<Arc<MeetingJobs>>();
    let meetings = app.state::<MeetingShell>();
    match finish_recording(&voice, &jobs, &meetings, &rec.session_id).await {
        Ok(_) => say(
            app,
            "Meeting saved. Writing it up once it is transcribed.",
            6000,
        ),
        Err(e) => {
            // Nothing was recorded (no audio reached the shell): don't leave an empty meeting.
            shell.lock().to_process.remove(&rec.meeting_id);
            let core = app.state::<Core>();
            let _ = pluto_core::meetings::delete(&core.db, &*core.clock, &rec.meeting_id).await;
            say(app, &format!("Nothing was recorded: {}", e.message), 7000);
        }
    }
}

fn say(app: &AppHandle, text: &str, ms: u32) {
    let _ = app.emit("pet://say", json!({"text": text, "pose": "wave", "ms": ms}));
}

/// A transcription job changed. Once a meeting recorded by auto mode is transcribed, it is written up.
pub(crate) fn job_update(app: &AppHandle, job: &TranscriptionJob) {
    let Some(shell) = app.try_state::<AutoMeetShell>() else {
        return;
    };
    let done = matches!(job.status, JobStatus::Completed | JobStatus::PartialFailure);
    let failed = matches!(job.status, JobStatus::Failed | JobStatus::Cancelled);
    if !done && !failed {
        return;
    }
    if !shell.lock().to_process.remove(&job.meeting_id) {
        // Not an auto-mode meeting: nothing is written without the person's approval. Say it is
        // ready; the review step in Meetings asks before any decision, item or task is saved.
        if done {
            say(
                app,
                "A transcript is ready. Open Meetings to review the action items. Nothing is saved until you approve.",
                8000,
            );
        }
        return;
    }
    let app = app.clone();
    let meeting_id = job.meeting_id.clone();
    if failed {
        say(
            &app,
            "I couldn't transcribe that meeting. You can retry it in Meetings.",
            7000,
        );
        return;
    }
    tauri::async_runtime::spawn(async move {
        let core = app.state::<Core>();
        let meetings = app.state::<MeetingShell>();
        let ai = meetings.ai(&core);
        match ai.auto_process(&meeting_id).await {
            Ok(s) if s.decisions + s.action_items + s.other > 0 => say(
                &app,
                &format!(
                    "Meeting notes are ready: {} decisions, {} action items. See Meetings.",
                    s.decisions, s.action_items
                ),
                8000,
            ),
            Ok(_) => say(
                &app,
                "The meeting is transcribed, but I found no decisions or action items.",
                7000,
            ),
            Err(e) => say(
                &app,
                &format!(
                    "The meeting is transcribed. I couldn't write it up: {}",
                    e.message
                ),
                8000,
            ),
        }
    });
}

// ---- commands from the pet ------------------------------------------------------------------

/// A button on the question was pressed.
#[tauri::command]
pub async fn auto_meeting_answer(id: u64, choice: String, app: AppHandle) -> Result<(), String> {
    let shell = app.state::<AutoMeetShell>();
    let picked = {
        let mut inner = shell.lock();
        match inner.ask.take() {
            Some(ask) if ask.id == id => ask
                .options
                .iter()
                .find(|(l, _)| *l == choice)
                .map(|(_, c)| *c),
            other => {
                inner.ask = other;
                None
            }
        }
    };
    let Some(picked) = picked else {
        return Ok(()); // an old or unknown question
    };
    let answer = match picked {
        Choice::Record => Answer::Record,
        Choice::TurnOnAndRecord => {
            app.state::<VoiceShell>().enable_cloud_voice();
            Answer::Record
        }
        Choice::OpenSettings => {
            crate::tray::show_main(&app);
            let _ = app.emit("app://open-settings", "voice");
            Answer::NotNow
        }
        Choice::NotNow => Answer::NotNow,
    };
    let action = shell.lock().machine.on(Event::Answered(answer));
    act(&app, action).await;
    Ok(())
}

/// The Stop button on the pet.
#[tauri::command]
pub async fn auto_meeting_stop(app: AppHandle) -> Result<(), String> {
    let action = app
        .state::<AutoMeetShell>()
        .lock()
        .machine
        .on(Event::StopPressed);
    act(&app, action).await;
    Ok(())
}

/// What is recording now, so the pet can show it after a reload.
#[tauri::command]
pub fn auto_meeting_state(shell: State<'_, AutoMeetShell>) -> Option<AutoRec> {
    shell.lock().recording.clone()
}

/// The page could not open the microphone: end the recording and say why.
#[tauri::command]
pub async fn auto_meeting_capture_failed(message: String, app: AppHandle) -> Result<(), String> {
    say(
        &app,
        &format!("I couldn't use the microphone: {message}"),
        7000,
    );
    let action = app
        .state::<AutoMeetShell>()
        .lock()
        .machine
        .on(Event::StopPressed);
    act(&app, action).await;
    Ok(())
}
