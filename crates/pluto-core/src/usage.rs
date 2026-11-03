//! "Where my time went": foreground-app and website time, opt-in and local only.
//!
//! Three layers, from pure to impure:
//!
//! 1. **Identity** ([`display_name`], [`category_for`], [`is_browser`], [`normalize_host`]):
//!    pure string rules.
//! 2. **[`Tracker`]**: a deterministic state machine. The OS collector feeds it focus, idle and
//!    shutdown events with timestamps and it hands back closed [`AppSession`]s. It knows nothing
//!    about the OS or the database, so every rule below is tested with fake timestamps.
//! 3. **[`UsageService`]**: persistence, queries and retention on top of the database writer.
//!    Events publish after commit, as everywhere else.
//!
//! Privacy is structural: only a bare, lower-case host is ever stored for a website, never a URL,
//! path or title (the tables have no column for them), untracked domains are refused at the door,
//! and incognito windows are the browser extension's responsibility to never report.
//! All times are ms since the Unix epoch (UTC); local hours use the injected [`Clock`]'s zone.

use std::collections::HashSet;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use chrono::{DateTime, Timelike, Utc};
use chrono_tz::Tz;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::bus::{EventBus, Recv};
use crate::clock::Clock;
use crate::db::Database;
use crate::error::{AppError, Result};
use crate::events::Event;
use crate::ids::new_id;

mod journal;

/// Sessions shorter than this are noise (alt-tab flicker) and are dropped.
pub const MIN_SESSION_MS: i64 = 2_000;
/// A single session is never longer than this; anything longer was a forgotten machine.
pub const MAX_SESSION_MS: i64 = 12 * 60 * 60 * 1000;
/// No keyboard or mouse input for this long means the person has stepped away.
pub const IDLE_THRESHOLD_MS: u64 = 10 * 60 * 1000;
/// How often the collector looks at the idle time while an app is in front (ADR-019).
pub const POLL_INTERVAL_MS: u64 = 60 * 1000;
/// A gap this much longer than [`POLL_INTERVAL_MS`] between two looks means the machine slept.
pub const SLEEP_GAP_MS: i64 = 150 * 1000;
/// A screen held awake by a film, a call or a presentation counts as activity for at most this
/// long without any input; beyond it the person is taken to have walked away from a forgotten
/// player.
pub const MEDIA_HOLD_MAX_MS: u64 = 2 * 60 * 60 * 1000;
/// A reading of the clock this much earlier than a previous one means the clock was set back.
pub const CLOCK_STEP_BACK_MS: i64 = 30_000;
/// Usage older than this is deleted.
pub const RETENTION_DAYS: i64 = 400;
/// `UsageUpdated` is published at most this often while recording.
pub const NOTIFY_EVERY_MS: i64 = 30 * 1000;
/// Reported times may be this far ahead of our clock (clock skew between browser and app).
const FUTURE_SLACK_MS: i64 = 5 * 60 * 1000;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;
const NAME_MAX: usize = 100;

pub const CATEGORIES: [&str; 7] = [
    "Coding",
    "Design",
    "Research",
    "Communication",
    "Notes",
    "Entertainment",
    "Other",
];

// ---- identity -------------------------------------------------------------------------------

struct Known {
    /// Lower-case executable name without `.exe` (or the macOS app name, lower-cased).
    key: &'static str,
    display: &'static str,
    category: &'static str,
    browser: bool,
}

const fn app(key: &'static str, display: &'static str, category: &'static str) -> Known {
    Known {
        key,
        display,
        category,
        browser: false,
    }
}

const fn browser(key: &'static str, display: &'static str) -> Known {
    Known {
        key,
        display,
        category: "Research",
        browser: true,
    }
}

const KNOWN: &[Known] = &[
    browser("chrome", "Google Chrome"),
    browser("google chrome", "Google Chrome"),
    browser("msedge", "Microsoft Edge"),
    browser("microsoft edge", "Microsoft Edge"),
    browser("firefox", "Firefox"),
    browser("brave", "Brave"),
    browser("brave browser", "Brave"),
    browser("opera", "Opera"),
    browser("vivaldi", "Vivaldi"),
    browser("safari", "Safari"),
    browser("arc", "Arc"),
    app("code", "VS Code", "Coding"),
    app("visual studio code", "VS Code", "Coding"),
    app("cursor", "Cursor", "Coding"),
    app("devenv", "Visual Studio", "Coding"),
    app("windowsterminal", "Terminal", "Coding"),
    app("terminal", "Terminal", "Coding"),
    app("iterm2", "iTerm", "Coding"),
    app("powershell", "PowerShell", "Coding"),
    app("pwsh", "PowerShell", "Coding"),
    app("cmd", "Command Prompt", "Coding"),
    app("xcode", "Xcode", "Coding"),
    app("idea64", "IntelliJ IDEA", "Coding"),
    app("pycharm64", "PyCharm", "Coding"),
    app("webstorm64", "WebStorm", "Coding"),
    app("rider64", "Rider", "Coding"),
    app("sublime_text", "Sublime Text", "Coding"),
    app("figma", "Figma", "Design"),
    app("photoshop", "Photoshop", "Design"),
    app("illustrator", "Illustrator", "Design"),
    app("sketch", "Sketch", "Design"),
    app("canva", "Canva", "Design"),
    app("gimp", "GIMP", "Design"),
    app("inkscape", "Inkscape", "Design"),
    app("acrord32", "Acrobat Reader", "Research"),
    app("acrobat", "Acrobat", "Research"),
    app("zotero", "Zotero", "Research"),
    app("slack", "Slack", "Communication"),
    app("teams", "Microsoft Teams", "Communication"),
    app("ms-teams", "Microsoft Teams", "Communication"),
    app("discord", "Discord", "Communication"),
    app("zoom", "Zoom", "Communication"),
    app("outlook", "Outlook", "Communication"),
    app("thunderbird", "Thunderbird", "Communication"),
    app("telegram", "Telegram", "Communication"),
    app("whatsapp", "WhatsApp", "Communication"),
    app("signal", "Signal", "Communication"),
    app("skype", "Skype", "Communication"),
    app("notion", "Notion", "Notes"),
    app("obsidian", "Obsidian", "Notes"),
    app("winword", "Word", "Notes"),
    app("onenote", "OneNote", "Notes"),
    app("notepad", "Notepad", "Notes"),
    app("evernote", "Evernote", "Notes"),
    app("typora", "Typora", "Notes"),
    app("logseq", "Logseq", "Notes"),
    app("spotify", "Spotify", "Entertainment"),
    app("steam", "Steam", "Entertainment"),
    app("netflix", "Netflix", "Entertainment"),
    app("vlc", "VLC", "Entertainment"),
    app("epicgameslauncher", "Epic Games", "Entertainment"),
    app("explorer", "File Explorer", "Other"),
    app("applicationframehost", "Windows App", "Other"),
    app("systemsettings", "Settings", "Other"),
    app("excel", "Excel", "Other"),
    app("powerpnt", "PowerPoint", "Other"),
    // macOS names (NSWorkspace gives the app's shown name).
    app("microsoft word", "Word", "Notes"),
    app("microsoft excel", "Excel", "Other"),
    app("microsoft powerpoint", "PowerPoint", "Other"),
    app("microsoft outlook", "Outlook", "Communication"),
    app("microsoft teams", "Microsoft Teams", "Communication"),
    app("zoom.us", "Zoom", "Communication"),
    app("mail", "Mail", "Communication"),
    app("messages", "Messages", "Communication"),
    app("notes", "Notes", "Notes"),
    app("pages", "Pages", "Notes"),
    app("intellij idea", "IntelliJ IDEA", "Coding"),
    app("sublime text", "Sublime Text", "Coding"),
    app("finder", "Finder", "Other"),
    app("system settings", "Settings", "Other"),
    app("preview", "Preview", "Research"),
];

/// Shell surfaces that flash in front of the real foreground app (lock screen, Start menu, task
/// switcher), and Pluto itself: planning in Pluto is not time spent "somewhere else" (decided
/// 2026-10-27; no setting yet, it is one line here to flip).
const IGNORED: &[&str] = &[
    "lockapp",
    "logonui",
    "searchhost",
    "searchapp",
    "startmenuexperiencehost",
    "shellexperiencehost",
    "textinputhost",
    "pluto",
    // macOS: the login and lock screen, the screen saver.
    "loginwindow",
    "screensaverengine",
];

/// The file name of `path` (either separator) without its directory.
fn base_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path).trim()
}

/// Windows draws the desktop, the taskbar, the tray and the task switcher with the same program
/// as File Explorer (`explorer.exe`). Only its folder windows are File Explorer; the rest is the
/// shell and is never "where time went" (owner 2026-10-25: File Explorer counted while unused).
/// `class` is the foreground window's class name.
pub fn is_shell_surface(exe: &str, class: &str) -> bool {
    stem_key(exe) == "explorer" && !matches!(class, "CabinetWClass" | "ExploreWClass")
}

/// Microsoft Store apps are all drawn inside this one program, so the shell must look inside the
/// frame for the real app (it falls back to this name, "Windows App", when it cannot).
pub fn is_store_frame(exe: &str) -> bool {
    stem_key(exe) == "applicationframehost"
}

/// On a Mac, clicking the desktop makes Finder the app in front even though no Finder window is
/// open. Finder with no window on screen is the desktop, not "where time went".
pub fn is_bare_finder(app: &str, has_window: bool) -> bool {
    !has_window && app.trim().eq_ignore_ascii_case("finder")
}

/// `C:\Apps\Chrome.EXE` -> `chrome`: the key used to look an app up.
/// Whether time in front of this program is never recorded (Pluto itself, the lock screen, the
/// Start menu and other shell surfaces that flash in front of the real app).
pub fn is_ignored_program(exe: &str) -> bool {
    IGNORED.contains(&stem_key(exe).as_str())
}

fn stem_key(path: &str) -> String {
    let base = base_name(path).to_lowercase();
    base.strip_suffix(".exe").unwrap_or(&base).trim().to_owned()
}

fn lookup(app: &str) -> Option<&'static Known> {
    let key = stem_key(app);
    let shown = app.trim().to_lowercase();
    KNOWN
        .iter()
        .find(|k| k.key == key)
        .or_else(|| KNOWN.iter().find(|k| k.display.to_lowercase() == shown))
}

/// A friendly name for an executable: `chrome.exe` -> "Google Chrome", `Code.exe` -> "VS Code",
/// otherwise the extension is dropped and an all-lower-case name is title-cased.
pub fn display_name(exe: &str) -> String {
    if let Some(known) = lookup(exe) {
        return known.display.to_owned();
    }
    let base = base_name(exe);
    let stem = match base.rsplit_once('.') {
        Some((stem, ext))
            if !stem.is_empty()
                && (1..=5).contains(&ext.len())
                && ext.chars().all(|c| c.is_ascii_alphanumeric()) =>
        {
            stem
        }
        _ => base,
    };
    let spaced: String = stem
        .chars()
        .map(|c| if c == '_' || c == '-' { ' ' } else { c })
        .collect();
    let spaced = spaced.trim();
    let named = if spaced.chars().any(char::is_uppercase) {
        spaced.to_owned() // already cased by its author ("VLC", "GitKraken")
    } else {
        spaced
            .split(' ')
            .filter(|w| !w.is_empty())
            .map(|w| {
                let mut cs = w.chars();
                cs.next()
                    .map(|f| f.to_uppercase().chain(cs).collect::<String>())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    named.chars().take(NAME_MAX).collect()
}

/// The default category for an app (an executable name or a display name).
pub fn category_for(app: &str) -> &'static str {
    if let Some(known) = lookup(app) {
        return known.category;
    }
    let key = stem_key(app);
    const CODING_HINTS: [&str; 8] = [
        "idea", "pycharm", "webstorm", "clion", "goland", "phpstorm", "studio", "terminal",
    ];
    if CODING_HINTS.iter().any(|h| key.contains(h)) {
        "Coding"
    } else {
        "Other"
    }
}

/// Whether the executable is a web browser (so its time can be broken down by site).
pub fn is_browser(exe: &str) -> bool {
    lookup(exe).is_some_and(|k| k.browser)
}

/// The default category for a website, by well-known domain (itself or a parent domain).
pub fn category_for_domain(domain: &str) -> &'static str {
    const SITES: &[(&str, &str)] = &[
        ("github.com", "Coding"),
        ("gitlab.com", "Coding"),
        ("stackoverflow.com", "Coding"),
        ("docs.rs", "Coding"),
        ("developer.mozilla.org", "Coding"),
        ("figma.com", "Design"),
        ("dribbble.com", "Design"),
        ("wikipedia.org", "Research"),
        ("arxiv.org", "Research"),
        ("scholar.google.com", "Research"),
        ("slack.com", "Communication"),
        ("discord.com", "Communication"),
        ("mail.google.com", "Communication"),
        ("outlook.live.com", "Communication"),
        ("notion.so", "Notes"),
        ("docs.google.com", "Notes"),
        ("youtube.com", "Entertainment"),
        ("netflix.com", "Entertainment"),
        ("twitch.tv", "Entertainment"),
        ("reddit.com", "Entertainment"),
        ("spotify.com", "Entertainment"),
    ];
    SITES
        .iter()
        .find(|(site, _)| domain == *site || domain.ends_with(&format!(".{site}")))
        .map_or("Other", |(_, category)| category)
}

/// Whether a website is itself a well-known advertising or tracking company (itself or a subdomain).
///
/// This is all the address bar can tell us: it names the page you are on, never the third-party
/// requests the page makes (DECISIONS U16, option d). A small fixed list, so a `true` is a fact
/// about the domain and a `false` means "not on the list", never "clean".
pub fn is_known_tracker(domain: &str) -> bool {
    const TRACKERS: &[&str] = &[
        "doubleclick.net",
        "googlesyndication.com",
        "googleadservices.com",
        "google-analytics.com",
        "googletagmanager.com",
        "facebook.net",
        "scorecardresearch.com",
        "adnxs.com",
        "criteo.com",
        "taboola.com",
        "outbrain.com",
        "hotjar.com",
        "mixpanel.com",
        "segment.io",
        "quantserve.com",
        "amazon-adsystem.com",
    ];
    TRACKERS
        .iter()
        .any(|t| domain == *t || domain.ends_with(&format!(".{t}")))
}

/// Check that `input` is a bare host name and return it lower-cased.
///
/// Anything that looks like a URL (a scheme, a path, a query, a port, credentials) is refused:
/// the extension must send the host only, so a full address can never reach the database.
pub fn normalize_host(input: &str) -> Result<String> {
    let bad = |m: &str| AppError::validation("domain", m);
    let host = input.trim().trim_end_matches('.').to_lowercase();
    if host.is_empty() {
        return Err(bad("A website needs a name."));
    }
    if host.len() > 253 {
        return Err(bad("That website name is too long."));
    }
    let allowed = |c: char| c.is_alphanumeric() || c == '-' || c == '.' || c == '_';
    if !host.chars().all(allowed) || host.split('.').any(str::is_empty) {
        return Err(bad(
            "Send only the website's name (like example.com), not a link.",
        ));
    }
    Ok(host)
}

fn clean_name(field: &'static str, input: &str) -> Result<String> {
    let name = input.trim();
    if name.is_empty() || name.chars().count() > NAME_MAX || name.chars().any(char::is_control) {
        return Err(AppError::validation(
            field,
            "That name is empty, too long or has odd characters.",
        ));
    }
    Ok(name.to_owned())
}

fn check_category(category: &str) -> Result<()> {
    if CATEGORIES.contains(&category) {
        Ok(())
    } else {
        Err(AppError::validation(
            "category",
            "Pick one of the listed categories.",
        ))
    }
}

/// How long the person has been away, for the idle rules. While the system is being kept awake
/// on purpose (a film, a video call, a presentation hold the screen on) a stretch with no
/// keyboard or mouse input is still use, for up to [`MEDIA_HOLD_MAX_MS`].
pub fn effective_idle_ms(idle_ms: u64, screen_held: bool) -> u64 {
    if screen_held && idle_ms < MEDIA_HOLD_MAX_MS {
        0
    } else {
        idle_ms
    }
}

/// Apply the minimum and maximum session rules. `None` means "too short, drop it".
pub fn normalize_span(started_at: i64, ended_at: i64) -> Option<(i64, i64)> {
    let ended = ended_at.min(started_at.saturating_add(MAX_SESSION_MS));
    (ended - started_at >= MIN_SESSION_MS).then_some((started_at, ended))
}

// ---- the tracker ----------------------------------------------------------------------------

/// A finished stretch in one foreground app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppSession {
    /// Display name, e.g. "Google Chrome".
    pub app: String,
    pub category: String,
    pub is_browser: bool,
    pub started_at: i64,
    pub ended_at: i64,
}

/// What the OS collector tells the [`Tracker`]. `at` is ms since the epoch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackEvent {
    /// A different window came to the front. `app` is its executable (or app) name, `None` when
    /// nothing trackable is in front (a denied process, the lock screen, no window).
    Focus { at: i64, app: Option<String> },
    /// The person stopped using the machine at `at` (the time of their last input).
    Idle { at: i64 },
    /// The person came back; their first input was at `at`.
    Active { at: i64 },
    /// Pluto is quitting or tracking was switched off.
    Shutdown { at: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Focused {
    exe: String,
    name: String,
    category: &'static str,
    is_browser: bool,
}

/// Turns focus/idle events into closed sessions. Pure and deterministic.
#[derive(Debug, Default)]
pub struct Tracker {
    excluded: HashSet<String>,
    /// The trackable app in front, even while idle (so activity can resume it).
    focused: Option<Focused>,
    /// When the open session started; `Some` only while `focused` is `Some` and not idle.
    open_since: Option<i64>,
    idle: bool,
    /// Events never go backwards: a late or skewed timestamp is raised to the previous one.
    last_event: i64,
    /// The last time the collector looked at the clock, for sleep detection.
    last_look: Option<i64>,
    /// Set when a look found the person already away (no input for a full look interval, but not
    /// yet long enough to end the session): when their input stopped. The stretch in progress is
    /// shown only up to here instead of running on until the idle threshold is reached.
    quiet_since: Option<i64>,
    /// The latest reading of the wall clock the collector gave, to notice the clock being set
    /// back. (`last_event` cannot do it: idle times are legitimately in the past.)
    last_now: Option<i64>,
}

impl Tracker {
    pub fn new(excluded: &[String]) -> Self {
        Self {
            excluded: excluded_set(excluded),
            ..Self::default()
        }
    }

    /// True while an app is in front, which is when the collector needs to watch the idle time.
    pub fn is_watching(&self) -> bool {
        self.focused.is_some()
    }

    pub fn is_idle(&self) -> bool {
        self.idle
    }

    /// When the person's input stopped, if a look found them already away but not yet long enough
    /// to end the stretch. The stretch in progress is not shown past this.
    pub fn quiet_since(&self) -> Option<i64> {
        self.quiet_since
    }

    /// Notice the wall clock being set back (a time sync, a wrong CMOS clock, a dual-boot machine):
    /// the stretch earned so far is closed where it was last seen and a fresh one starts at the
    /// corrected time. Without this the stretch would stay frozen at the old, later time until the
    /// clock caught up, and the time in between would be lost.
    fn observe(&mut self, now: i64) -> Option<AppSession> {
        let Some(previous) = self.last_now else {
            self.last_now = Some(now);
            return None;
        };
        if now.saturating_add(CLOCK_STEP_BACK_MS) >= previous {
            self.last_now = Some(previous.max(now));
            return None;
        }
        // Set back: everything from here is measured on the corrected clock.
        self.last_now = Some(now);
        let seen_until = self
            .quiet_since
            .map_or(previous, |quiet| quiet.min(previous));
        let closed = self.close(seen_until);
        self.last_event = now;
        self.quiet_since = None;
        if self.focused.is_some() {
            self.last_look = Some(now);
            if !self.idle {
                self.open_since = Some(now);
            }
        }
        closed
    }

    /// The stretch in progress, if an app is in front and the person is active. Its `ended_at`
    /// is `now`; it is not final and is never stored, only shown (see [`UsageService::set_live`]).
    pub fn open_session(&self, now: i64) -> Option<AppSession> {
        let started_at = self.open_since?;
        let focused = self.focused.as_ref()?;
        Some(AppSession {
            app: focused.name.clone(),
            category: focused.category.to_owned(),
            is_browser: focused.is_browser,
            started_at,
            ended_at: self.quiet_since.map_or(now, |q| q.min(now)).max(started_at),
        })
    }

    /// Replace the "do not track" list. If the app in front is now excluded its session ends at
    /// `at` and nothing further is recorded for it.
    pub fn set_excluded(&mut self, at: i64, excluded: &[String]) -> Vec<AppSession> {
        self.excluded = excluded_set(excluded);
        let hide = self
            .focused
            .as_ref()
            .is_some_and(|f| self.is_excluded(&f.exe));
        if !hide {
            return vec![];
        }
        let at = at.max(self.last_event);
        let closed = self.close(at);
        self.focused = None;
        closed.into_iter().collect()
    }

    pub fn handle(&mut self, event: TrackEvent) -> Vec<AppSession> {
        let mut out = Vec::new();
        match event {
            TrackEvent::Focus { at, app } => {
                out.extend(self.observe(at));
                let at = self.advance(at);
                let next = app.as_deref().and_then(|a| self.resolve(a));
                if next.as_ref().map(|f| &f.name) == self.focused.as_ref().map(|f| &f.name) {
                    return out; // the same app again: one continuous session
                }
                out.extend(self.close(at));
                self.focused = next;
                self.quiet_since = None;
                if self.focused.is_some() {
                    self.last_look = Some(at);
                    if !self.idle {
                        self.open_since = Some(at);
                    }
                }
            }
            TrackEvent::Idle { at } => {
                let at = self.advance(at);
                if !self.idle {
                    self.idle = true;
                    out.extend(self.close(at));
                }
            }
            TrackEvent::Active { at } => {
                let at = self.advance(at);
                self.quiet_since = None;
                if self.idle {
                    self.idle = false;
                    if self.focused.is_some() {
                        self.open_since = Some(at);
                    }
                }
            }
            TrackEvent::Shutdown { at } => {
                out.extend(self.observe(at));
                let at = self.advance(at);
                out.extend(self.close(at));
                self.focused = None;
                self.last_look = None;
            }
        }
        out
    }

    /// The collector's periodic look: `now` and how long since the last keyboard or mouse input.
    ///
    /// * Idle for [`IDLE_THRESHOLD_MS`]: the session ends when the input stopped, not now.
    /// * Input again while idle: a new session starts at that input.
    /// * Much longer since the last look than the interval: the machine slept, so the session
    ///   ends at the last look instead of spanning the night.
    /// * Quiet for a whole look interval but not yet idle: the session stays open (they may be
    ///   reading), but [`Tracker::open_session`] shows it only up to the last input.
    pub fn poll(&mut self, now: i64, idle_ms: u64) -> Vec<AppSession> {
        let mut out = Vec::new();
        out.extend(self.observe(now));
        if self.focused.is_none() {
            self.last_look = None;
            return out;
        }
        if let Some(before) = self.last_look {
            if now - before > SLEEP_GAP_MS && !self.idle {
                out.extend(self.handle(TrackEvent::Idle { at: before }));
            }
        }
        self.last_look = Some(now);
        let last_input = now.saturating_sub(i64::try_from(idle_ms).unwrap_or(i64::MAX));
        if idle_ms >= IDLE_THRESHOLD_MS {
            out.extend(self.handle(TrackEvent::Idle { at: last_input }));
        } else {
            out.extend(self.handle(TrackEvent::Active { at: last_input }));
            if idle_ms >= POLL_INTERVAL_MS {
                self.quiet_since = Some(last_input);
            }
        }
        out
    }

    fn advance(&mut self, at: i64) -> i64 {
        self.last_event = self.last_event.max(at);
        self.last_event
    }

    fn is_excluded(&self, exe: &str) -> bool {
        self.excluded.contains(&stem_key(exe))
            || self.excluded.contains(&display_name(exe).to_lowercase())
    }

    fn resolve(&self, exe: &str) -> Option<Focused> {
        let key = stem_key(exe);
        if key.is_empty() || IGNORED.contains(&key.as_str()) || self.is_excluded(exe) {
            return None;
        }
        Some(Focused {
            exe: exe.to_owned(),
            name: display_name(exe),
            category: category_for(exe),
            is_browser: is_browser(exe),
        })
    }

    fn close(&mut self, at: i64) -> Option<AppSession> {
        let started_at = self.open_since.take()?;
        let focused = self.focused.as_ref()?;
        let (started_at, ended_at) = normalize_span(started_at, at.max(started_at))?;
        Some(AppSession {
            app: focused.name.clone(),
            category: focused.category.to_owned(),
            is_browser: focused.is_browser,
            started_at,
            ended_at,
        })
    }
}

fn excluded_set(list: &[String]) -> HashSet<String> {
    list.iter()
        .map(|e| stem_key(e))
        .filter(|e| !e.is_empty())
        .chain(
            list.iter()
                .map(|e| e.trim().to_lowercase())
                .filter(|e| !e.is_empty()),
        )
        .collect()
}

// ---- types that cross the IPC boundary ------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct TimeRange {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub started_at: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub ended_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AppUsage {
    pub app: String,
    pub category: String,
    pub is_browser: bool,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total_seconds: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub sessions: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct DomainUsage {
    pub domain: String,
    /// The browser the site was most recently seen in.
    pub browser: String,
    pub category: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total_seconds: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub sessions: i64,
    /// The site is itself a known advertising or tracking company (`is_known_tracker`). `false`
    /// means "not on the list", never "clean".
    pub is_tracker: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct UsageSummary {
    /// Total foreground time in the range, all apps.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total_seconds: i64,
    /// Most used first.
    pub apps: Vec<AppUsage>,
    /// Most used first. A breakdown of browser time by site, so it is not added to the total.
    pub domains: Vec<DomainUsage>,
    /// Exactly 24 entries: foreground seconds by hour of the local day (0 = midnight to 1 am),
    /// summed over the range, in the clock's time zone.
    #[cfg_attr(feature = "ts", ts(type = "number[]"))]
    pub hourly_seconds: Vec<i64>,
}

/// A website the person has seen, with their choices for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct DomainSetting {
    pub domain: String,
    pub category: String,
    pub tracked: bool,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub first_seen: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub last_seen: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct TrackingStatus {
    /// This operating system has a collector.
    pub supported: bool,
    /// The collector is running right now.
    pub running: bool,
    /// The person turned tracking on (`tracking.apps`).
    pub enabled: bool,
}

// ---- persistence ----------------------------------------------------------------------------

/// A website stretch reported by a browser extension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainSession {
    pub browser: String,
    pub domain: String,
    pub started_at: i64,
    pub ended_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsageRecord {
    App(AppSession),
    Domain(DomainSession),
}

/// Reads, writes and retention for usage data.
pub struct UsageService {
    db: Arc<Database>,
    clock: Arc<dyn Clock>,
    /// When `UsageUpdated` was last published by a recording (not a delete).
    last_notified: AtomicI64,
    /// Time the totals must include although it is not in the database yet. Set by the
    /// collector; never stored by this field.
    unsaved: Mutex<Unsaved>,
    /// Asks the OS how long the person has been away, so the stretch in progress can be shown up
    /// to their last input at the moment the totals are read, not only at the last look.
    idle_probe: Mutex<Option<IdleProbe>>,
    /// Held while the totals are read and while a waiting batch is written, so a session can
    /// never be counted twice (in the database and still waiting) or not at all.
    gate: tokio::sync::Mutex<()>,
}

/// Milliseconds since the last keyboard or mouse input (a held screen already counted as input),
/// or `None` if the OS cannot say. Cheap: it is called every time the totals are read.
pub type IdleProbe = Arc<dyn Fn() -> Option<u64> + Send + Sync>;

/// What the collector holds in memory: the stretch in progress, and finished sessions waiting for
/// their batch write. Both are in the totals; without them the dashboard would lag and, worse,
/// drop a session for up to a minute after the person switched away from it.
#[derive(Debug, Default)]
struct Unsaved {
    live: Option<AppSession>,
    /// The live stretch is not shown past this: the person's input stopped here.
    live_until: Option<i64>,
    waiting: Vec<AppSession>,
}

impl UsageService {
    pub fn new(db: Arc<Database>, clock: Arc<dyn Clock>) -> Self {
        Self {
            db,
            clock,
            last_notified: AtomicI64::new(i64::MIN),
            unsaved: Mutex::new(Unsaved::default()),
            idle_probe: Mutex::new(None),
            gate: tokio::sync::Mutex::new(()),
        }
    }

    fn unsaved(&self) -> std::sync::MutexGuard<'_, Unsaved> {
        self.unsaved.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Tell the service which stretch is in progress (`None` when nothing is being timed). It is
    /// shown up to the present moment.
    pub fn set_live(&self, session: Option<AppSession>) {
        self.set_unsaved(session, None, vec![]);
    }

    /// Tell the service everything the collector holds in memory, in one step so the totals
    /// never see half of it: the stretch in progress, where it must stop being shown (when the
    /// person's input stopped, if a look found them away), and the finished sessions still
    /// waiting to be written.
    pub fn set_unsaved(
        &self,
        live: Option<AppSession>,
        live_until: Option<i64>,
        waiting: Vec<AppSession>,
    ) {
        *self.unsaved() = Unsaved {
            live,
            live_until,
            waiting,
        };
    }

    /// Let the totals ask the OS for the idle time (`None` removes it).
    pub fn set_idle_probe(&self, probe: Option<IdleProbe>) {
        *self
            .idle_probe
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = probe;
    }

    /// The stretch in progress as it should be shown right now, and the sessions waiting to be
    /// written.
    ///
    /// It is shown up to the person's last input, never past it, whenever the OS can say when that
    /// was. The last input time only moves forward, so the total can no longer climb through quiet
    /// time and drop back when a look finds the person gone (up to two minutes later). Without a
    /// probe it is shown up to the present, or to where the last look saw their input stop.
    fn unsaved_now(&self) -> (Option<AppSession>, Vec<AppSession>) {
        let (live, live_until, waiting) = {
            let state = self.unsaved();
            (state.live.clone(), state.live_until, state.waiting.clone())
        };
        let live = live.map(|mut s| {
            let now = self.clock.now_ms();
            let probe = self
                .idle_probe
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
            let shown_until = match probe.and_then(|ask| ask()) {
                Some(idle) => now.saturating_sub(i64::try_from(idle).unwrap_or(i64::MAX)),
                None => live_until.map_or(now, |until| until.min(now)),
            };
            s.ended_at = shown_until
                .min(s.started_at.saturating_add(MAX_SESSION_MS))
                .max(s.started_at);
            s
        });
        (live, waiting)
    }

    /// Write the collector's waiting sessions in one transaction and, in the same step, stop
    /// counting them as waiting. Records that are invalid or too short are skipped.
    pub async fn save_waiting(&self, batch: Vec<UsageRecord>) -> Result<u64> {
        let _gate = self.gate.lock().await;
        let saved = self.record_many(batch).await?;
        self.unsaved().waiting.clear();
        Ok(saved)
    }

    fn notification_due(&self, now: i64) -> bool {
        now.saturating_sub(self.last_notified.load(Ordering::Relaxed)) >= NOTIFY_EVERY_MS
    }

    /// Store one app session. `Ok(false)` if it was too short to keep.
    pub async fn record_app_session(&self, session: AppSession) -> Result<bool> {
        self.record_one(UsageRecord::App(session)).await
    }

    /// Store one website session reported by a browser extension. `Ok(false)` if it was dropped
    /// (too short, or the person marked the domain "do not track"). A value that looks like a
    /// link rather than a host is an error.
    pub async fn record_domain_session(
        &self,
        browser: &str,
        domain: &str,
        started_at: i64,
        ended_at: i64,
    ) -> Result<bool> {
        self.record_one(UsageRecord::Domain(DomainSession {
            browser: browser.to_owned(),
            domain: domain.to_owned(),
            started_at,
            ended_at,
        }))
        .await
    }

    async fn record_one(&self, record: UsageRecord) -> Result<bool> {
        let now = self.clock.now_ms();
        let notify = self.notification_due(now);
        let saved = self
            .db
            .write(move |tx| {
                let saved = store(tx, &record, now)?;
                let events = if saved && notify {
                    vec![Event::UsageUpdated { at: now }]
                } else {
                    vec![]
                };
                Ok((saved, events))
            })
            .await?;
        if saved && notify {
            self.last_notified.store(now, Ordering::Relaxed);
        }
        Ok(saved)
    }

    /// Store a batch in one transaction. Records that are invalid, too short or for an untracked
    /// domain are skipped so one bad record cannot lose the rest. Returns how many were saved.
    pub async fn record_many(&self, batch: Vec<UsageRecord>) -> Result<u64> {
        if batch.is_empty() {
            return Ok(0);
        }
        let now = self.clock.now_ms();
        let notify = self.notification_due(now);
        let saved = self
            .db
            .write(move |tx| {
                let mut saved = 0u64;
                for record in &batch {
                    match store(tx, record, now) {
                        Ok(true) => saved += 1,
                        Ok(false) => {}
                        Err(e) if e.code == crate::error::ErrorCode::Validation => {
                            tracing::warn!("skipped a usage record that failed validation");
                        }
                        Err(e) => return Err(e),
                    }
                }
                let events = if saved > 0 && notify {
                    vec![Event::UsageUpdated { at: now }]
                } else {
                    vec![]
                };
                Ok((saved, events))
            })
            .await?;
        if saved > 0 && notify {
            self.last_notified.store(now, Ordering::Relaxed);
        }
        Ok(saved)
    }

    /// Totals, per-app, per-site and per-hour time within `[from, to)`. Sessions that straddle
    /// the edges count only for the part inside.
    pub async fn summary(&self, from: i64, to: i64) -> Result<UsageSummary> {
        check_range(from, to)?;
        let tz = self.clock.tz();
        let _gate = self.gate.lock().await;
        let mut summary = self.db.read(move |c| read_summary(c, tz, from, to)).await?;
        let (live, waiting) = self.unsaved_now();
        for session in waiting.iter().chain(live.iter()) {
            add_live(&mut summary, tz, session, from, to);
        }
        Ok(summary)
    }

    /// The exact active spans of one app within `[from, to)`, oldest first, clipped.
    pub async fn app_sessions(&self, app: &str, from: i64, to: i64) -> Result<Vec<TimeRange>> {
        check_range(from, to)?;
        let app = app.trim().to_owned();
        self.db
            .read(move |c| {
                spans(
                    c,
                    "SELECT MAX(started_at, ?2), MIN(ended_at, ?3) FROM app_sessions
                     WHERE app = ?1 AND started_at < ?3 AND ended_at > ?2 AND ended_at > started_at
                     ORDER BY started_at",
                    params![app, from, to],
                )
            })
            .await
    }

    /// The exact active spans on one site within `[from, to)`, oldest first, clipped.
    pub async fn domain_sessions(
        &self,
        domain: &str,
        from: i64,
        to: i64,
    ) -> Result<Vec<TimeRange>> {
        check_range(from, to)?;
        let domain = normalize_host(domain)?;
        self.db
            .read(move |c| {
                spans(
                    c,
                    "SELECT MAX(started_at, ?2), MIN(ended_at, ?3) FROM domain_sessions
                     WHERE domain = ?1 AND started_at < ?3 AND ended_at > ?2 AND ended_at > started_at
                     ORDER BY started_at",
                    params![domain, from, to],
                )
            })
            .await
    }

    /// Every website seen and not yet forgotten, with its category and tracking choice.
    pub async fn domains(&self) -> Result<Vec<DomainSetting>> {
        self.db
            .read(|c| {
                let mut stmt = c.prepare(
                    "SELECT domain, category, tracked, first_seen, last_seen FROM domains
                     ORDER BY domain",
                )?;
                let rows = stmt
                    .query_map([], |r| {
                        Ok(DomainSetting {
                            domain: r.get(0)?,
                            category: r.get(1)?,
                            tracked: r.get::<_, i64>(2)? != 0,
                            first_seen: r.get(3)?,
                            last_seen: r.get(4)?,
                        })
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await
    }

    /// Delete every session that overlaps `[from, to)` (a session that straddles an edge goes
    /// entirely: deleting is the privacy action, so it errs on the side of removing more).
    /// Returns how many sessions were removed.
    pub async fn delete_range(&self, from: i64, to: i64) -> Result<u64> {
        check_range(from, to)?;
        let now = self.clock.now_ms();
        self.db
            .write(move |tx| {
                let mut removed = tx.execute(
                    "DELETE FROM app_sessions WHERE started_at < ?2 AND ended_at > ?1",
                    params![from, to],
                )?;
                removed += tx.execute(
                    "DELETE FROM domain_sessions WHERE started_at < ?2 AND ended_at > ?1",
                    params![from, to],
                )?;
                Self::clear_range(tx, from, to)?;
                forget_unseen_domains(tx)?;
                Ok((removed as u64, vec![Event::UsageUpdated { at: now }]))
            })
            .await
    }

    /// Delete all usage data. Sites the person marked "do not track" keep that mark, so deleting
    /// history never re-enables a site.
    pub async fn delete_all(&self) -> Result<u64> {
        let now = self.clock.now_ms();
        self.db
            .write(move |tx| {
                let mut removed = tx.execute("DELETE FROM app_sessions", [])?;
                removed += tx.execute("DELETE FROM domain_sessions", [])?;
                tx.execute("DELETE FROM usage_open", [])?;
                tx.execute("DELETE FROM domains WHERE tracked = 1", [])?;
                Ok((removed as u64, vec![Event::UsageUpdated { at: now }]))
            })
            .await
    }

    /// Mark a site tracked or "do not track". Untracking also deletes what was recorded for it.
    pub async fn set_domain_tracked(&self, domain: &str, tracked: bool) -> Result<()> {
        let domain = normalize_host(domain)?;
        let now = self.clock.now_ms();
        self.db
            .write(move |tx| {
                tx.execute(
                    "INSERT INTO domains (domain, category, tracked, first_seen, last_seen)
                     VALUES (?1, ?2, ?3, ?4, ?4)
                     ON CONFLICT(domain) DO UPDATE SET tracked = excluded.tracked",
                    params![domain, category_for_domain(&domain), tracked, now],
                )?;
                if !tracked {
                    tx.execute("DELETE FROM domain_sessions WHERE domain = ?1", [&domain])?;
                    tx.execute(
                        "DELETE FROM usage_open WHERE kind = 'site' AND name = ?1",
                        [&domain],
                    )?;
                }
                Ok(((), vec![Event::UsageUpdated { at: now }]))
            })
            .await
    }

    pub async fn set_domain_category(&self, domain: &str, category: &str) -> Result<()> {
        check_category(category)?;
        let domain = normalize_host(domain)?;
        let (category, now) = (category.to_owned(), self.clock.now_ms());
        self.db
            .write(move |tx| {
                tx.execute(
                    "INSERT INTO domains (domain, category, tracked, first_seen, last_seen)
                     VALUES (?1, ?2, 1, ?3, ?3)
                     ON CONFLICT(domain) DO UPDATE SET category = excluded.category",
                    params![domain, category, now],
                )?;
                Ok(((), vec![Event::UsageUpdated { at: now }]))
            })
            .await
    }

    /// Delete usage older than [`RETENTION_DAYS`]. Returns how many sessions were removed.
    /// Runs at startup and on every day rollover, never on a timer of its own.
    pub async fn purge_expired(&self) -> Result<u64> {
        let now = self.clock.now_ms();
        let cutoff = now - RETENTION_DAYS * DAY_MS;
        self.db
            .write(move |tx| {
                let mut removed =
                    tx.execute("DELETE FROM app_sessions WHERE ended_at < ?1", [cutoff])?;
                removed += tx.execute("DELETE FROM domain_sessions WHERE ended_at < ?1", [cutoff])?;
                tx.execute(
                    "DELETE FROM domains WHERE tracked = 1 AND last_seen < ?1
                       AND NOT EXISTS (SELECT 1 FROM domain_sessions s WHERE s.domain = domains.domain)",
                    [cutoff],
                )?;
                let events = if removed > 0 {
                    vec![Event::UsageUpdated { at: now }]
                } else {
                    vec![]
                };
                Ok((removed as u64, events))
            })
            .await
    }
}

/// Run [`UsageService::purge_expired`] whenever the day rolls over (and after falling behind on
/// the bus), like the Bin's janitor. Event-driven: it parks on the bus between days.
pub fn spawn_janitor(service: Arc<UsageService>, bus: &EventBus) -> tokio::task::JoinHandle<()> {
    let mut events = bus.subscribe();
    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Recv::Event(Event::DayRolledOver { .. }) | Recv::Resync { .. } => {
                    if let Err(error) = service.purge_expired().await {
                        tracing::warn!(code = ?error.code, "couldn't clear old usage data; it will be retried tomorrow");
                    }
                }
                Recv::Closed => break,
                Recv::Event(_) => {}
            }
        }
    })
}

fn check_range(from: i64, to: i64) -> Result<()> {
    if from >= to {
        return Err(AppError::validation(
            "to",
            "The end must be after the start.",
        ));
    }
    Ok(())
}

/// Validate and insert one record. `Ok(false)` means it was dropped on purpose.
fn store(conn: &Connection, record: &UsageRecord, now: i64) -> Result<bool> {
    match record {
        UsageRecord::App(s) => {
            let app = clean_name("app", &s.app)?;
            check_category(&s.category)?;
            let Some((start, end)) = checked_span(s.started_at, s.ended_at, now)? else {
                return Ok(false);
            };
            conn.execute(
                "INSERT INTO app_sessions (id, app, category, is_browser, started_at, ended_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![new_id(), app, s.category, s.is_browser, start, end],
            )?;
            Ok(true)
        }
        UsageRecord::Domain(s) => {
            let domain = normalize_host(&s.domain)?;
            let browser = clean_name("browser", &s.browser)?;
            let Some((start, end)) = checked_span(s.started_at, s.ended_at, now)? else {
                return Ok(false);
            };
            let tracked: Option<i64> = conn
                .query_row(
                    "SELECT tracked FROM domains WHERE domain = ?1",
                    [&domain],
                    |r| r.get(0),
                )
                .optional()?;
            if tracked == Some(0) {
                return Ok(false);
            }
            conn.execute(
                "INSERT INTO domains (domain, category, tracked, first_seen, last_seen)
                 VALUES (?1, ?2, 1, ?3, ?4)
                 ON CONFLICT(domain) DO UPDATE SET
                   first_seen = MIN(first_seen, excluded.first_seen),
                   last_seen = MAX(last_seen, excluded.last_seen)",
                params![domain, category_for_domain(&domain), start, end],
            )?;
            conn.execute(
                "INSERT INTO domain_sessions (id, browser, domain, started_at, ended_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![new_id(), browser, domain, start, end],
            )?;
            Ok(true)
        }
    }
}

fn checked_span(started_at: i64, ended_at: i64, now: i64) -> Result<Option<(i64, i64)>> {
    if started_at < 0 || ended_at < started_at {
        return Err(AppError::validation(
            "ended_at",
            "A session can't end before it starts.",
        ));
    }
    if ended_at > now.saturating_add(FUTURE_SLACK_MS) {
        return Err(AppError::validation(
            "ended_at",
            "A session can't end in the future.",
        ));
    }
    Ok(normalize_span(started_at, ended_at))
}

/// Remove site rows that no longer have any session behind them. "Do not track" marks stay.
fn forget_unseen_domains(conn: &Connection) -> Result<()> {
    conn.execute(
        "DELETE FROM domains WHERE tracked = 1
           AND NOT EXISTS (SELECT 1 FROM domain_sessions s WHERE s.domain = domains.domain)",
        [],
    )?;
    Ok(())
}

fn spans(conn: &Connection, sql: &str, args: impl rusqlite::Params) -> Result<Vec<TimeRange>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map(args, |r| {
            Ok(TimeRange {
                started_at: r.get(0)?,
                ended_at: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn read_summary(conn: &Connection, tz: Tz, from: i64, to: i64) -> Result<UsageSummary> {
    let mut apps_stmt = conn.prepare(
        "SELECT app, MAX(category), MAX(is_browser),
                SUM(MIN(ended_at, ?2) - MAX(started_at, ?1)) AS ms, COUNT(*)
         FROM app_sessions
         WHERE started_at < ?2 AND ended_at > ?1 AND ended_at > started_at
         GROUP BY app ORDER BY ms DESC, app",
    )?;
    let apps = apps_stmt
        .query_map(params![from, to], |r| {
            Ok(AppUsage {
                app: r.get(0)?,
                category: r.get(1)?,
                is_browser: r.get::<_, i64>(2)? != 0,
                total_seconds: r.get::<_, i64>(3)? / 1000,
                sessions: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut domains_stmt = conn.prepare(
        "SELECT d.domain,
                (SELECT l.browser FROM domain_sessions l
                  WHERE l.domain = d.domain AND l.started_at < ?2 AND l.ended_at > ?1
                  ORDER BY l.ended_at DESC LIMIT 1),
                COALESCE(MAX(m.category), 'Other'),
                SUM(MIN(d.ended_at, ?2) - MAX(d.started_at, ?1)) AS ms, COUNT(*)
         FROM domain_sessions d LEFT JOIN domains m ON m.domain = d.domain
         WHERE d.started_at < ?2 AND d.ended_at > ?1 AND d.ended_at > d.started_at
         GROUP BY d.domain ORDER BY ms DESC, d.domain",
    )?;
    let domains = domains_stmt
        .query_map(params![from, to], |r| {
            let domain: String = r.get(0)?;
            Ok(DomainUsage {
                is_tracker: is_known_tracker(&domain),
                domain,
                browser: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                category: r.get(2)?,
                total_seconds: r.get::<_, i64>(3)? / 1000,
                sessions: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut hours = [0i64; 24];
    let mut total_ms = 0i64;
    let mut clipped = conn.prepare(
        "SELECT MAX(started_at, ?1), MIN(ended_at, ?2) FROM app_sessions
         WHERE started_at < ?2 AND ended_at > ?1 AND ended_at > started_at",
    )?;
    let rows = clipped.query_map(params![from, to], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
    })?;
    for row in rows {
        let (start, end) = row?;
        total_ms += end - start;
        add_to_hours(&mut hours, tz, start, end);
    }

    Ok(UsageSummary {
        total_seconds: total_ms / 1000,
        apps,
        domains,
        hourly_seconds: hours.iter().map(|ms| ms / 1000).collect(),
    })
}

/// Add the stretch in progress (the part inside `[from, to)`) to a summary read from the database.
fn add_live(summary: &mut UsageSummary, tz: Tz, live: &AppSession, from: i64, to: i64) {
    let (start, end) = (live.started_at.max(from), live.ended_at.min(to));
    if end - start < MIN_SESSION_MS {
        return;
    }
    let seconds = (end - start) / 1000;
    summary.total_seconds += seconds;
    match summary.apps.iter_mut().find(|a| a.app == live.app) {
        Some(a) => {
            a.total_seconds += seconds;
            a.sessions += 1;
        }
        None => summary.apps.push(AppUsage {
            app: live.app.clone(),
            category: live.category.clone(),
            is_browser: live.is_browser,
            total_seconds: seconds,
            sessions: 1,
        }),
    }
    summary.apps.sort_by(|a, b| {
        b.total_seconds
            .cmp(&a.total_seconds)
            .then_with(|| a.app.cmp(&b.app))
    });
    let mut hours = [0i64; 24];
    add_to_hours(&mut hours, tz, start, end);
    for (slot, ms) in summary.hourly_seconds.iter_mut().zip(hours) {
        *slot += ms / 1000;
    }
}

/// Split `[start, end)` at local hour boundaries and add each piece (ms) to its hour of day.
fn add_to_hours(hours: &mut [i64; 24], tz: Tz, start: i64, end: i64) {
    let mut t = start;
    while t < end {
        let Some(utc) = DateTime::<Utc>::from_timestamp_millis(t) else {
            return;
        };
        let local = utc.with_timezone(&tz);
        let into_hour = i64::from(local.minute() * 60 + local.second()) * 1000
            + i64::from(local.timestamp_subsec_millis() % 1000);
        let piece_end = (t + 3_600_000 - into_hour).min(end);
        hours[local.hour() as usize] += piece_end - t;
        t = piece_end;
    }
}

#[cfg(test)]
mod tests;
