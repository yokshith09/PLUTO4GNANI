//! Dictation output: types text where the cursor is, in whatever app has the focus.
//!
//! The voice runtime does the listening (`VoiceRuntime::dictation_toggle`, Gnani Realtime) and
//! hands each finished phrase to [`OsTyper`]. Nothing is typed unless dictation is on, and the
//! text goes only to the focused window; it is never stored.
//!
//! * **Windows**: `SendInput` with Unicode key events, so any language works. An app running as
//!   administrator ignores input from a normal program; that is Windows, not Pluto.
//! * **macOS**: Quartz key events with a Unicode string. macOS needs Accessibility permission
//!   for Pluto (System Settings > Privacy & Security > Accessibility).
//!
//! Unverified on a real OS: CI builds this, but nobody has dictated into an app yet.

use pluto_contracts::error::{VoiceError, VoiceErrorCode, VoiceResult};
use pluto_voice::runtime::Typer;

pub struct OsTyper;

fn failed(message: impl Into<String>) -> VoiceError {
    VoiceError::new(VoiceErrorCode::Core, message)
}

/// Splits text into pieces of at most `n` UTF-16 units without cutting a character in half.
#[cfg_attr(not(any(windows, target_os = "macos", test)), allow(dead_code))]
fn pieces(text: &str, n: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut units = 0;
    for c in text.chars() {
        let len = c.len_utf16();
        if units + len > n && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
            units = 0;
        }
        cur.push(c);
        units += len;
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(windows)]
impl Typer for OsTyper {
    fn type_text(&self, text: &str) -> VoiceResult<()> {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
            KEYEVENTF_UNICODE,
        };
        let key = |unit: u16, up: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: 0,
                    wScan: unit,
                    dwFlags: KEYEVENTF_UNICODE | if up { KEYEVENTF_KEYUP } else { 0 },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        for piece in pieces(text, 64) {
            let inputs: Vec<INPUT> = piece
                .encode_utf16()
                .flat_map(|u| [key(u, false), key(u, true)])
                .collect();
            let len = u32::try_from(inputs.len()).unwrap_or(u32::MAX);
            // SAFETY: `inputs` is a valid array of `len` INPUT structures that outlives the call.
            let sent = unsafe {
                SendInput(
                    len,
                    inputs.as_ptr(),
                    i32::try_from(std::mem::size_of::<INPUT>()).unwrap_or(40),
                )
            };
            if sent != len {
                return Err(failed(
                    "Windows wouldn't let Pluto type there. If that app runs as administrator, Pluto can't type into it.",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
impl Typer for OsTyper {
    fn type_text(&self, text: &str) -> VoiceResult<()> {
        use core_graphics::event::{CGEvent, CGEventTapLocation};
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
        let denied = || {
            failed("macOS wouldn't let Pluto type. Allow Pluto under System Settings > Privacy & Security > Accessibility.")
        };
        let source =
            CGEventSource::new(CGEventSourceStateID::HIDSystemState).map_err(|()| denied())?;
        for piece in pieces(text, 16) {
            for down in [true, false] {
                let event =
                    CGEvent::new_keyboard_event(source.clone(), 0, down).map_err(|()| denied())?;
                event.set_string(&piece);
                event.post(CGEventTapLocation::HID);
            }
        }
        Ok(())
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
impl Typer for OsTyper {
    fn type_text(&self, _text: &str) -> VoiceResult<()> {
        Err(failed("Dictation works on Windows and Mac."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_text_is_split_without_cutting_a_character() {
        let text = "नमस्ते दोस्तों, कैसे हैं आप सब लोग आज? 😀😀😀";
        let parts = pieces(text, 8);
        assert!(parts.iter().all(|p| p.encode_utf16().count() <= 8));
        assert_eq!(parts.concat(), text);
        assert_eq!(pieces("", 8), Vec::<String>::new());
    }
}

/// Types a short line into whatever window has the focus after three seconds, so typing can be
/// tried without speaking (Voice page → "Test typing"). If this appears but dictation does not,
/// the problem is in what reaches Pluto from the microphone, not in the typing.
#[tauri::command]
pub async fn dictation_test_typing() -> Result<(), String> {
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    tauri::async_runtime::spawn_blocking(|| OsTyper.type_text("Pluto can type here. "))
        .await
        .map_err(|_| "typing was interrupted".to_string())?
        .map_err(|e| e.message)
}
