//! Firefox's open tabs, read from its own session file (macOS website time and tabs, owner
//! 2026-10-23). Firefox has no AppleScript, so instead of asking it, Pluto reads
//! `sessionstore-backups/recovery.jsonlz4`, which Firefox rewrites every ~15 seconds.
//! **Private windows are never written to that file**, so they are never seen. Only the
//! address of each tab is used, reduced to its domain; titles only for the open-tabs list.
//!
//! The file is "mozlz4": the 8 bytes `mozLz40\0`, the decompressed size (u32, little endian),
//! then one LZ4 block.

use crate::error::{AppError, Result};
use crate::sites::{domain_from_address, OpenTab};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 8] = b"mozLz40\0";
/// Larger than any real session file; refuses a corrupt size before allocating.
const MAX_SESSION_BYTES: usize = 64 * 1024 * 1024;

/// Decompress a mozlz4 file.
pub fn decode_mozlz4(bytes: &[u8]) -> Result<Vec<u8>> {
    let bad = |m: &str| AppError::validation("firefox", m);
    if bytes.len() < 12 || &bytes[..8] != MAGIC {
        return Err(bad("not a Firefox session file"));
    }
    let size = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
    if size > MAX_SESSION_BYTES {
        return Err(bad("session file too large"));
    }
    lz4_flex::block::decompress(&bytes[12..], size).map_err(|_| bad("session file is damaged"))
}

#[derive(Deserialize, Default)]
struct Session {
    #[serde(default)]
    windows: Vec<Window>,
    /// 1-based index of the window last in front.
    #[serde(default, rename = "selectedWindow")]
    selected_window: usize,
}

#[derive(Deserialize, Default)]
struct Window {
    #[serde(default)]
    tabs: Vec<Tab>,
    /// 1-based index of the selected tab.
    #[serde(default)]
    selected: usize,
}

#[derive(Deserialize, Default)]
struct Tab {
    #[serde(default)]
    entries: Vec<Entry>,
    /// 1-based index into `entries` (the tab's back/forward history): the page showing.
    #[serde(default)]
    index: usize,
}

#[derive(Deserialize, Default)]
struct Entry {
    #[serde(default)]
    url: String,
    #[serde(default)]
    title: Option<String>,
}

impl Tab {
    fn current(&self) -> Option<&Entry> {
        let i = self.index.max(1) - 1;
        self.entries.get(i).or_else(|| self.entries.last())
    }
}

fn parse(json: &[u8]) -> Result<Session> {
    serde_json::from_slice(json)
        .map_err(|_| AppError::validation("firefox", "session file is not valid"))
}

/// The domain of the tab showing in the window last in front, if it is a website.
pub fn active_domain(json: &[u8]) -> Result<Option<String>> {
    let s = parse(json)?;
    let w = s
        .windows
        .get(s.selected_window.max(1) - 1)
        .or_else(|| s.windows.first());
    let tab = w.and_then(|w| w.tabs.get(w.selected.max(1) - 1));
    Ok(tab
        .and_then(Tab::current)
        .and_then(|e| domain_from_address(&e.url)))
}

/// Every open tab (private windows are not in the file), for the open-tabs list.
pub fn open_tabs(json: &[u8], browser: &str) -> Result<Vec<OpenTab>> {
    let s = parse(json)?;
    let mut out = vec![];
    for (wi, w) in s.windows.iter().enumerate() {
        for (ti, t) in w.tabs.iter().enumerate() {
            let Some(e) = t.current() else { continue };
            out.push(OpenTab {
                browser: browser.to_owned(),
                window_index: wi as u32,
                tab_index: ti as u32,
                title: e.title.clone().filter(|s| !s.is_empty()),
                domain: domain_from_address(&e.url),
                active: wi + 1 == s.selected_window.max(1) && ti + 1 == w.selected.max(1),
            });
        }
    }
    Ok(out)
}

/// The most recently written session file among Firefox profiles under `profiles_dir`
/// (macOS: `~/Library/Application Support/Firefox/Profiles`).
pub fn newest_session_file(profiles_dir: &Path) -> Option<PathBuf> {
    fs::read_dir(profiles_dir)
        .ok()?
        .flatten()
        .map(|p| {
            p.path()
                .join("sessionstore-backups")
                .join("recovery.jsonlz4")
        })
        .filter_map(|f| Some((fs::metadata(&f).ok()?.modified().ok()?, f)))
        .max_by_key(|(t, _)| *t)
        .map(|(_, f)| f)
}

/// Read and decode a session file.
pub fn read_session(file: &Path) -> Result<Vec<u8>> {
    let bytes = fs::read(file).map_err(|e| AppError::io(e.to_string()))?;
    decode_mozlz4(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mozlz4(json: &str) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        out.extend((json.len() as u32).to_le_bytes());
        out.extend(lz4_flex::block::compress(json.as_bytes()));
        out
    }

    const SESSION: &str = r#"{
      "selectedWindow": 2,
      "windows": [
        { "selected": 1, "tabs": [ { "index": 1, "entries": [ { "url": "https://github.com/x", "title": "GitHub" } ] } ] },
        { "selected": 2, "tabs": [
            { "index": 1, "entries": [ { "url": "about:newtab", "title": "New Tab" } ] },
            { "index": 2, "entries": [ { "url": "https://old.example.com" }, { "url": "https://www.khanacademy.org/math", "title": "Math" } ] }
        ] }
      ]
    }"#;

    #[test]
    fn decodes_a_mozlz4_file() {
        assert_eq!(
            decode_mozlz4(&mozlz4("{\"windows\":[]}")).unwrap(),
            b"{\"windows\":[]}"
        );
        assert!(decode_mozlz4(b"plain json, not mozlz4").is_err());
        let mut huge = MAGIC.to_vec();
        huge.extend(u32::MAX.to_le_bytes());
        assert!(decode_mozlz4(&huge).is_err());
    }

    #[test]
    fn finds_the_site_showing_in_the_front_window() {
        let json = decode_mozlz4(&mozlz4(SESSION)).unwrap();
        // Window 2, tab 2, history entry 2 → khanacademy.org (www. dropped, path dropped).
        assert_eq!(
            active_domain(&json).unwrap().as_deref(),
            Some("khanacademy.org")
        );
        let blank = br#"{"selectedWindow":1,"windows":[{"selected":1,"tabs":[{"index":1,"entries":[{"url":"about:home"}]}]}]}"#;
        assert_eq!(active_domain(blank).unwrap(), None);
        assert_eq!(active_domain(b"{}").unwrap(), None);
        assert!(active_domain(b"not json").is_err());
    }

    #[test]
    fn lists_every_tab_with_the_active_one_marked() {
        let tabs = open_tabs(SESSION.as_bytes(), "Firefox").unwrap();
        assert_eq!(tabs.len(), 3);
        assert_eq!(tabs[0].domain.as_deref(), Some("github.com"));
        assert!(!tabs[0].active);
        assert_eq!(tabs[1].domain, None);
        assert!(tabs[2].active);
        assert_eq!((tabs[2].window_index, tabs[2].tab_index), (1, 1));
        assert_eq!(tabs[2].title.as_deref(), Some("Math"));
    }

    #[test]
    fn picks_the_newest_profile_session() {
        let t = tempfile::tempdir().unwrap();
        for p in ["a.default", "b.work"] {
            let d = t.path().join(p).join("sessionstore-backups");
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join("recovery.jsonlz4"), mozlz4(SESSION)).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let newest = newest_session_file(t.path()).unwrap();
        assert!(newest.to_string_lossy().contains("b.work"));
        assert!(read_session(&newest).is_ok());
        assert!(newest_session_file(&t.path().join("missing")).is_none());
    }
}
