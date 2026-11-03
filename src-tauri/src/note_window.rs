//! A note in a window of its own (owner 2026-10-25: "open it as a separate window"). It is the
//! dashboard page with the note's id; bus events reach it like the main window, so an edit there
//! shows up everywhere.

use pluto_core::error::{AppError, Result};
use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

/// Window labels for notes start with this (see `capabilities/note.json`).
pub const PREFIX: &str = "note-";

/// The window label for a note id. Only ids Pluto makes (letters, digits, `-` and `_`) are
/// accepted, so the id is safe to put in a label and in the page's start-up script.
pub fn label_for(id: &str) -> Option<String> {
    let ok = !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    ok.then(|| format!("{PREFIX}{id}"))
}

#[tauri::command]
pub fn note_window_open<R: Runtime>(app: AppHandle<R>, id: String) -> Result<()> {
    let label = label_for(&id)
        .ok_or_else(|| AppError::validation("id", "That note can't open in a window."))?;
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }
    WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("main.html".into()))
        .title("Pluto note")
        .inner_size(720.0, 640.0)
        .min_inner_size(420.0, 360.0)
        .initialization_script(format!("window.__PLUTO_NOTE__ = \"{id}\";"))
        .build()
        .map_err(|error| {
            tracing::warn!(%error, "couldn't open a note window");
            AppError::io("Pluto couldn't open the note in its own window.")
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_ids_get_a_window() {
        assert_eq!(label_for("01JABC-x_9").as_deref(), Some("note-01JABC-x_9"));
        for bad in ["", "a\"b", "a b", "../x", "a;alert(1)", &"x".repeat(65)] {
            assert_eq!(label_for(bad), None, "{bad}");
        }
    }
}
