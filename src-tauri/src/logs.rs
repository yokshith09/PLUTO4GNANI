//! The log file and crash reports (F0-13), in the app's log folder. Logging starts before
//! anything else, at `info`; the `advanced.log_level` setting is applied as soon as settings are
//! readable and again whenever it changes (Settings › Advanced), without a restart.

use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use pluto_core::bus::{EventBus, Recv};
use pluto_core::events::Event;
use pluto_core::logging::{self, LevelHandle, LogGuard, LogLevel};
use pluto_core::settings_service::SettingsService;
use serde_json::Value;

/// Keeps the log writer alive for the life of the app.
pub struct Logs {
    _guard: LogGuard,
    level: LevelHandle,
}

/// Start the log file and the crash-report hook. `None` if the folder can't be used (Pluto still
/// runs; it just can't write logs).
pub fn init(dir: &Path) -> Option<Logs> {
    std::fs::create_dir_all(dir).ok()?;
    logging::install_panic_hook(dir.to_owned());
    let (guard, level) = logging::init_reloadable(dir, LogLevel::Info).ok()?;
    tracing::info!(version = pluto_core::VERSION, "Pluto started");
    Some(Logs {
        _guard: guard,
        level,
    })
}

fn level_of(all: &std::collections::BTreeMap<String, Value>) -> LogLevel {
    all.get("advanced.log_level")
        .and_then(Value::as_str)
        .and_then(|s| LogLevel::from_str(s).ok())
        .unwrap_or(LogLevel::Info)
}

/// Apply `advanced.log_level` now and on every change.
pub fn follow_setting(logs: Arc<Logs>, settings: Arc<SettingsService>, bus: &EventBus) {
    let mut events = bus.subscribe();
    tauri::async_runtime::spawn(async move {
        logs.level
            .set(level_of(&settings.get_all().await.unwrap_or_default()));
        loop {
            match events.recv().await {
                Recv::Event(Event::SettingChanged { key, .. }) if key == "advanced.log_level" => {
                    logs.level
                        .set(level_of(&settings.get_all().await.unwrap_or_default()));
                }
                Recv::Event(Event::AllDataDeleted { .. }) => logs.level.set(LogLevel::Info),
                Recv::Event(_) | Recv::Resync { .. } => {}
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
    fn the_setting_picks_the_level_and_anything_odd_means_info() {
        let with =
            |v: Value| level_of(&[("advanced.log_level".to_owned(), v)].into_iter().collect());
        assert_eq!(with(json!("debug")), LogLevel::Debug);
        assert_eq!(with(json!("warn")), LogLevel::Warn);
        assert_eq!(with(json!("loud")), LogLevel::Info);
        assert_eq!(level_of(&Default::default()), LogLevel::Info);
    }
}
