//! Website time from the browser's address bar (`tracking.sites`, needs `tracking.apps` too).
//!
//! The decisions (what counts as a site, when a session starts and ends) live in
//! `pluto_core::sites` and are tested there. This file only supplies one look at the browser in
//! front, every [`LOOK_EVERY_MS`] while both switches are on:
//!
//! * **Windows**: UI Automation. The browser window in front is matched by its executable
//!   (Chrome, Edge, Brave, Firefox, …); its address bar is the first edit box in the window. The
//!   text is reduced to the host at once and the rest is dropped. Nothing is read while you are
//!   typing in the address bar. Private windows are skipped: Edge "InPrivate" and Firefox
//!   "Private Browsing" by their title, Chrome and Brave by their "Incognito"/"Private" toolbar
//!   button (English names only for now). A window that can't be classified is skipped too.
//! * **macOS**: AppleScript, asked of the browser in front: Chrome, Edge, Brave and Vivaldi
//!   report the active tab's address and whether the window is incognito; **Arc** reports its
//!   window's `incognito` flag the same way. macOS asks once per browser ("Pluto wants to control
//!   Google Chrome"); if you say no, nothing is recorded for it. **Safari** can't say whether a
//!   window is private, so it is read only with the extra `tracking.safari` switch (off by
//!   default). **Firefox** has no AppleScript: its own session file is read instead
//!   (`pluto_core::firefox`; private windows are never in it), so its site can lag ~15 s.
//! * Elsewhere: nothing is recorded.
//!
//! Polling is the exception to ADR-009 here: no OS event fires when the address bar changes.
//!
//! Unverified on a real Windows PC.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use pluto_core::bus::{EventBus, Recv};
use pluto_core::clock::Clock;
use pluto_core::events::Event;
use pluto_core::settings_service::SettingsService;
use pluto_core::sites::{Seen, SiteTracker, CHECKPOINT_EVERY_MS, LOOK_EVERY_MS};
use pluto_core::usage::UsageService;
use tokio::sync::mpsc;

/// Whether the looking thread should look at all (both switches on).
static ACTIVE: AtomicBool = AtomicBool::new(false);
/// macOS: whether Safari may be read too (`tracking.safari`, off by default).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
static SAFARI: AtomicBool = AtomicBool::new(false);

/// macOS: whether Safari may be read (also used by the open-tabs list).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn safari_on() -> bool {
    SAFARI.load(Ordering::Relaxed)
}

fn note_safari(all: &std::collections::BTreeMap<String, serde_json::Value>) {
    let on = all
        .get("tracking.safari")
        .and_then(serde_json::Value::as_bool)
        == Some(true);
    SAFARI.store(on, Ordering::Relaxed);
}

#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
/// One look from the OS thread: what it saw and how long since the last input.
struct Look {
    seen: Seen,
    idle_ms: u64,
}

pub fn start(
    usage: Arc<UsageService>,
    settings: Arc<SettingsService>,
    clock: Arc<dyn Clock>,
    bus: &EventBus,
) {
    let (tx, mut looks) = mpsc::unbounded_channel::<Look>();
    if !os::spawn(tx) {
        return; // this platform can't read address bars yet
    }
    let mut events = bus.subscribe();
    tauri::async_runtime::spawn(async move {
        let wanted = |all: std::collections::BTreeMap<String, serde_json::Value>| {
            let on = |k: &str| all.get(k).and_then(serde_json::Value::as_bool) == Some(true);
            on("tracking.apps") && on("tracking.sites")
        };
        let all = settings.get_all().await.unwrap_or_default();
        note_safari(&all);
        ACTIVE.store(wanted(all), Ordering::Relaxed);
        let mut tracker = SiteTracker::default();
        // Crash safety: the site in front is journaled every CHECKPOINT_EVERY_MS and recovered
        // here, before anything new is timed.
        if let Err(error) = usage.recover_open().await {
            tracing::warn!(code = ?error.code, "couldn't recover unsaved website time");
        }
        let mut last_checkpoint = i64::MIN;
        let mut journaled = false;
        let save = |session: Option<pluto_core::usage::DomainSession>| {
            let usage = usage.clone();
            async move {
                if let Some(s) = session {
                    if let Err(error) = usage
                        .record_domain_session(&s.browser, &s.domain, s.started_at, s.ended_at)
                        .await
                    {
                        tracing::warn!(code = ?error.code, "couldn't save time on a website");
                    }
                }
            }
        };
        loop {
            tokio::select! {
                Some(look) = looks.recv() => {
                    if ACTIVE.load(Ordering::Relaxed) {
                        let now = clock.now_ms();
                        save(tracker.look(now, look.idle_ms, look.seen)).await;
                        match tracker.open_session(now, look.idle_ms) {
                            Some(open) if now.saturating_sub(last_checkpoint) >= CHECKPOINT_EVERY_MS => {
                                match usage.checkpoint_site(Some(&open)).await {
                                    Ok(()) => {
                                        last_checkpoint = now;
                                        journaled = true;
                                    }
                                    Err(error) => tracing::warn!(code = ?error.code, "couldn't remember the open website"),
                                }
                            }
                            None if journaled && usage.checkpoint_site(None).await.is_ok() => {
                                journaled = false;
                            }
                            _ => {}
                        }
                    }
                }
                event = events.recv() => match event {
                    Recv::Event(Event::SettingChanged { key, .. }) if key.starts_with("tracking.") => {
                        let all = settings.get_all().await.unwrap_or_default();
                        note_safari(&all);
                        let on = wanted(all);
                        ACTIVE.store(on, Ordering::Relaxed);
                        if !on {
                            save(tracker.stop(clock.now_ms())).await;
                            if journaled && usage.checkpoint_site(None).await.is_ok() {
                                journaled = false;
                            }
                        }
                    }
                    Recv::Event(Event::AppShuttingDown { .. }) => {
                        ACTIVE.store(false, Ordering::Relaxed);
                        save(tracker.stop(clock.now_ms())).await;
                        if journaled {
                            let _ = usage.checkpoint_site(None).await;
                        }
                    }
                    Recv::Event(Event::AllDataDeleted { .. }) | Recv::Resync { .. } => {
                        tracker = SiteTracker::default();
                        ACTIVE.store(wanted(settings.get_all().await.unwrap_or_default()), Ordering::Relaxed);
                    }
                    Recv::Event(_) => {}
                    Recv::Closed => break,
                },
            }
        }
    });
}

#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
fn interval() -> Duration {
    Duration::from_millis(LOOK_EVERY_MS)
}

#[cfg(windows)]
mod os {
    use std::collections::HashMap;
    use std::sync::atomic::Ordering;

    use pluto_core::sites::{domain_from_address, Seen};
    use pluto_core::usage::{display_name, is_browser};
    use tokio::sync::mpsc::UnboundedSender;
    use windows::core::BSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
    };
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationValuePattern,
        TreeScope_Descendants, UIA_ButtonControlTypeId, UIA_ControlTypePropertyId,
        UIA_EditControlTypeId, UIA_NamePropertyId, UIA_ValuePatternId,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW};

    use super::{Look, ACTIVE};

    /// What we know about one browser window: its address bar, and whether it is private.
    struct Window {
        address_bar: Option<IUIAutomationElement>,
        private: bool,
    }

    pub fn spawn(tx: UnboundedSender<Look>) -> bool {
        std::thread::Builder::new()
            .name("pluto-sites".into())
            .spawn(move || run(&tx))
            .is_ok()
    }

    fn run(tx: &UnboundedSender<Look>) {
        // SAFETY: plain COM initialisation for this thread, which owns every UIA object below.
        if unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_err() {
            tracing::warn!("couldn't start UI Automation; website time is off");
            return;
        }
        // SAFETY: creating the in-process UI Automation client, as documented.
        let uia: IUIAutomation =
            match unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) } {
                Ok(u) => u,
                Err(error) => {
                    tracing::warn!(%error, "couldn't start UI Automation; website time is off");
                    return;
                }
            };
        let mut windows: HashMap<isize, Window> = HashMap::new();
        loop {
            std::thread::sleep(super::interval());
            if !ACTIVE.load(Ordering::Relaxed) {
                windows.clear();
                continue;
            }
            let seen = look(&uia, &mut windows);
            let idle_ms = pluto_core::usage::effective_idle_ms(
                crate::tracking::win::idle_ms().unwrap_or(0),
                crate::tracking::win::display_required(),
            );
            if tx.send(Look { seen, idle_ms }).is_err() {
                return;
            }
        }
    }

    fn look(uia: &IUIAutomation, windows: &mut HashMap<isize, Window>) -> Seen {
        // SAFETY: plain FFI call.
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_invalid() {
            return Seen::NoBrowser;
        }
        let Some(exe) = crate::tracking::win::executable_name(hwnd.0) else {
            return Seen::NoBrowser;
        };
        if !is_browser(&exe) {
            return Seen::NoBrowser;
        }
        if windows.len() > 64 {
            windows.clear(); // closed windows; cheap to find again
        }
        let key = hwnd.0 as isize;
        let window = windows.entry(key).or_insert_with(|| inspect(uia, hwnd));
        if window.private || title_says_private(hwnd) {
            return Seen::Nothing;
        }
        let Some(bar) = &window.address_bar else {
            return Seen::Nothing;
        };
        // While you type in the address bar it holds your search, not the site: keep the last one.
        // SAFETY: COM call on the thread that created the element.
        if unsafe { bar.CurrentHasKeyboardFocus() }.is_ok_and(|f| f.as_bool()) {
            return Seen::Nothing;
        }
        // SAFETY: as above; the value pattern of an edit box is its text.
        let text = unsafe {
            bar.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                .and_then(|p| p.CurrentValue())
        };
        let Ok(text) = text else {
            windows.remove(&key); // the element went stale (the window changed); find it again next time
            return Seen::Nothing;
        };
        match domain_from_address(&text.to_string()) {
            Some(domain) => Seen::Site {
                browser: display_name(&exe),
                domain,
            },
            None => Seen::Nothing,
        }
    }

    /// Find a window's address bar and whether it is a private window. Done once per window.
    fn inspect(uia: &IUIAutomation, hwnd: HWND) -> Window {
        let find = |control: i32, name: Option<&str>| -> Option<IUIAutomationElement> {
            // SAFETY: COM calls on this thread with valid arguments.
            unsafe {
                let root = uia.ElementFromHandle(hwnd).ok()?;
                let kind = uia
                    .CreatePropertyCondition(UIA_ControlTypePropertyId, &VARIANT::from(control))
                    .ok()?;
                let condition = match name {
                    Some(n) => {
                        let named = uia
                            .CreatePropertyCondition(
                                UIA_NamePropertyId,
                                &VARIANT::from(BSTR::from(n)),
                            )
                            .ok()?;
                        uia.CreateAndCondition(&kind, &named).ok()?
                    }
                    None => kind,
                };
                root.FindFirst(TreeScope_Descendants, &condition).ok()
            }
        };
        let private = ["Incognito", "Private"]
            .iter()
            .any(|n| find(UIA_ButtonControlTypeId.0, Some(n)).is_some());
        Window {
            address_bar: find(UIA_EditControlTypeId.0, None),
            private,
        }
    }

    /// Edge and Firefox name private windows in the title. The title is checked and dropped.
    fn title_says_private(hwnd: HWND) -> bool {
        let mut buf = [0u16; 512];
        // SAFETY: `buf` is writable for its length.
        let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
        let title = String::from_utf16_lossy(&buf[..usize::try_from(n).unwrap_or(0)]);
        title.contains("InPrivate")
            || title.contains("Private Browsing")
            || title.contains("Incognito")
    }
}

#[cfg(target_os = "macos")]
mod os {
    use std::process::Command;
    use std::sync::atomic::Ordering;

    use pluto_core::sites::{domain_from_address, Seen};
    use pluto_core::usage::{display_name, is_browser};
    use tokio::sync::mpsc::UnboundedSender;

    use super::{Look, ACTIVE, SAFARI};

    /// Browsers that can say both the active tab's address and whether the window is incognito.
    const ASKABLE: &[&str] = &[
        "Google Chrome",
        "Microsoft Edge",
        "Brave Browser",
        "Vivaldi",
    ];

    pub fn spawn(tx: UnboundedSender<Look>) -> bool {
        std::thread::Builder::new()
            .name("pluto-sites".into())
            .spawn(move || loop {
                std::thread::sleep(super::interval());
                if !ACTIVE.load(Ordering::Relaxed) {
                    continue;
                }
                let seen = look();
                let idle_ms = pluto_core::usage::effective_idle_ms(
                    crate::tracking::mac::idle_ms().unwrap_or(0),
                    crate::tracking::mac::display_held(),
                );
                if tx.send(Look { seen, idle_ms }).is_err() {
                    return;
                }
            })
            .is_ok()
    }

    fn look() -> Seen {
        let Some(app) = crate::tracking::mac::frontmost_app() else {
            return Seen::NoBrowser;
        };
        if !is_browser(&app) {
            return Seen::NoBrowser;
        }
        let script = match app.as_str() {
            a if ASKABLE.contains(&a) => None,
            // Arc: `incognito` is true or false; anything else (an older Arc) records nothing.
            "Arc" => Some("tell application \"Arc\" to if (count of windows) > 0 then return (incognito of front window as string) & linefeed & (URL of active tab of front window)".to_owned()),
            "Safari" if SAFARI.load(Ordering::Relaxed) => Some("tell application \"Safari\" to if (count of windows) > 0 then return \"normal\" & linefeed & (URL of current tab of front window)".to_owned()),
            "Firefox" => return firefox_site(&app),
            _ => return Seen::Nothing,
        };
        if let Some(script) = script {
            return ask(&app, &script, if app == "Arc" { "false" } else { "normal" });
        }
        // The app name comes from the fixed list above, so it is safe to put in the script.
        let script = format!(
            "tell application \"{app}\" to if (count of windows) > 0 then return (mode of front window) & linefeed & (URL of active tab of front window)"
        );
        ask(&app, &script, "normal")
    }

    /// Run `script`; its first line must be `allowed` (a normal, non-private window), its
    /// second the address.
    fn ask(app: &str, script: &str, allowed: &str) -> Seen {
        let Ok(out) = Command::new("/usr/bin/osascript")
            .args(["-e", script])
            .output()
        else {
            return Seen::Nothing;
        };
        if !out.status.success() {
            return Seen::Nothing; // not allowed to ask, or no window
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let mut lines = text.lines();
        if lines.next().map(str::trim) != Some(allowed) {
            return Seen::Nothing; // incognito, or an answer we don't understand
        }
        match lines.next().and_then(domain_from_address) {
            Some(domain) => Seen::Site {
                browser: display_name(app),
                domain,
            },
            None => Seen::Nothing,
        }
    }

    /// Firefox: the site showing in its front window, from its session file.
    fn firefox_site(app: &str) -> Seen {
        let Some(home) = std::env::var_os("HOME") else {
            return Seen::Nothing;
        };
        let profiles =
            std::path::Path::new(&home).join("Library/Application Support/Firefox/Profiles");
        let domain = pluto_core::firefox::newest_session_file(&profiles)
            .and_then(|f| pluto_core::firefox::read_session(&f).ok())
            .and_then(|json| pluto_core::firefox::active_domain(&json).ok().flatten());
        match domain {
            Some(domain) => Seen::Site {
                browser: display_name(app),
                domain,
            },
            None => Seen::Nothing,
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod os {
    use tokio::sync::mpsc::UnboundedSender;

    pub fn spawn(_tx: UnboundedSender<super::Look>) -> bool {
        false
    }
}
