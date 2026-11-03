//! Local-only logging and crash reports (ADR-012: nothing leaves the device).
//!
//! * Logs go to `<app-data>/logs/pluto.log`, rotated by size: 5 files × 2 MB. `tracing-appender`
//!   rotates by time only, so [`RotatingWriter`] does the size-based part.
//! * No log line at `info` or above carries user content (TRD §6.9). [`Sensitive`] makes it
//!   hard to leak by accident at lower levels, and a test scans the crate's own source.
//! * A panic writes `crash-<ms>.txt` next to the logs, synchronously, before anything else.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use tracing::level_filters::LevelFilter;

pub const LOG_STEM: &str = "pluto";
pub const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;
pub const KEEP_LOG_FILES: usize = 5;

/// A file writer that rotates by size, keeping at most `keep` files:
/// `pluto.log` (newest), `pluto.1.log`, …, `pluto.{keep-1}.log` (oldest, deleted on the next rotation).
pub struct RotatingWriter {
    dir: PathBuf,
    stem: String,
    max_bytes: u64,
    keep: usize,
    file: File,
    size: u64,
}

impl RotatingWriter {
    pub fn new(
        dir: impl Into<PathBuf>,
        stem: &str,
        max_bytes: u64,
        keep: usize,
    ) -> io::Result<Self> {
        assert!(keep >= 1 && max_bytes >= 1);
        let dir = dir.into();
        fs::create_dir_all(&dir)?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(format!("{stem}.log")))?;
        let size = file.metadata()?.len();
        Ok(Self {
            dir,
            stem: stem.to_owned(),
            max_bytes,
            keep,
            file,
            size,
        })
    }

    fn path(&self, index: usize) -> PathBuf {
        if index == 0 {
            self.dir.join(format!("{}.log", self.stem))
        } else {
            self.dir.join(format!("{}.{index}.log", self.stem))
        }
    }

    fn rotate(&mut self) -> io::Result<()> {
        self.file.flush()?;
        let oldest = self.keep - 1;
        if oldest == 0 {
            // keep == 1: just start the single file over.
            self.file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(self.path(0))?;
            self.size = 0;
            return Ok(());
        }
        match fs::remove_file(self.path(oldest)) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        for i in (0..oldest).rev() {
            match fs::rename(self.path(i), self.path(i + 1)) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        self.file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.path(0))?;
        self.size = 0;
        Ok(())
    }
}

impl Write for RotatingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // A single event bigger than the cap is truncated so no file ever exceeds `max_bytes`.
        let chunk = &buf[..buf.len().min(self.max_bytes as usize)];
        if self.size > 0 && self.size + chunk.len() as u64 > self.max_bytes {
            self.rotate()?;
        }
        self.file.write_all(chunk)?;
        self.size += chunk.len() as u64;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

/// The levels users can choose in Settings › Advanced (`advanced.log_level`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
}

impl LogLevel {
    pub fn filter(self) -> LevelFilter {
        match self {
            Self::Debug => LevelFilter::DEBUG,
            Self::Info => LevelFilter::INFO,
            Self::Warn => LevelFilter::WARN,
        }
    }
}

impl FromStr for LogLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "debug" => Ok(Self::Debug),
            "info" => Ok(Self::Info),
            "warn" => Ok(Self::Warn),
            other => Err(format!(
                "unknown log level {other:?}; expected debug, info or warn"
            )),
        }
    }
}

/// Keep this alive for the life of the process: dropping it flushes and stops the log thread.
pub struct LogGuard(#[allow(dead_code)] tracing_appender::non_blocking::WorkerGuard);

/// Build a subscriber that writes to `writer` without blocking the caller.
pub fn build_subscriber(
    writer: impl Write + Send + 'static,
    level: LogLevel,
) -> (impl tracing::Subscriber + Send + Sync, LogGuard) {
    let (non_blocking, guard) = tracing_appender::non_blocking(writer);
    let subscriber = tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_max_level(level.filter())
        .finish();
    (subscriber, LogGuard(guard))
}

/// Changes the log level of a running logger (Settings › Advanced, no restart needed).
pub struct LevelHandle(
    tracing_subscriber::reload::Handle<LevelFilter, tracing_subscriber::Registry>,
);

impl LevelHandle {
    pub fn set(&self, level: LogLevel) {
        let _ = self.0.modify(|f| *f = level.filter());
    }
}

/// Like [`build_subscriber`], with a level that can change later.
pub fn build_reloadable(
    writer: impl Write + Send + 'static,
    level: LogLevel,
) -> (
    impl tracing::Subscriber + Send + Sync,
    LogGuard,
    LevelHandle,
) {
    use tracing_subscriber::layer::SubscriberExt;
    let (non_blocking, guard) = tracing_appender::non_blocking(writer);
    let (filter, handle) = tracing_subscriber::reload::Layer::new(level.filter());
    let subscriber = tracing_subscriber::registry().with(filter).with(
        tracing_subscriber::fmt::layer()
            .with_writer(non_blocking)
            .with_ansi(false),
    );
    (subscriber, LogGuard(guard), LevelHandle(handle))
}

/// Install the process-wide logger writing to `<dir>/pluto.log` (5 × 2 MB). The level can be
/// changed later through the returned handle.
pub fn init_reloadable(dir: &Path, level: LogLevel) -> io::Result<(LogGuard, LevelHandle)> {
    let writer = RotatingWriter::new(dir, LOG_STEM, MAX_LOG_BYTES, KEEP_LOG_FILES)?;
    let (subscriber, guard, handle) = build_reloadable(writer, level);
    tracing::subscriber::set_global_default(subscriber).map_err(|_| {
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "a global logger is already installed",
        )
    })?;
    Ok((guard, handle))
}

/// Install the process-wide logger writing to `<dir>/pluto.log` (5 × 2 MB).
pub fn init(dir: &Path, level: LogLevel) -> io::Result<LogGuard> {
    let writer = RotatingWriter::new(dir, LOG_STEM, MAX_LOG_BYTES, KEEP_LOG_FILES)?;
    let (subscriber, guard) = build_subscriber(writer, level);
    tracing::subscriber::set_global_default(subscriber).map_err(|_| {
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "a global logger is already installed",
        )
    })?;
    Ok(guard)
}

/// Wrap anything user-authored before it goes near a log macro. It never prints the value.
pub struct Sensitive<T>(pub T);

impl<T> fmt::Debug for Sensitive<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

impl<T> fmt::Display for Sensitive<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

/// Write `crash-<ms>.txt` into `dir`. Synchronous on purpose: it runs while the process is dying.
pub fn write_crash_report(
    dir: &Path,
    message: &str,
    location: Option<&str>,
) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let path = dir.join(format!("crash-{ms}.txt"));
    let backtrace = std::backtrace::Backtrace::force_capture();
    let body = format!(
        "Pluto crash report\nversion: {}\ntime_ms: {ms}\nlocation: {}\nmessage: {message}\n\nbacktrace:\n{backtrace}\n",
        crate::VERSION,
        location.unwrap_or("unknown"),
    );
    fs::write(&path, body)?;
    Ok(path)
}

/// On panic: log where (not what) at `error`, write the crash file, then run the previous hook.
pub fn install_panic_hook(dir: PathBuf) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = match info.payload().downcast_ref::<&str>() {
            Some(s) => (*s).to_owned(),
            None => info
                .payload()
                .downcast_ref::<String>()
                .cloned()
                .unwrap_or_else(|| "non-string panic".into()),
        };
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()));
        tracing::error!(
            location = location.as_deref().unwrap_or("unknown"),
            "Pluto panicked"
        );
        let _ = write_crash_report(&dir, &message, location.as_deref());
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn log_files(dir: &Path) -> Vec<PathBuf> {
        let mut v: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "log"))
            .collect();
        v.sort();
        v
    }

    #[test]
    fn rotation_never_keeps_more_than_5_files_or_exceeds_the_size_cap() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = RotatingWriter::new(dir.path(), "pluto", 100, 5).unwrap();
        for i in 0..400 {
            writeln!(w, "line number {i:04} padding padding padding").unwrap();
        }
        w.flush().unwrap();
        let files = log_files(dir.path());
        assert_eq!(
            files.len(),
            5,
            "exactly the 5 newest files remain: {files:?}"
        );
        for f in files {
            assert!(
                fs::metadata(&f).unwrap().len() <= 100,
                "{f:?} exceeds the cap"
            );
        }
    }

    #[test]
    fn newest_lines_are_in_the_current_file_and_the_oldest_are_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = RotatingWriter::new(dir.path(), "pluto", 100, 3).unwrap();
        for i in 0..200 {
            writeln!(w, "event-{i:04}").unwrap();
        }
        w.flush().unwrap();
        let current = fs::read_to_string(dir.path().join("pluto.log")).unwrap();
        assert!(
            current.contains("event-0199"),
            "newest event must be in pluto.log"
        );
        let all: String = log_files(dir.path())
            .iter()
            .map(|f| fs::read_to_string(f).unwrap())
            .collect();
        assert!(
            !all.contains("event-0000"),
            "oldest event must have been rotated away"
        );
    }

    #[test]
    fn an_event_larger_than_the_cap_is_truncated_not_allowed_to_overflow() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = RotatingWriter::new(dir.path(), "pluto", 50, 2).unwrap();
        w.write_all(&[b'x'; 500]).unwrap();
        w.flush().unwrap();
        assert!(fs::metadata(dir.path().join("pluto.log")).unwrap().len() <= 50);
    }

    #[test]
    fn restarting_appends_to_the_existing_file_instead_of_clobbering_it() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut w = RotatingWriter::new(dir.path(), "pluto", 1000, 2).unwrap();
            writeln!(w, "before restart").unwrap();
        }
        let mut w = RotatingWriter::new(dir.path(), "pluto", 1000, 2).unwrap();
        writeln!(w, "after restart").unwrap();
        w.flush().unwrap();
        let text = fs::read_to_string(dir.path().join("pluto.log")).unwrap();
        assert!(text.contains("before restart") && text.contains("after restart"));
    }

    #[test]
    fn the_level_can_change_while_running() {
        let dir = tempfile::tempdir().unwrap();
        let writer =
            RotatingWriter::new(dir.path(), "pluto", MAX_LOG_BYTES, KEEP_LOG_FILES).unwrap();
        let (subscriber, guard, level) = build_reloadable(writer, LogLevel::Warn);
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("before-change-info");
            level.set(LogLevel::Debug);
            tracing::debug!("after-change-debug");
        });
        drop(guard);
        let text = fs::read_to_string(dir.path().join("pluto.log")).unwrap();
        assert!(!text.contains("before-change-info"));
        assert!(text.contains("after-change-debug"));
    }

    #[test]
    fn the_configured_level_filters_what_reaches_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let writer =
            RotatingWriter::new(dir.path(), "pluto", MAX_LOG_BYTES, KEEP_LOG_FILES).unwrap();
        let (subscriber, guard) = build_subscriber(writer, LogLevel::Warn);
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("hidden-info-line");
            tracing::warn!("visible-warn-line");
        });
        drop(guard); // flushes the background writer
        let text = fs::read_to_string(dir.path().join("pluto.log")).unwrap();
        assert!(text.contains("visible-warn-line"));
        assert!(!text.contains("hidden-info-line"));
    }

    #[test]
    fn log_levels_parse_and_reject_unknown_values() {
        assert_eq!("debug".parse::<LogLevel>(), Ok(LogLevel::Debug));
        assert_eq!("info".parse::<LogLevel>(), Ok(LogLevel::Info));
        assert_eq!("warn".parse::<LogLevel>(), Ok(LogLevel::Warn));
        assert!("trace".parse::<LogLevel>().is_err());
    }

    #[test]
    fn sensitive_never_prints_its_value_even_when_nested() {
        let s = Sensitive("my private note");
        assert_eq!(format!("{s}"), "<redacted>");
        assert_eq!(format!("{s:?}"), "<redacted>");
        assert_eq!(
            format!("{:?}", Some(Sensitive("my private note"))),
            "Some(<redacted>)"
        );
    }

    #[test]
    fn crash_report_has_the_message_location_and_a_backtrace() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_crash_report(dir.path(), "boom", Some("src/x.rs:9")).unwrap();
        assert!(path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("crash-"));
        let text = fs::read_to_string(path).unwrap();
        assert!(text.contains("message: boom") && text.contains("location: src/x.rs:9"));
        assert!(text.contains("backtrace:") && text.contains("version:"));
    }

    #[test]
    fn the_panic_hook_writes_a_crash_file() {
        let dir = tempfile::tempdir().unwrap();
        let saved = std::panic::take_hook();
        install_panic_hook(dir.path().to_owned());
        let caught = std::panic::catch_unwind(|| panic!("deliberate test panic"));
        // Restore the harness's hook so other tests report normally.
        let _ = std::panic::take_hook();
        std::panic::set_hook(saved);
        assert!(caught.is_err());
        let crash = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("crash-")
            })
            .expect("a crash file must exist");
        assert!(fs::read_to_string(crash)
            .unwrap()
            .contains("deliberate test panic"));
    }

    // ---- content-free logging guard (TRD §6.9) -------------------------------------------------

    /// Words that name user-authored content. They must not appear inside an `info!`/`warn!`/`error!` call.
    const CONTENT_WORDS: &[&str] = &[
        "body",
        "title",
        "description",
        "transcript",
        "notes",
        "participants",
        "decisions",
        "assignee",
        "content",
    ];

    /// Returns the macro calls (at info level or above) that mention a content word.
    fn content_leaks(source: &str) -> Vec<String> {
        // Everything after the first `#[cfg(test)]` is test code and may mention anything.
        let production = source.split("#[cfg(test)]").next().unwrap_or(source);
        let mut leaks = Vec::new();
        for mac in ["info!(", "warn!(", "error!("] {
            for (start, _) in production.match_indices(mac) {
                let rest = &production[start..];
                let mut depth = 0i32;
                let mut end = rest.len();
                for (i, c) in rest.char_indices() {
                    match c {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                end = i + 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                let call = &rest[..end];
                let lowered = call.to_lowercase();
                let hit = CONTENT_WORDS.iter().any(|w| {
                    lowered.match_indices(w).any(|(i, _)| {
                        let before = lowered[..i].chars().next_back();
                        let after = lowered[i + w.len()..].chars().next();
                        !before.is_some_and(|c| c.is_alphanumeric() || c == '_')
                            && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
                    })
                });
                if hit {
                    leaks.push(call.to_owned());
                }
            }
        }
        leaks
    }

    fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }

    #[test]
    fn the_scanner_flags_a_leak_and_passes_clean_calls() {
        assert_eq!(
            content_leaks(r#"fn f() { info!(body = %note.body, "saved"); }"#).len(),
            1
        );
        assert_eq!(
            content_leaks(r#"fn f() { warn!("title is {}", t); }"#).len(),
            1
        );
        assert!(content_leaks(r#"fn f() { info!(count = 3, "rolled over"); }"#).is_empty());
        assert!(content_leaks(r#"fn f() { debug!(body = %b, "debug is allowed"); }"#).is_empty());
        assert!(
            content_leaks("fn f() {}\n#[cfg(test)]\nmod t { fn g() { info!(\"body\"); } }")
                .is_empty()
        );
        assert!(
            content_leaks(r#"fn f() { info!("subtitled and bodywork"); }"#).is_empty(),
            "whole words only"
        );
    }

    #[test]
    fn no_info_or_above_log_call_in_this_crate_mentions_user_content() {
        let mut files = Vec::new();
        rust_files(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut files,
        );
        assert!(!files.is_empty());
        let mut all = Vec::new();
        for f in files {
            for leak in content_leaks(&fs::read_to_string(&f).unwrap()) {
                all.push(format!("{}: {leak}", f.display()));
            }
        }
        assert!(
            all.is_empty(),
            "possible content in logs:\n{}",
            all.join("\n")
        );
    }
}
