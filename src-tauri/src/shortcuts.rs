//! System-wide keyboard shortcuts (UI brief §8). They work while Pluto is in the tray or behind
//! other windows. Two can be changed in Settings (`shortcuts.global_open`,
//! `shortcuts.global_new_note`; `null` means the default); they are registered again whenever
//! those settings change. A shortcut another app already owns is skipped with a warning in the
//! log; the rest still work.
//!
//! `Mod+Shift+T` (new task) was removed by the owner on 2026-10-12. Its setting,
//! `shortcuts.global_new_task`, is no longer read.
//!
//! **Push-to-talk** (T-301, the `VoicePtt` action) is the one shortcut that also acts on key
//! **release**: press opens Listening, release commits. Its combination is the voice settings'
//! `hotkey` (default `Ctrl+Shift+Space`), registered with the others and again when it changes.

use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pluto_core::bus::{EventBus, Recv};
use pluto_core::events::Event;
use pluto_core::settings_service::SettingsService;
use serde_json::Value;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// What a shortcut does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    TogglePet,
    ToggleDashboard,
    NewNote,
    /// Talk hands-free: press once, speak, it sends after a pause.
    HandsFree,
    /// Dictation: press to type what you say where the cursor is, press again to stop.
    Dictation,
}

/// Each action, its setting (if it can be changed) and its default.
pub const ACTIONS: [(Action, Option<&str>, &str); 5] = [
    // Off unless chosen in Settings (FR-48): an empty default means "not registered".
    (Action::TogglePet, Some("shortcuts.global_pet"), ""),
    (
        Action::ToggleDashboard,
        Some("shortcuts.global_open"),
        "CmdOrCtrl+Shift+L",
    ),
    (
        Action::NewNote,
        Some("shortcuts.global_new_note"),
        "CmdOrCtrl+Shift+N",
    ),
    (
        Action::HandsFree,
        Some("shortcuts.voice_hands_free"),
        "Ctrl+Alt+Space",
    ),
    (
        Action::Dictation,
        Some("shortcuts.voice_dictation"),
        "Ctrl+Alt+Shift+Space",
    ),
];

/// The combination for each action given the current settings: the setting if it holds a
/// string, otherwise the default.
pub fn combos(settings: &std::collections::BTreeMap<String, Value>) -> Vec<(Action, String)> {
    ACTIONS
        .iter()
        .map(|(action, key, default)| {
            let chosen = key
                .and_then(|k| settings.get(k))
                .and_then(Value::as_str)
                .unwrap_or(default);
            (*action, chosen.to_owned())
        })
        .filter(|(_, combo)| !combo.is_empty())
        .collect()
}

/// A held key repeats: Windows reports "pressed" again every few milliseconds after the first half
/// second. A shortcut that toggles something (dictation, hands-free listening) must act once per
/// real press, or holding the keys a moment longer switches it on and straight off again.
#[derive(Debug, Default)]
pub struct OncePerPress {
    down: bool,
    last: Option<Instant>,
}

impl OncePerPress {
    /// A press where the key was up (or silent for long enough that its release must have been
    /// missed) counts; repeats while it is held do not.
    pub fn accept(&mut self, now: Instant) -> bool {
        let repeat = self.down
            && self
                .last
                .is_some_and(|l| now.duration_since(l) < Duration::from_millis(1500));
        self.down = true;
        self.last = Some(now);
        !repeat
    }

    pub fn released(&mut self) {
        self.down = false;
    }
}

fn run(app: &AppHandle, action: Action) {
    match action {
        Action::TogglePet => crate::pet::toggle_pet(app),
        Action::ToggleDashboard => {
            let visible = app
                .get_webview_window("main")
                .and_then(|w| w.is_visible().ok())
                .unwrap_or(false);
            if visible {
                crate::pet::hide_main(app);
            } else {
                crate::tray::show_main(app);
            }
        }
        Action::NewNote => crate::pet::command(app, "new-note"),
        Action::HandsFree => crate::voice::hotkey_hands_free(app),
        Action::Dictation => crate::voice::hotkey_dictation(app),
    }
}

fn register_all(app: &AppHandle, combos: &[(Action, String)]) {
    let gs = app.global_shortcut();
    if let Err(error) = gs.unregister_all() {
        tracing::warn!(%error, "couldn't clear the global shortcuts");
    }
    for (action, combo) in combos {
        let Ok(shortcut) = Shortcut::from_str(combo) else {
            tracing::warn!(
                ?action,
                "a global shortcut setting isn't a valid key combination; skipped"
            );
            continue;
        };
        let action = *action;
        let once = std::sync::Mutex::new(OncePerPress::default());
        let result = gs.on_shortcut(shortcut, move |app, _shortcut, event| {
            let mut once = once
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match event.state() {
                ShortcutState::Pressed => {
                    if once.accept(Instant::now()) {
                        drop(once);
                        run(app, action);
                    }
                }
                ShortcutState::Released => once.released(),
            }
        });
        if let Err(error) = result {
            tracing::warn!(%error, ?action, "a global shortcut is taken by another app; skipped");
        }
    }
    register_voice_ptt(app);
}

/// The push-to-talk hotkey: press and release both matter.
fn register_voice_ptt(app: &AppHandle) {
    let Some(combo) = crate::voice::hotkey_combo(app) else {
        return;
    };
    let Ok(shortcut) = Shortcut::from_str(&combo) else {
        tracing::warn!("the push-to-talk hotkey isn't a valid key combination; skipped");
        return;
    };
    let result = app
        .global_shortcut()
        .on_shortcut(shortcut, move |app, _shortcut, event| {
            crate::voice::hotkey(app, event.state() == ShortcutState::Pressed);
        });
    if let Err(error) = result {
        tracing::warn!(%error, "the push-to-talk hotkey is taken by another app; skipped");
    }
}

/// Registers everything again from the current settings (after the voice hotkey changes).
pub fn reapply(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(settings) = app.try_state::<Arc<SettingsService>>() else {
            return;
        };
        let all = settings.get_all().await.unwrap_or_default();
        register_all(&app, &combos(&all));
    });
}

/// Register from the settings now, and again whenever a shortcut setting changes.
pub fn start(app: AppHandle, settings: Arc<SettingsService>, bus: &EventBus) {
    let mut events = bus.subscribe();
    tauri::async_runtime::spawn(async move {
        let apply = |app: &AppHandle, all| register_all(app, &combos(&all));
        apply(&app, settings.get_all().await.unwrap_or_default());
        loop {
            match events.recv().await {
                Recv::Event(Event::SettingChanged { key, .. }) if key.starts_with("shortcuts.") => {
                    apply(&app, settings.get_all().await.unwrap_or_default());
                }
                Recv::Event(Event::AllDataDeleted { .. }) | Recv::Resync { .. } => {
                    apply(&app, settings.get_all().await.unwrap_or_default());
                }
                Recv::Event(_) => {}
                Recv::Closed => break,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_held_key_acts_once_and_the_next_press_acts_again() {
        let mut k = OncePerPress::default();
        let t0 = Instant::now();
        assert!(k.accept(t0), "the first press");
        // Key repeat while it is held: nothing more.
        for ms in [500, 530, 560, 590, 1200] {
            assert!(!k.accept(t0 + Duration::from_millis(ms)), "{ms} ms");
        }
        k.released();
        assert!(
            k.accept(t0 + Duration::from_millis(1300)),
            "pressed again after release"
        );
        // A release that never arrived: a press long after the last one still counts.
        assert!(k.accept(t0 + Duration::from_millis(5000)));
    }

    #[test]
    fn defaults_are_distinct_valid_and_leave_mod_shift_t_free() {
        let all = combos(&Default::default());
        let mut keys: Vec<_> = all.iter().map(|(_, c)| c.clone()).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), ACTIONS.len() - 1, "the pet's is off by default");
        for (_, c) in &all {
            assert!(pluto_core::settings::shortcut_is_valid(c), "{c}");
            assert!(Shortcut::from_str(c).is_ok(), "{c}");
            assert!(!c.ends_with("+T"));
        }
    }

    #[test]
    fn a_chosen_combination_replaces_the_default_and_null_means_default() {
        let settings = [
            ("shortcuts.global_open".to_owned(), json!("Alt+F9")),
            ("shortcuts.global_new_note".to_owned(), Value::Null),
        ]
        .into_iter()
        .collect();
        let all = combos(&settings);
        assert!(all.contains(&(Action::ToggleDashboard, "Alt+F9".to_owned())));
        assert!(all.contains(&(Action::NewNote, "CmdOrCtrl+Shift+N".to_owned())));
        assert!(
            !all.iter().any(|(a, _)| *a == Action::TogglePet),
            "off until chosen"
        );
        let pet: std::collections::BTreeMap<_, _> = [(
            "shortcuts.global_pet".to_owned(),
            json!("CmdOrCtrl+Shift+P"),
        )]
        .into_iter()
        .collect();
        assert!(combos(&pet).contains(&(Action::TogglePet, "CmdOrCtrl+Shift+P".to_owned())));
    }
}
