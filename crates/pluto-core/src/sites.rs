//! Website time without a browser extension (`tracking.sites`, off by default; owner decision
//! 2026-10-12, Phase 2 "address bar first").
//!
//! The shell reads the address bar of the browser window in front (Windows: UI Automation) every
//! couple of seconds while a browser is in front. Everything that decides anything is here and
//! tested: [`domain_from_address`] keeps only the host and throws the rest of the address away,
//! and [`SiteTracker`] turns those readings into [`DomainSession`]s. Nothing but the host ever
//! leaves this module; private and incognito windows are never read into a session.

use serde::{Deserialize, Serialize};

use crate::usage::{normalize_host, DomainSession, IDLE_THRESHOLD_MS, SLEEP_GAP_MS};

/// How often the shell looks at the address bar while a browser is in front.
pub const LOOK_EVERY_MS: u64 = 2_000;

/// How often the site in front is written to the usage journal (crash safety, see
/// `usage::journal`): at most this much time on a site is lost to a crash or power cut.
pub const CHECKPOINT_EVERY_MS: i64 = 30_000;

/// One open tab, read live on request (owner decision 2026-10-13: no browser extension; the
/// shell reads this with the same permission already granted for `tracking.sites`). Never
/// stored — this is a live list, not a usage record.
///
/// `domain` is set only where the OS can say it without switching to the tab: always on macOS
/// (the browser's scripting dictionary lists every tab's URL directly); on Windows, only for the
/// tab that is actually showing (its address bar can be read; a background tab's page is not
/// rendered, so UI Automation has nothing to read for it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct OpenTab {
    pub browser: String,
    /// Which browser window, 0-based; stable only within one `pet_open_tabs` reading.
    pub window_index: u32,
    /// Which tab in that window, 0-based; pass both indices back to `pet_close_tab`.
    pub tab_index: u32,
    pub title: Option<String>,
    pub domain: Option<String>,
    pub active: bool,
}

/// The host of what the address bar shows, or `None` when it isn't a website: a browser page
/// (`chrome://`, `edge://`, `about:`), a file, a search being typed, or empty. `www.` is dropped
/// so `www.github.com` and `github.com` count as one site. The path, query, port and any login
/// in the address are never returned.
pub fn domain_from_address(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() || text.chars().any(char::is_whitespace) {
        return None; // empty, or words being typed into the bar
    }
    let lower = text.to_lowercase();
    let rest = if let Some((scheme, rest)) = lower.split_once("://") {
        if scheme != "http" && scheme != "https" {
            return None;
        }
        rest
    } else {
        // Browsers hide `https://`; anything else with a scheme-like prefix is a browser page.
        if lower.starts_with("about:")
            || lower.starts_with("data:")
            || lower.starts_with("javascript:")
        {
            return None;
        }
        &lower
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = if host_port.starts_with('[') {
        return None; // IPv6 literal: not a site worth naming
    } else {
        host_port.split(':').next().unwrap_or_default()
    };
    let host = host.strip_prefix("www.").unwrap_or(host);
    if !(host.contains('.') || host == "localhost") {
        return None; // a single word is a search, not a site
    }
    normalize_host(host).ok()
}

/// What the shell saw on one look.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seen {
    /// No browser in front (or the person is in another app).
    NoBrowser,
    /// A browser is in front but there is no site to count: a private window, a browser page,
    /// or the address bar couldn't be read.
    Nothing,
    /// A browser in front showing this site.
    Site { browser: String, domain: String },
}

struct Open {
    browser: String,
    domain: String,
    since: i64,
}

/// Turns looks at the address bar into finished website sessions.
#[derive(Default)]
pub struct SiteTracker {
    open: Option<Open>,
    last_look: Option<i64>,
}

impl SiteTracker {
    /// One look at `now`, with `idle_ms` since the last keyboard or mouse input. Returns the
    /// session that ended, if one did.
    ///
    /// * Idle for [`IDLE_THRESHOLD_MS`]: the session ends when the input stopped.
    /// * A gap much longer than the look interval (the computer slept): it ends at the last look.
    /// * A different site, a private window or another app: it ends now.
    pub fn look(&mut self, now: i64, idle_ms: u64, seen: Seen) -> Option<DomainSession> {
        let mut ended = None;
        if let Some(before) = self.last_look {
            if now - before > SLEEP_GAP_MS {
                ended = self.close(before);
            }
        }
        self.last_look = Some(now);
        if idle_ms >= IDLE_THRESHOLD_MS {
            let last_input = now.saturating_sub(i64::try_from(idle_ms).unwrap_or(i64::MAX));
            return ended.or_else(|| self.close(last_input));
        }
        let same = matches!((&self.open, &seen), (Some(o), Seen::Site { browser, domain }) if &o.browser == browser && &o.domain == domain);
        if same {
            return ended;
        }
        let closed = self.close(now);
        if let Seen::Site { browser, domain } = seen {
            self.open = Some(Open {
                browser,
                domain,
                since: now,
            });
        }
        ended.or(closed)
    }

    /// The stretch in progress, for the crash journal. It runs to the person's last input (not
    /// to `now`), so time spent away is never credited. `None` when no site is in front.
    pub fn open_session(&self, now: i64, idle_ms: u64) -> Option<DomainSession> {
        let open = self.open.as_ref()?;
        let last_input = now.saturating_sub(i64::try_from(idle_ms).unwrap_or(i64::MAX));
        Some(DomainSession {
            browser: open.browser.clone(),
            domain: open.domain.clone(),
            started_at: open.since,
            ended_at: last_input.max(open.since),
        })
    }

    /// End the open session now (tracking switched off, or Pluto is quitting).
    pub fn stop(&mut self, now: i64) -> Option<DomainSession> {
        self.last_look = None;
        self.close(now)
    }

    fn close(&mut self, at: i64) -> Option<DomainSession> {
        let open = self.open.take()?;
        (at > open.since).then_some(DomainSession {
            browser: open.browser,
            domain: open.domain,
            started_at: open.since,
            ended_at: at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_the_host() {
        for (input, want) in [
            ("github.com/anthropics/claude?tab=1#top", Some("github.com")),
            ("https://www.YouTube.com/watch?v=abc", Some("youtube.com")),
            (
                "http://user:secret@intranet.example.org:8080/a",
                Some("intranet.example.org"),
            ),
            ("localhost:5173/notes", Some("localhost")),
            ("docs.google.com", Some("docs.google.com")),
            ("chrome://settings", None),
            ("edge://newtab", None),
            ("about:blank", None),
            ("file:///C:/secret.pdf", None),
            ("how to cook rice", None),
            ("rust", None),
            ("", None),
            ("[::1]:3000", None),
        ] {
            assert_eq!(domain_from_address(input).as_deref(), want, "{input}");
        }
    }

    fn site(d: &str) -> Seen {
        Seen::Site {
            browser: "Google Chrome".into(),
            domain: d.into(),
        }
    }

    #[test]
    fn one_session_per_stretch_on_a_site() {
        let mut t = SiteTracker::default();
        assert_eq!(t.look(0, 0, site("github.com")), None);
        assert_eq!(t.look(2_000, 0, site("github.com")), None);
        let s = t.look(4_000, 0, site("youtube.com")).unwrap();
        assert_eq!(
            (s.domain.as_str(), s.started_at, s.ended_at),
            ("github.com", 0, 4_000)
        );
        let s = t.look(6_000, 0, Seen::NoBrowser).unwrap();
        assert_eq!(
            (s.domain.as_str(), s.started_at, s.ended_at),
            ("youtube.com", 4_000, 6_000)
        );
        assert_eq!(t.look(8_000, 0, Seen::NoBrowser), None);
    }

    #[test]
    fn a_private_window_ends_the_session_and_records_nothing() {
        let mut t = SiteTracker::default();
        t.look(0, 0, site("github.com"));
        assert!(t.look(2_000, 0, Seen::Nothing).is_some());
        assert_eq!(t.look(60_000, 0, Seen::Nothing), None);
        assert_eq!(t.stop(70_000), None);
    }

    #[test]
    fn idle_ends_the_session_when_input_stopped() {
        let mut t = SiteTracker::default();
        t.look(0, 0, site("netflix.com"));
        let idle = IDLE_THRESHOLD_MS;
        let now = 800_000;
        // Looks every 2 s while the film plays and nobody touches anything.
        let mut at = 2_000;
        while at < now {
            let quiet = u64::try_from(at - 100_000).unwrap_or(0);
            assert_eq!(t.look(at, quiet.min(idle - 1), site("netflix.com")), None);
            at += 2_000;
        }
        let s = t.look(now, idle, site("netflix.com")).unwrap();
        assert_eq!(s.ended_at, now - idle as i64);
        assert_eq!(t.look(now + 2_000, idle + 2_000, site("netflix.com")), None);
        // Back at the keyboard: a new session starts.
        assert_eq!(t.look(now + 4_000, 0, site("netflix.com")), None);
        assert_eq!(t.stop(now + 10_000).unwrap().started_at, now + 4_000);
    }

    #[test]
    fn sleep_ends_the_session_at_the_last_look() {
        let mut t = SiteTracker::default();
        t.look(0, 0, site("github.com"));
        t.look(2_000, 0, site("github.com"));
        let s = t
            .look(2_000 + SLEEP_GAP_MS + 1, 0, site("github.com"))
            .unwrap();
        assert_eq!(s.ended_at, 2_000);
    }

    #[test]
    fn the_open_site_for_the_journal_runs_to_the_last_input_not_to_now() {
        let mut t = SiteTracker::default();
        assert_eq!(t.open_session(10_000, 0), None, "nothing in front");
        assert_eq!(t.look(1_000, 0, site("github.com")), None);
        let open = t.open_session(61_000, 0).unwrap();
        assert_eq!(
            (open.domain.as_str(), open.started_at, open.ended_at),
            ("github.com", 1_000, 61_000)
        );
        // The person walked away 40 s ago: they are credited only up to then.
        assert_eq!(t.open_session(61_000, 40_000).unwrap().ended_at, 21_000);
        // Never before it started, even for a huge idle value.
        assert_eq!(t.open_session(61_000, u64::MAX).unwrap().ended_at, 1_000);
        // After the site ends there is nothing to remember.
        t.stop(70_000);
        assert_eq!(t.open_session(80_000, 0), None);
    }
}
