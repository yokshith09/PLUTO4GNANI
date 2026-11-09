//! The operating-system side of app tracking (opt-in, `tracking.apps`).
//!
//! Everything that decides anything lives in `pluto-core` (`usage`, `usage_collector`) and is
//! tested there. This file only supplies the two things an OS has to: *which window is in front*
//! and *how long since the last input*.
//!
//! * **Windows**: a `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` hook on its own thread with a
//!   blocking message loop (no polling), resolving the window to its executable's file name
//!   (never the path, never the title). Processes that deny access (protected ones) report
//!   nothing. Idle time comes from `GetLastInputInfo`. A film, a video call or a presentation
//!   keeps the display on with a power request (`ES_DISPLAY_REQUIRED`); that is read as "the
//!   screen is held" so time without input is still counted (up to two hours). Once a minute the
//!   window really in front is asked directly (`GetForegroundWindow`), which corrects a focus
//!   event Windows never delivered.
//! * **macOS**: `NSWorkspace.frontmostApplication` (the app's name as macOS shows it, e.g.
//!   "Google Chrome"), looked at once a second on its own thread and reported only when it
//!   changes. That one-second look is an exception to ADR-009: the notification needs a run loop
//!   this thread doesn't have, and the look is a single in-process call. Idle time comes from
//!   `CGEventSourceSecondsSinceLastEventType`. A held screen is read from the power assertions
//!   (`PreventUserIdleDisplaySleep`). No permission is needed.
//! * Elsewhere: [`start`] returns [`Unsupported`] and the UI shows tracking as unavailable.

use std::sync::Arc;

use pluto_core::bus::EventBus;
use pluto_core::clock::Clock;
use pluto_core::settings_service::SettingsService;
use pluto_core::usage::UsageService;
use pluto_core::usage_collector::Collector;

/// This operating system has no collector yet. (Never built on Windows or macOS.)
#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unsupported;

/// The running collector, if this platform has one.
pub struct Tracking {
    collector: Option<Collector>,
}

impl Tracking {
    pub fn unsupported() -> Self {
        Self { collector: None }
    }

    pub fn supported(&self) -> bool {
        self.collector.is_some()
    }

    pub fn is_running(&self) -> bool {
        self.collector.as_ref().is_some_and(Collector::is_running)
    }

    /// After `AppShuttingDown`: wait until the open session has been written.
    pub async fn finish(&self) {
        if let Some(collector) = &self.collector {
            collector.finish().await;
        }
    }
}

/// Start the collector task. It only starts watching windows while `tracking.apps` is on.
/// Must be called inside the async runtime.
#[cfg(any(windows, target_os = "macos"))]
pub fn start(
    usage: Arc<UsageService>,
    settings: Arc<SettingsService>,
    clock: Arc<dyn Clock>,
    bus: &EventBus,
) -> Result<Tracking, Unsupported> {
    #[cfg(windows)]
    let platform = Arc::new(win::Foreground::default());
    #[cfg(target_os = "macos")]
    let platform = Arc::new(mac::Foreground::default());
    Ok(Tracking {
        collector: Some(Collector::spawn(platform, usage, settings, clock, bus)),
    })
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn start(
    _usage: Arc<UsageService>,
    _settings: Arc<SettingsService>,
    _clock: Arc<dyn Clock>,
    _bus: &EventBus,
) -> Result<Tracking, Unsupported> {
    Err(Unsupported)
}

#[cfg(windows)]
pub(crate) mod win {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::sync::mpsc as std_mpsc;
    use std::sync::{Mutex, MutexGuard, PoisonError};
    use std::thread::JoinHandle;
    use std::time::Duration;

    use pluto_core::error::{AppError, Result};
    use pluto_core::usage_collector::Platform;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::mpsc::UnboundedSender;
    use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM};
    use windows_sys::Win32::System::Power::{
        CallNtPowerInformation, SystemExecutionState, ES_DISPLAY_REQUIRED,
    };
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::System::Threading::{
        GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, EnumChildWindows, GetAncestor, GetClassNameW, GetForegroundWindow,
        GetMessageW, GetWindowThreadProcessId, KillTimer, PeekMessageW, PostThreadMessageW,
        SetTimer, TranslateMessage, EVENT_SYSTEM_FOREGROUND, GA_ROOT, MSG, OBJID_WINDOW,
        PM_NOREMOVE, WINEVENT_OUTOFCONTEXT, WM_QUIT, WM_USER,
    };

    /// How long after a foreground event the window in front is asked for again.
    const RECHECK_MS: u32 = 1_000;
    /// The hook thread's pending recheck timer (0 = none).
    static RECHECK: AtomicUsize = AtomicUsize::new(0);

    /// Where the hook callback (a plain function with no captures) sends what it sees.
    static SINK: Mutex<Option<UnboundedSender<Option<String>>>> = Mutex::new(None);

    fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
        m.lock().unwrap_or_else(PoisonError::into_inner)
    }

    struct HookThread {
        thread_id: u32,
        join: JoinHandle<()>,
    }

    #[derive(Default)]
    pub struct Foreground {
        thread: Mutex<Option<HookThread>>,
    }

    impl Platform for Foreground {
        fn start(&self, sink: UnboundedSender<Option<String>>) -> Result<()> {
            let mut slot = lock(&self.thread);
            if slot.is_some() {
                return Ok(());
            }
            *lock(&SINK) = Some(sink);
            let failed = || AppError::internal("Pluto couldn't watch which app is in front.");
            let (ready_tx, ready_rx) = std_mpsc::channel::<Option<u32>>();
            let join = std::thread::Builder::new()
                .name("pluto-winevent".into())
                .spawn(move || hook_thread(&ready_tx))
                .map_err(|_| {
                    *lock(&SINK) = None;
                    failed()
                })?;
            match ready_rx.recv_timeout(Duration::from_secs(2)) {
                Ok(Some(thread_id)) => {
                    *slot = Some(HookThread { thread_id, join });
                    Ok(())
                }
                _ => {
                    *lock(&SINK) = None;
                    Err(failed())
                }
            }
        }

        fn stop(&self) {
            let thread = lock(&self.thread).take();
            if let Some(thread) = thread {
                // SAFETY: plain FFI call; the id is that of a thread this module started. The
                // thread's message queue exists (it was forced before `start` returned), and
                // WM_QUIT ends its message loop.
                unsafe {
                    PostThreadMessageW(thread.thread_id, WM_QUIT, 0, 0);
                }
                let _ = thread.join.join();
            }
            *lock(&SINK) = None;
        }

        fn idle_ms(&self) -> Option<u64> {
            let ms = idle_ms();
            trace::idle(ms);
            ms
        }

        fn screen_held(&self) -> bool {
            display_required()
        }

        fn current_app(&self) -> Option<Option<String>> {
            // SAFETY: plain FFI call with no arguments; it returns null when no window is in front.
            let front = unsafe { GetForegroundWindow() };
            if is_overlay(front) {
                trace::line("look", None, front, Some(KEPT));
                return None;
            }
            let app = foreground_app(front);
            trace::line("look", None, front, app.as_deref());
            Some(app)
        }
    }

    /// Whether something (a video player, a call, a presentation) has asked Windows to keep the
    /// display on right now.
    pub(crate) fn display_required() -> bool {
        let mut state: u32 = 0;
        // SAFETY: the output buffer is a `u32` and its size is passed; there is no input buffer.
        let status = unsafe {
            CallNtPowerInformation(
                SystemExecutionState,
                std::ptr::null(),
                0,
                std::ptr::addr_of_mut!(state).cast(),
                std::mem::size_of::<u32>() as u32,
            )
        };
        status == 0 && state & ES_DISPLAY_REQUIRED != 0
    }

    /// Milliseconds since the last keyboard or mouse input anywhere.
    pub(crate) fn idle_ms() -> Option<u64> {
        let mut info = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        // SAFETY: `info` is a valid LASTINPUTINFO with `cbSize` set, as the API requires.
        if unsafe { GetLastInputInfo(&mut info) } == 0 {
            return None;
        }
        // SAFETY: plain FFI call with no arguments. Both values are 32-bit tick counts that
        // wrap together, so the wrapping difference is right across the 49-day wrap.
        let now = unsafe { GetTickCount() };
        Some(u64::from(now.wrapping_sub(info.dwTime)))
    }

    /// Install the hook and pump messages until `WM_QUIT`. Sends the thread id once the hook is
    /// in place, or `None` if it could not be installed.
    fn hook_thread(ready: &std_mpsc::Sender<Option<u32>>) {
        // SAFETY: every call below is a plain FFI call on this thread with valid arguments. The
        // hook is installed out of context, so Windows delivers events to `on_foreground` on this
        // thread from inside `GetMessageW`, and it is removed before the thread ends.
        unsafe {
            let mut msg = MSG::default();
            // Looking at the queue creates it, so `PostThreadMessageW` can reach this thread.
            PeekMessageW(
                &mut msg,
                std::ptr::null_mut(),
                WM_USER,
                WM_USER,
                PM_NOREMOVE,
            );
            let hook = SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                std::ptr::null_mut(),
                Some(on_foreground),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            );
            if hook.is_null() {
                let _ = ready.send(None);
                return;
            }
            let _ = ready.send(Some(GetCurrentThreadId()));
            // The hook reports changes only, so say what is in front right now.
            report(GetForegroundWindow());
            // Blocks until a message arrives: nothing runs while the foreground window is
            // unchanged. Returns 0 on WM_QUIT and -1 on error; both end the loop.
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            UnhookWinEvent(hook);
        }
    }

    /// Called by Windows on the hook thread. Must not panic or block.
    unsafe extern "system" fn on_foreground(
        _hook: HWINEVENTHOOK,
        event: u32,
        hwnd: HWND,
        id_object: i32,
        _id_child: i32,
        _event_thread: u32,
        _event_time: u32,
    ) {
        if event == EVENT_SYSTEM_FOREGROUND && id_object == OBJID_WINDOW {
            // SAFETY: plain FFI call with no arguments.
            let front = unsafe { GetForegroundWindow() };
            let shown = in_front(front, hwnd);
            if is_overlay(shown) {
                // The previous app keeps counting; nothing to re-check until something else comes forward.
                trace::line("event", Some(hwnd), front, Some(KEPT));
                return;
            }
            let app = report(shown);
            trace::line("event", Some(hwnd), front, app.as_deref());
            schedule_recheck();
        }
    }

    /// The window to record after a foreground event: the one really in front now, and the
    /// event's own window only if Windows says nothing is in front.
    ///
    /// The event's window is not always the one in front (owner 2026-10-31: with continuous use of
    /// VS Code, Chrome and Claude, half of an hour was lost because events named a window that was
    /// not in front, and nothing named the real one again until the once-a-minute look).
    pub(crate) fn in_front(foreground: HWND, event: HWND) -> HWND {
        if foreground.is_null() {
            event
        } else {
            foreground
        }
    }

    /// Ask again a moment after each change, in case the window that came forward goes away
    /// without Windows announcing the one behind it. One pending timer at most; a new change
    /// restarts it.
    fn schedule_recheck() {
        let previous = RECHECK.load(Ordering::Relaxed);
        // SAFETY: a thread timer (no window) on the hook thread, whose message loop dispatches
        // WM_TIMER to `recheck`. Passing the pending id replaces that timer instead of adding one.
        let id = unsafe { SetTimer(std::ptr::null_mut(), previous, RECHECK_MS, Some(recheck)) };
        RECHECK.store(id, Ordering::Relaxed);
    }

    /// Runs on the hook thread from its message loop. Must not panic or block.
    unsafe extern "system" fn recheck(_hwnd: HWND, _msg: u32, id: usize, _time: u32) {
        // SAFETY: plain FFI calls; `id` is the thread timer that fired.
        let front = unsafe {
            KillTimer(std::ptr::null_mut(), id);
            RECHECK.store(0, Ordering::Relaxed);
            GetForegroundWindow()
        };
        if is_overlay(front) {
            trace::line("recheck", None, front, Some(KEPT));
            return;
        }
        let app = report(front);
        trace::line("recheck", None, front, app.as_deref());
        // Something untracked is still in front (the desktop, the Start menu, Pluto): keep asking
        // every second until a real app is back, because Windows does not always announce the
        // return (owner 2026-11-01: time still missing after the one-shot re-check).
        if !app.as_deref().is_some_and(is_tracked) {
            schedule_recheck();
        }
    }

    /// What the trace says when Pluto's pet is in front and the app before it keeps counting.
    const KEPT: &str = "kept (Pluto's pet)";

    /// Pluto's own windows, by handle, with their window label (`pet`, `main`).
    static OWN: Mutex<Vec<(usize, &'static str)>> = Mutex::new(Vec::new());

    /// Remembered once at start-up, so the trace can say which Pluto window was in front and the
    /// pet can be seen through.
    pub(crate) fn set_own_windows(windows: &[(HWND, &'static str)]) {
        *lock(&OWN) = windows
            .iter()
            .map(|&(h, label)| (h as usize, label))
            .collect();
    }

    /// The label of the Pluto window `hwnd` is (or belongs to), if it is one.
    pub(crate) fn own_label(hwnd: HWND) -> Option<&'static str> {
        if hwnd.is_null() {
            return None;
        }
        // SAFETY: plain FFI call; a stale or foreign handle only gives another (or no) window.
        let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
        let own = lock(&OWN);
        [hwnd, root]
            .iter()
            .find_map(|h| own.iter().find(|(o, _)| *o == *h as usize).map(|(_, l)| *l))
    }

    /// The pet floats over the app being used and come forward when touched (a click,
    /// Sleep, a task ticked) while the person carries on reading or scrolling in that app. They do
    /// not stop its time (owner 2026-11-01: the gaps in the trace were all `pluto.exe` in front for
    /// minutes while the input never stopped). The dashboard and note windows are real use of
    /// Pluto and stay uncounted, as before.
    pub(crate) fn is_see_through(label: &str) -> bool {
        label == crate::pet::LABEL
    }

    fn is_overlay(hwnd: HWND) -> bool {
        own_label(hwnd).is_some_and(is_see_through)
    }

    /// Whether time in front of this program is recorded.
    pub(crate) fn is_tracked(exe: &str) -> bool {
        !pluto_core::usage::is_ignored_program(exe)
    }

    fn report(hwnd: HWND) -> Option<String> {
        let app = foreground_app(hwnd);
        if let Some(sink) = lock(&SINK).as_ref() {
            let _ = sink.send(app.clone());
        }
        app
    }

    /// A plain-text trace of what Windows said was in front, for finding uncounted time
    /// (`scripts/tracking-report.py` prints it next to the sessions). Program file names and
    /// window class names only: never window titles. One file in the data folder, emptied when
    /// it passes 2 MB. Temporary, while the owner's missing time is being found.
    pub(crate) mod trace {
        use std::io::Write;
        use std::path::PathBuf;
        use std::sync::Mutex;

        use super::{class_name, executable_name, lock, own_label, HWND};

        const MAX_BYTES: u64 = 2 * 1024 * 1024;
        static FILE: Mutex<Option<PathBuf>> = Mutex::new(None);

        /// Where the trace goes; called once at start-up with the data folder.
        pub(crate) fn set_folder(dir: &std::path::Path) {
            *lock(&FILE) = Some(dir.join("tracking-trace.txt"));
        }

        fn describe(hwnd: HWND) -> String {
            if hwnd.is_null() {
                return "-".to_owned();
            }
            let exe = executable_name(hwnd).unwrap_or_else(|| "?".to_owned());
            let label = own_label(hwnd)
                .map(|l| format!(" [{l}]"))
                .unwrap_or_default();
            format!("{exe}|{}{label}", class_name(hwnd))
        }

        /// One line: time (ms since 1970), what asked, the event's window, the window in front, what was
        /// recorded. Errors are ignored: the trace must never disturb tracking.
        pub(crate) fn line(kind: &str, event: Option<HWND>, front: HWND, recorded: Option<&str>) {
            let event = event.map_or_else(|| "-".to_owned(), describe);
            write(&format!(
                "{kind}\tevent={event}\tfront={}\trecorded={}",
                describe(front),
                recorded.unwrap_or("nothing"),
            ));
        }

        /// The idle time Windows reported (the look reads it once a minute, the totals on every
        /// refresh): shows whether input was seen while the time went missing.
        pub(crate) fn idle(ms: Option<u64>) {
            let value = ms.map_or_else(|| "unknown".to_owned(), |v| format!("{v} ms"));
            write(&format!("idle\tevent=-\tfront=-\trecorded={value}"));
        }

        fn write(fields: &str) {
            let Some(path) = lock(&FILE).clone() else {
                return;
            };
            if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
                let _ = std::fs::remove_file(&path);
            }
            let ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis());
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                let _ = writeln!(f, "{ms}\t{fields}");
            }
        }
    }

    /// The app a foreground window belongs to. The desktop, taskbar and Alt+Tab belong to
    /// explorer.exe too: they record nothing.
    fn foreground_app(hwnd: HWND) -> Option<String> {
        executable_name(hwnd)
            .filter(|exe| !pluto_core::usage::is_shell_surface(exe, &class_name(hwnd)))
    }

    /// The window's class name (e.g. `CabinetWClass` for a File Explorer folder window); empty if
    /// it can't be read.
    fn class_name(hwnd: HWND) -> String {
        let mut buf = [0u16; 256];
        // SAFETY: `buf` holds 256 UTF-16 units and the call writes at most that many; it returns
        // the number written (0 on failure).
        let len = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
        let len = usize::try_from(len).unwrap_or(0).min(buf.len());
        String::from_utf16_lossy(&buf[..len])
    }

    /// The file name (not the path) of the program that owns `hwnd`; `None` if there is no
    /// window or the process cannot be inspected (elevated or protected processes), in which
    /// case nothing is recorded for it. A Microsoft Store app is shown by name, not as the shared
    /// frame it runs in.
    pub(crate) fn executable_name(hwnd: HWND) -> Option<String> {
        let frame = executable_of(process_id(hwnd)?)?;
        if pluto_core::usage::is_store_frame(&frame) {
            if let Some(app) = store_app_process(hwnd).and_then(executable_of) {
                return Some(app);
            }
        }
        Some(frame)
    }

    fn process_id(hwnd: HWND) -> Option<u32> {
        if hwnd.is_null() {
            return None;
        }
        let mut pid = 0u32;
        // SAFETY: `hwnd` came from the OS and `pid` is a valid out pointer.
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        (pid != 0).then_some(pid)
    }

    /// Store apps are drawn inside `ApplicationFrameHost.exe`; the real app is the process of the
    /// frame's `Windows.UI.Core.CoreWindow` child. `None` if it has none (e.g. the app is
    /// suspended), and the frame is recorded as before.
    fn store_app_process(frame: HWND) -> Option<u32> {
        struct Search {
            frame_pid: u32,
            found: Option<u32>,
        }
        unsafe extern "system" fn visit(child: HWND, state: LPARAM) -> i32 {
            // SAFETY: `state` is the `Search` passed to `EnumChildWindows` below, alive for the
            // whole enumeration and used only on this thread.
            let search = unsafe { &mut *(state as *mut Search) };
            if class_name(child) == "Windows.UI.Core.CoreWindow" {
                match process_id(child) {
                    Some(pid) if pid != search.frame_pid => {
                        search.found = Some(pid);
                        return 0; // stop: found it
                    }
                    _ => {}
                }
            }
            1
        }
        let mut search = Search {
            frame_pid: process_id(frame)?,
            found: None,
        };
        // SAFETY: `search` outlives the call and the callback only touches it through `state`.
        unsafe { EnumChildWindows(frame, Some(visit), std::ptr::addr_of_mut!(search) as LPARAM) };
        search.found
    }

    fn executable_of(pid: u32) -> Option<String> {
        // SAFETY: plain FFI call. A null handle means access was denied or the process is gone.
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        // SAFETY: `buf` holds `len` UTF-16 units and `len` is updated to the number written.
        let ok = unsafe {
            QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len)
        };
        // SAFETY: `process` is a handle we opened above and close exactly once.
        unsafe { CloseHandle(process) };
        let written = usize::try_from(len).ok()?.min(buf.len());
        if ok == 0 || written == 0 {
            return None;
        }
        let path = OsString::from_wide(&buf[..written])
            .to_string_lossy()
            .into_owned();
        path.rsplit(['\\', '/'])
            .next()
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
    }

    #[cfg(test)]
    mod tests {
        use super::in_front;

        #[test]
        fn the_window_really_in_front_wins_over_the_event_window() {
            let real = 0x10 as super::HWND;
            let event = 0x20 as super::HWND;
            assert_eq!(in_front(real, event), real);
            assert_eq!(in_front(std::ptr::null_mut(), event), event);
        }

        #[test]
        fn the_pet_is_seen_through_but_the_dashboard_is_not() {
            assert!(super::is_see_through("pet"));
            assert!(!super::is_see_through("main"));
            assert!(!super::is_see_through("note-abc"));
        }

        #[test]
        fn pluto_windows_are_known_by_handle_and_others_are_not() {
            let pet = 0x1010 as super::HWND;
            let main = 0x2020 as super::HWND;
            super::set_own_windows(&[(pet, "pet"), (main, "main")]);
            assert_eq!(super::own_label(pet), Some("pet"));
            assert_eq!(super::own_label(main), Some("main"));
            assert_eq!(super::own_label(std::ptr::null_mut()), None);
            assert!(super::is_overlay(pet));
            assert!(!super::is_overlay(main), "the dashboard counts as real use");
            super::set_own_windows(&[]);
            assert!(!super::is_overlay(pet));
        }

        #[test]
        fn rechecking_continues_only_while_nothing_tracked_is_in_front() {
            assert!(super::is_tracked("Code.exe"));
            assert!(!super::is_tracked("pluto.exe"));
            assert!(!super::is_tracked("StartMenuExperienceHost.exe"));
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) mod mac {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, PoisonError};
    use std::thread::JoinHandle;
    use std::time::Duration;

    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use core_graphics::window::{
        copy_window_info, kCGNullWindowID, kCGWindowLayer, kCGWindowListExcludeDesktopElements,
        kCGWindowListOptionOnScreenOnly, kCGWindowOwnerPID,
    };
    use objc2_app_kit::NSWorkspace;
    use pluto_core::error::{AppError, Result};
    use pluto_core::usage_collector::Platform;
    use tokio::sync::mpsc::UnboundedSender;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventSourceSecondsSinceLastEventType(state: i32, event_type: u32) -> f64;
    }
    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOPMCopyAssertionsStatus(assertions: *mut CFDictionaryRef) -> i32;
    }
    const COMBINED_SESSION_STATE: i32 = 0;
    const ANY_INPUT_EVENT: u32 = !0;

    /// How often the app in front is looked at.
    const LOOK_EVERY: Duration = Duration::from_secs(1);

    /// The name of the app in front, as macOS shows it ("Google Chrome", "Code", "Finder").
    /// Finder with no window open is the desktop and reports nothing.
    pub(crate) fn frontmost_app() -> Option<String> {
        let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
        let name = app.localizedName()?.to_string();
        if name.trim().is_empty() {
            return None;
        }
        // Only Finder is asked (a window list is not free), and once a second at most.
        if !pluto_core::usage::is_bare_finder(&name, false) {
            return Some(name);
        }
        let has_window = has_normal_window(app.processIdentifier());
        (!pluto_core::usage::is_bare_finder(&name, has_window)).then_some(name)
    }

    /// Whether the process owns an ordinary window (layer 0) on screen. Window titles are never
    /// read, so no Screen Recording permission is needed. If the list can't be read, say yes: the
    /// app then counts as before.
    fn has_normal_window(pid: i32) -> bool {
        let Some(list) = copy_window_info(
            kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
            kCGNullWindowID,
        ) else {
            return true;
        };
        // SAFETY: the keys are constants exported by CoreGraphics, valid for the whole process.
        let (pid_key, layer_key) = unsafe {
            (
                CFString::wrap_under_get_rule(kCGWindowOwnerPID),
                CFString::wrap_under_get_rule(kCGWindowLayer),
            )
        };
        let number = |dict: &CFDictionary<CFString, CFType>, key: &CFString| {
            dict.find(key)
                .and_then(|v| v.downcast::<CFNumber>())
                .and_then(|n| n.to_i32())
        };
        list.iter().any(|item| {
            // SAFETY: every element of the window list is a CFDictionary, owned by the array.
            let dict = unsafe {
                CFDictionary::<CFString, CFType>::wrap_under_get_rule(*item as CFDictionaryRef)
            };
            number(&dict, &pid_key) == Some(pid) && number(&dict, &layer_key) == Some(0)
        })
    }

    struct Watcher {
        stop: Arc<AtomicBool>,
        join: JoinHandle<()>,
    }

    #[derive(Default)]
    pub struct Foreground {
        watcher: Mutex<Option<Watcher>>,
    }

    impl Platform for Foreground {
        fn start(&self, sink: UnboundedSender<Option<String>>) -> Result<()> {
            let mut slot = self.watcher.lock().unwrap_or_else(PoisonError::into_inner);
            if slot.is_some() {
                return Ok(());
            }
            let stop = Arc::new(AtomicBool::new(false));
            let flag = stop.clone();
            let join = std::thread::Builder::new()
                .name("pluto-frontmost".into())
                .spawn(move || {
                    let mut last: Option<Option<String>> = None;
                    while !flag.load(Ordering::Relaxed) {
                        let now = frontmost_app();
                        if last.as_ref() != Some(&now) {
                            if sink.send(now.clone()).is_err() {
                                return;
                            }
                            last = Some(now);
                        }
                        std::thread::sleep(LOOK_EVERY);
                    }
                })
                .map_err(|_| AppError::internal("Pluto couldn't watch which app is in front."))?;
            *slot = Some(Watcher { stop, join });
            Ok(())
        }

        fn stop(&self) {
            let watcher = self
                .watcher
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take();
            if let Some(w) = watcher {
                w.stop.store(true, Ordering::Relaxed);
                let _ = w.join.join();
            }
        }

        fn idle_ms(&self) -> Option<u64> {
            idle_ms()
        }

        fn screen_held(&self) -> bool {
            display_held()
        }
    }

    /// Whether something (a video player, a call, a presentation) is keeping the display awake.
    pub(crate) fn display_held() -> bool {
        let mut raw: CFDictionaryRef = std::ptr::null();
        // SAFETY: plain FFI call; on success it hands over a dictionary we own (Create rule).
        if unsafe { IOPMCopyAssertionsStatus(&mut raw) } != 0 || raw.is_null() {
            return false;
        }
        // SAFETY: `raw` is a non-null CFDictionary returned by a Copy function, so we own it.
        let dict = unsafe { CFDictionary::<CFString, CFType>::wrap_under_create_rule(raw) };
        let key = CFString::new("PreventUserIdleDisplaySleep");
        dict.find(&key)
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_i32())
            .is_some_and(|count| count > 0)
    }

    /// Milliseconds since the last keyboard or mouse input anywhere.
    pub(crate) fn idle_ms() -> Option<u64> {
        // SAFETY: plain FFI call with constant arguments; it returns seconds as a double.
        let secs = unsafe {
            CGEventSourceSecondsSinceLastEventType(COMBINED_SESSION_STATE, ANY_INPUT_EVENT)
        };
        (secs.is_finite() && secs >= 0.0).then_some((secs * 1000.0) as u64)
    }
}
