//! Native microphone for voice (beta, `voice.native_mic`, off by default).
//!
//! By default the web view captures the microphone and streams frames to Rust (`audio_push`).
//! With this switched on, Rust opens the microphone itself (`pluto-mic`) and puts the frames on
//! the same route, so listening continues with the window hidden and the OS asks for the
//! microphone permission on Pluto's behalf. The web view skips its own capture while this runs;
//! if Rust cannot open the microphone it says so (`voice://native-mic-failed`) and the web view
//! takes over for that session.

use std::sync::{Arc, Mutex};

use pluto_core::settings_service::SettingsService;
use pluto_voice::audio::AudioRouter;
use tauri::{AppHandle, Emitter, Manager};

pub const SETTING: &str = "voice.native_mic";
pub const FAILED_EVENT: &str = "voice://native-mic-failed";

struct Running {
    session: String,
    /// Opened for the wake word (stays on) or for one turn (ends with it).
    for_wake: bool,
    _mic: pluto_mic::Mic,
}

#[derive(Default)]
pub struct NativeMic {
    current: Mutex<Option<Running>>,
}

impl NativeMic {
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Running>> {
        self.current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Whether the always-on microphone is the native one (turns then reuse it).
    pub fn serving_wake(&self) -> bool {
        self.lock().as_ref().is_some_and(|r| r.for_wake)
    }

    /// Stops the capture for `session` (or any, with `None`).
    pub fn stop(&self, session: Option<&str>) {
        let mut cur = self.lock();
        if cur
            .as_ref()
            .is_some_and(|r| session.is_none_or(|s| s == r.session))
        {
            *cur = None;
        }
    }

    /// Stops a per-turn capture; the always-on one keeps running.
    pub fn stop_turn(&self) {
        let mut cur = self.lock();
        if cur.as_ref().is_some_and(|r| !r.for_wake) {
            *cur = None;
        }
    }

    fn start(&self, app: &AppHandle, router: Arc<AudioRouter>, session: &str, for_wake: bool) {
        let mut cur = self.lock();
        if cur.as_ref().is_some_and(|r| r.session == session) {
            return;
        }
        *cur = None;
        let id = session.to_owned();
        let sink = id.clone();
        let started = pluto_mic::start(move |frame| {
            // A closed route just drops the frame.
            let _ = router.push(&sink, frame);
        });
        match started {
            Ok(mic) => {
                *cur = Some(Running {
                    session: id,
                    for_wake,
                    _mic: mic,
                });
            }
            Err(e) => {
                tracing::warn!(error = %e, "the native microphone could not start");
                let _ = app.emit(FAILED_EVENT, e.to_string());
            }
        }
    }
}

async fn enabled(app: &AppHandle) -> bool {
    let Some(settings) = app.try_state::<Arc<SettingsService>>() else {
        return false;
    };
    settings
        .get_all()
        .await
        .ok()
        .and_then(|all| all.get(SETTING).and_then(serde_json::Value::as_bool))
        .unwrap_or(false)
}

/// Starts the native microphone for `session` if the beta switch is on.
pub fn start_if_enabled(app: &AppHandle, session: String, for_wake: bool) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if !enabled(&app).await {
            return;
        }
        let Some(shell) = app.try_state::<crate::voice::VoiceShell>() else {
            return;
        };
        shell
            .native
            .start(&app, shell.router.clone(), &session, for_wake);
    });
}
