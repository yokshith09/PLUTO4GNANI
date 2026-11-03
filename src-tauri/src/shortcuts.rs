//! System-wide keyboard shortcuts (UI brief §8). They work while Pluto is in the tray or behind
//! other windows. Two can be changed in Settings (`shortcuts.global_open`,
//! `shortcuts.global_new_note`; `null` means the default); they are registered again whenever
//! those settings change. A shortcut another app already owns is skipped with a warning in the
//! log; the rest still work.
//!
//! `Mod+Shift+T` (new task) was removed by the owner on 2026-10-12. Its setting,
//! `shortcuts.global_new_task`, is no longer read.

use std::str::FromStr;
use std::sync::Arc;

use pluto_core::bus::{EventBus, Recv};
use pluto_core::events::Event;
use pluto_core::settings_service::SettingsService;
use serde_json::Value;
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// What a shortcut does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    TogglePet,
    ToggleDashboard,
    NewNote,
}

/// Each action, its setting (if it can be changed) and its default.
pub const ACTIONS: [(Action, Option<&str>, &str); 3] = [
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

fn run<R: Runtime>(app: &AppHandle<R>, action: Action) {
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
    }
}

fn register_all<R: Runtime>(app: &AppHandle<R>, combos: &[(Action, String)]) {
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
        let result = gs.on_shortcut(shortcut, move |app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                run(app, action);
            }
        });
        if let Err(error) = result {
            tracing::warn!(%error, ?action, "a global shortcut is taken by another app; skipped");
        }
    }
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
