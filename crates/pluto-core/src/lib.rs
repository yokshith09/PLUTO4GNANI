//! Pluto core: all business logic, persistence and event routing (ADR-002).
//!
//! This crate deliberately has no `tauri` dependency, so it builds and its tests
//! run on any machine. `scripts/check-core-no-tauri.mjs` enforces that in CI.

pub mod bus;
pub mod clock;
pub mod courses;
pub mod daily_log;
pub mod data;
pub mod db;
pub mod diary;
pub mod error;
pub mod events;
pub mod firefox;
pub mod focus;
pub mod ids;
pub mod labels;
pub mod legacy_data;
pub mod links;
pub mod logging;
pub mod meetings;
pub mod notes;
pub mod pet_screen;
pub mod pets;
pub mod reminders;
pub mod retention;
pub mod scheduler;
pub mod settings;
pub mod settings_service;
pub mod sites;
pub mod task_plans;
pub mod tasks;
pub mod usage;
pub mod usage_collector;

/// The version of the core, taken from the workspace package version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_semver_shaped() {
        let parts: Vec<&str> = VERSION.split('.').collect();
        assert_eq!(parts.len(), 3, "expected MAJOR.MINOR.PATCH, got {VERSION}");
        assert!(parts.iter().all(|p| p.parse::<u32>().is_ok()));
    }
}
