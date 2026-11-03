//! Reminder alerts. A Windows or macOS notification can be swallowed by the system (Focus
//! Assist / Do not disturb, notifications turned off for Pluto), and the notification plugin
//! never reports that. So a due reminder also flashes Pluto in the taskbar, and the pet rings
//! (src/pet/reactions.ts). Settings has a test button and a shortcut to the system settings.

use pluto_core::error::{AppError, Result};
use pluto_core::reminders::Reminder;
use tauri::{AppHandle, Manager, Runtime, UserAttentionType};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;

/// The system page where notifications for Pluto are turned on or off.
pub fn settings_url() -> Option<&'static str> {
    if cfg!(target_os = "windows") {
        Some("ms-settings:notifications")
    } else if cfg!(target_os = "macos") {
        Some("x-apple.systempreferences:com.apple.preference.notifications")
    } else {
        None
    }
}

/// The system sound a reminder plays. Without one, a Windows toast is silent (the owner found
/// reminders made no sound, 2026-10-25). Windows: the "Reminder" toast sound; macOS: "Glass".
pub const REMINDER_SOUND: &str = if cfg!(target_os = "macos") {
    "Glass"
} else {
    "Reminder"
};

fn show<R: Runtime>(app: &AppHandle<R>, title: &str, body: &str, sound: bool) -> Result<()> {
    let builder = app.notification().builder().title(title).body(body);
    let builder = if sound {
        builder.sound(REMINDER_SOUND)
    } else {
        builder
    };
    builder.show().map_err(|error| {
        tracing::warn!(%error, "couldn't show a notification");
        AppError::io("Pluto couldn't show a notification.")
    })
}

/// A reminder is due: notify, and ask for attention in case the notification is hidden.
pub fn reminder<R: Runtime>(app: &AppHandle<R>, reminder: &Reminder) {
    // Best effort: a refused or failed notification must not stop the bus.
    let _ = show(app, &reminder.title, "Reminder from Pluto", reminder.sound);
    if let Some(main) = app.get_webview_window("main") {
        if !main.is_focused().unwrap_or(false) {
            let _ = main.request_user_attention(Some(UserAttentionType::Informational));
        }
    }
}

/// Settings › Notifications › "Send a test notification".
#[tauri::command]
pub fn notification_test(app: AppHandle) -> Result<()> {
    show(
        &app,
        "Pluto notifications work",
        "Reminders will show up like this.",
        true,
    )
}

/// Settings › Notifications › "Open notification settings".
#[tauri::command]
pub fn notification_settings_open(app: AppHandle) -> Result<()> {
    let url = settings_url()
        .ok_or_else(|| AppError::io("Open your system settings to manage notifications."))?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|_| AppError::io("Pluto couldn't open the notification settings."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reminders_name_a_sound_the_system_knows() {
        // Windows toasts know "Default", "IM", "Mail", "Reminder" and "SMS"; an unknown name or
        // none at all means a silent toast. macOS plays a named system sound.
        let windows = ["Default", "IM", "Mail", "Reminder", "SMS"];
        let mac = ["Basso", "Blow", "Glass", "Hero", "Ping", "Pop", "Submarine"];
        assert!(windows.contains(&REMINDER_SOUND) || mac.contains(&REMINDER_SOUND));
    }

    #[test]
    fn the_settings_link_is_a_system_scheme_on_desktop_systems() {
        if let Some(url) = settings_url() {
            assert!(
                url.starts_with("ms-settings:") || url.starts_with("x-apple.systempreferences:")
            );
        }
    }
}
