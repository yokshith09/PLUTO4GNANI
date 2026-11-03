//! F0-09: open Pluto at login, quietly in the tray (`--hidden`).
//!
//! The setting `general.autostart` is the switch; the operating system's login item (Windows Run
//! key, macOS launch agent) is the effect. Two jobs:
//!
//! * **Follow the switch.** When the setting changes, add or remove the login item.
//! * **Don't lie.** At startup, if the login item and the setting disagree (the user removed it in
//!   system settings, or an uninstall did), the setting is changed to match the system.
//!
//! Event-driven: one task parked on the bus, no timers.

use std::sync::Arc;

use pluto_core::bus::{EventBus, Recv};
use pluto_core::events::Event;
use pluto_core::settings_service::SettingsService;
use serde_json::{json, Value};
use tauri::{AppHandle, Runtime};
use tauri_plugin_autostart::ManagerExt;

/// The argument the login item starts Pluto with.
pub const HIDDEN_FLAG: &str = "--hidden";
const KEY: &str = "general.autostart";

/// True when the process was started with `--hidden` (by the login item).
pub fn is_hidden_launch<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter().any(|a| a.as_ref() == HIDDEN_FLAG)
}

/// What to do at startup, given the saved setting and the system's login item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reconcile {
    /// They agree.
    InSync,
    /// They disagree: the system wins, so the setting is changed to this value.
    SettingBecomes(bool),
}

pub fn reconcile(setting_on: bool, system_on: bool) -> Reconcile {
    if setting_on == system_on {
        Reconcile::InSync
    } else {
        Reconcile::SettingBecomes(system_on)
    }
}

fn is_on(value: &Value) -> bool {
    value.as_bool().unwrap_or(false)
}

/// Sync the login item with the setting now and whenever the setting changes.
pub fn start<R: Runtime>(app: AppHandle<R>, settings: Arc<SettingsService>, bus: &EventBus) {
    // Subscribe first, so a change made while reconciling is not missed.
    let mut events = bus.subscribe();
    tauri::async_runtime::spawn(async move {
        let system_on = app.autolaunch().is_enabled().unwrap_or(false);
        let setting_on = settings
            .get_all()
            .await
            .ok()
            .and_then(|all| all.get(KEY).map(is_on))
            .unwrap_or(false);
        if let Reconcile::SettingBecomes(on) = reconcile(setting_on, system_on) {
            if let Err(error) = settings.set(KEY, json!(on)).await {
                tracing::warn!(code = ?error.code, "couldn't update the autostart setting to match the system");
            }
        }

        loop {
            match events.recv().await {
                Recv::Event(Event::SettingChanged { key, value, .. }) if key == KEY => {
                    let want = is_on(&value);
                    let launch = app.autolaunch();
                    let result = if want {
                        launch.enable()
                    } else {
                        launch.disable()
                    };
                    if let Err(error) = result {
                        tracing::warn!(%error, want, "couldn't change the login item");
                        // The switch must not claim something the system refused.
                        let actual = launch.is_enabled().unwrap_or(false);
                        if actual != want {
                            let _ = settings.set(KEY, json!(actual)).await;
                        }
                    }
                }
                // "Delete all data" returns Pluto to a first run: no login item either.
                Recv::Event(Event::AllDataDeleted { .. }) => {
                    let _ = app.autolaunch().disable();
                }
                Recv::Event(_) | Recv::Resync { .. } => {}
                Recv::Closed => break,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_flag_is_found_anywhere_in_the_arguments() {
        assert!(is_hidden_launch(["pluto.exe", "--hidden"]));
        assert!(is_hidden_launch(["--hidden"]));
        assert!(!is_hidden_launch(["pluto.exe"]));
        assert!(!is_hidden_launch(["pluto.exe", "--hidden-ish", "hidden"]));
        assert!(!is_hidden_launch(Vec::<String>::new()));
    }

    #[test]
    fn reconcile_covers_all_four_combinations() {
        assert_eq!(reconcile(false, false), Reconcile::InSync);
        assert_eq!(reconcile(true, true), Reconcile::InSync);
        // The user removed the login item in system settings: the switch must turn off.
        assert_eq!(reconcile(true, false), Reconcile::SettingBecomes(false));
        // The login item exists but the setting says off: the switch must turn on.
        assert_eq!(reconcile(false, true), Reconcile::SettingBecomes(true));
    }
}
