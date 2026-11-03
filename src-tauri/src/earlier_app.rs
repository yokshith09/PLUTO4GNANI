//! macOS: an earlier build of this app may still be installed.
//!
//! A different app name is a different app to macOS, so after installing Pluto the earlier program
//! stayed in the Applications folder and kept its login item. There is no installer step to hook on
//! a Mac, so Pluto offers to clear it away itself: once, after the person's data has already been
//! copied across (`pluto_core::legacy_data::carry_over`), and only with their say-so.
//!
//! * Asked at most once (`ui.earlier_app_asked`), never at a login launch, never without the dialog.
//! * "Move to Bin" stops the old program, removes its login item and moves it to the Bin (through
//!   Finder, so it can be put back; if Finder refuses, straight into the Bin folder).
//! * "Keep it" changes nothing and is not asked again.
//!
//! Windows has the same job done by the installer (`src-tauri/windows/hooks.nsh`).

// Only a Mac asks (see lib.rs); the Bin helper is plain file moving, so it is tested everywhere.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use pluto_core::legacy_data::{self, EarlierMacApp};
use pluto_core::settings_service::SettingsService;
use serde_json::json;
use tauri::AppHandle;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

const ASKED: &str = "ui.earlier_app_asked";

/// Look for an earlier install and, if there is one and the person has not been asked, ask.
pub fn offer_removal(app: AppHandle, settings: Arc<SettingsService>) {
    if crate::autostart::is_hidden_launch(std::env::args()) {
        return; // a login launch should not open a dialog
    }
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return;
    };
    let found = legacy_data::earlier_mac_app(
        &[PathBuf::from("/Applications"), home.join("Applications")],
        &home.join("Library/LaunchAgents"),
    );
    if found.is_empty() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        if settings.get_pref(ASKED).await.ok().flatten().is_some() {
            return;
        }
        let asker = app.clone();
        let yes = tauri::async_runtime::spawn_blocking(move || {
            asker
                .dialog()
                .message(
                    "An earlier version of this app is still installed. Your notes, tasks and \
                     settings have already been copied into Pluto.\n\nMove the earlier version to \
                     the Bin and stop it starting at login?",
                )
                .title("Pluto")
                .buttons(MessageDialogButtons::OkCancelCustom(
                    "Move to Bin".into(),
                    "Keep it".into(),
                ))
                .blocking_show()
        })
        .await
        .unwrap_or(false);
        let _ = settings
            .set_pref(ASKED, json!(if yes { "removed" } else { "kept" }))
            .await;
        if yes {
            let _ = tauri::async_runtime::spawn_blocking(move || remove(&found, &home)).await;
        }
    });
}

fn remove(found: &EarlierMacApp, home: &Path) {
    for item in &found.login_items {
        if let Err(error) = std::fs::remove_file(item) {
            tracing::warn!(%error, "couldn't remove the earlier app's login item");
        }
    }
    let _ = Command::new("pkill")
        .args(["-x", legacy_data::OLD_PROCESS_NAME])
        .status();
    for bundle in &found.bundles {
        if !bin_through_finder(bundle) && !bin_directly(bundle, &home.join(".Trash")) {
            tracing::warn!(path = %bundle.display(), "couldn't move the earlier app to the Bin");
        }
    }
}

/// Finder's own "delete": goes to the Bin with Put Back. Asks the person's permission the first time.
fn bin_through_finder(bundle: &Path) -> bool {
    let path = bundle
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    Command::new("osascript")
        .args([
            "-e",
            &format!("tell application \"Finder\" to delete POSIX file \"{path}\""),
        ])
        .status()
        .is_ok_and(|s| s.success())
}

/// Fallback: move the bundle into the Bin folder under a name that is not taken.
fn bin_directly(bundle: &Path, trash: &Path) -> bool {
    let Some(name) = bundle.file_stem().and_then(|n| n.to_str()) else {
        return false;
    };
    (0..100).any(|i| {
        let target = if i == 0 {
            trash.join(format!("{name}.app"))
        } else {
            trash.join(format!("{name} {i}.app"))
        };
        !target.exists() && std::fs::rename(bundle, &target).is_ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bundle_is_moved_into_the_bin_under_a_free_name() {
        let tmp = tempfile::tempdir().unwrap();
        let (bin, apps) = (tmp.path().join(".Trash"), tmp.path().join("Applications"));
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(bin.join("Loaf.app")).unwrap(); // the name is taken
        std::fs::create_dir_all(apps.join("Loaf.app/Contents")).unwrap();

        assert!(bin_directly(&apps.join("Loaf.app"), &bin));

        assert!(!apps.join("Loaf.app").exists());
        assert!(bin.join("Loaf 1.app/Contents").is_dir());
        assert!(
            bin.join("Loaf.app").is_dir(),
            "what was already in the Bin is left alone"
        );
    }
}
