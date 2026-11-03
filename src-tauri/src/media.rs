//! Play / pause / next under the pet (owner request H4, 2026-10-25).
//!
//! The pet sends the system's own media keys, so whatever is playing (Spotify, a browser tab,
//! a video player) reacts as it does to the keyboard's media keys. There is no browser extension
//! and nothing is read about what is playing.
//!
//! * **Windows**: `SendInput` with `VK_MEDIA_PLAY_PAUSE`, `VK_MEDIA_NEXT_TRACK` and
//!   `VK_MEDIA_PREV_TRACK` (key down, key up).
//! * **macOS**: the same keys as `NSEvent` system-defined events (`NX_KEYTYPE_PLAY` 16,
//!   `NX_KEYTYPE_NEXT` 17, `NX_KEYTYPE_PREVIOUS` 18), posted with a short JavaScript-for-
//!   Automation script run by `/usr/bin/osascript`, so no extra crate or build step is needed.
//!   macOS may ask for the Accessibility permission the first time.
//! * **Elsewhere**: a plain "not available" error; the buttons stay hidden.
//!
//! Unverified on a real OS: the key mapping and the script text are unit-tested, nobody has
//! pressed the buttons on a Windows PC or a Mac yet.

use pluto_core::error::{AppError, Result};

const VK_MEDIA_NEXT_TRACK: u16 = 0xB0;
const VK_MEDIA_PREV_TRACK: u16 = 0xB1;
const VK_MEDIA_PLAY_PAUSE: u16 = 0xB3;

/// The virtual-key code for a pet media action, or `None` for an unknown one. Pure, so it is tested.
pub fn key_for(action: &str) -> Option<u16> {
    match action {
        "play-pause" => Some(VK_MEDIA_PLAY_PAUSE),
        "next" => Some(VK_MEDIA_NEXT_TRACK),
        "previous" => Some(VK_MEDIA_PREV_TRACK),
        _ => None,
    }
}

/// The macOS `NX_KEYTYPE_*` code for a pet media action. Pure, so it is tested everywhere.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn nx_key_for(action: &str) -> Option<u8> {
    match action {
        "play-pause" => Some(16),
        "next" => Some(17),
        "previous" => Some(18),
        _ => None,
    }
}

/// The JavaScript-for-Automation text that presses and releases one media key.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn mac_script(nx_key: u8) -> String {
    format!(
        "ObjC.import('Cocoa');\
         for (const down of [true, false]) {{\
           const flags = down ? 0xa00 : 0xb00;\
           const data = ({nx_key} << 16) | ((down ? 0xa : 0xb) << 8);\
           const e = $.NSEvent.otherEventWithTypeLocationModifierFlagsTimestampWindowNumberContextSubtypeData1Data2(\
             $.NSEventTypeSystemDefined, $.NSMakePoint(0, 0), flags, 0, 0, null, 8, data, -1);\
           $.CGEventPost(0, e.CGEvent);\
         }}"
    )
}

#[cfg(windows)]
fn press(action: &str) -> Result<()> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    };
    let vk =
        key_for(action).ok_or_else(|| AppError::validation("action", "Unknown media action."))?;
    let key = |flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let inputs = [key(0), key(KEYEVENTF_KEYUP)];
    // SAFETY: `inputs` is a valid array of two INPUT structs and the size is that of one INPUT.
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        )
    };
    if sent == inputs.len() as u32 {
        Ok(())
    } else {
        Err(AppError::io("Couldn't send the media key."))
    }
}

#[cfg(target_os = "macos")]
fn press(action: &str) -> Result<()> {
    let key = nx_key_for(action)
        .ok_or_else(|| AppError::validation("action", "Unknown media action."))?;
    let status = std::process::Command::new("/usr/bin/osascript")
        .args(["-l", "JavaScript", "-e", &mac_script(key)])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|_| AppError::io("Couldn't send the media key."))?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError::io(
            "Couldn't send the media key. macOS may need the Accessibility permission for Pluto.",
        ))
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn press(_action: &str) -> Result<()> {
    Err(AppError::io(
        "Media buttons only work on Windows and macOS.",
    ))
}

#[tauri::command]
pub fn pet_media(action: String) -> Result<()> {
    if key_for(&action).is_none() {
        return Err(AppError::validation("action", "Unknown media action."));
    }
    press(&action)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_map_to_the_media_virtual_keys() {
        assert_eq!(key_for("play-pause"), Some(0xB3));
        assert_eq!(key_for("next"), Some(0xB0));
        assert_eq!(key_for("previous"), Some(0xB1));
    }

    #[test]
    fn mac_actions_map_to_the_nx_media_keys_and_the_script_names_the_right_one() {
        assert_eq!(nx_key_for("play-pause"), Some(16));
        assert_eq!(nx_key_for("next"), Some(17));
        assert_eq!(nx_key_for("previous"), Some(18));
        assert_eq!(nx_key_for("volume-up"), None);
        let script = mac_script(17);
        assert!(script.contains("(17 << 16)"));
        assert!(script.contains("CGEventPost"));
    }

    #[test]
    fn anything_else_is_refused() {
        assert_eq!(key_for(""), None);
        assert_eq!(key_for("volume-up"), None);
        assert!(pet_media("rewind".into()).is_err());
    }
}
