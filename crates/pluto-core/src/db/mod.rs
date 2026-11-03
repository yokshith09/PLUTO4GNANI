//! SQLite access (ADR-005): one file, WAL, foreign keys on.
//!
//! This module opens and migrates the database. The single writer thread and the read pool
//! build on it (F0-05).

pub mod handle;
pub mod migrate;

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::Connection;

use crate::error::{AppError, Result};
pub use handle::Database;
pub use migrate::{Migration, MIGRATIONS};

/// Open `path` and set the PRAGMAs every Pluto connection needs (Schema §1).
pub fn open_connection(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    configure(&conn)?;
    Ok(conn)
}

/// An in-memory database with the same PRAGMAs, for tests.
pub fn open_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    configure(&conn)?;
    Ok(conn)
}

fn configure(conn: &Connection) -> Result<()> {
    conn.busy_timeout(Duration::from_millis(5000))?;
    conn.pragma_update(None, "foreign_keys", true)?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    // Returns the resulting mode ("wal", or "memory" for in-memory databases).
    let _mode: String =
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;
    Ok(())
}

/// Open the database at `path`, refuse to touch it if it is damaged, and bring it up to date.
///
/// A damaged file is left exactly as found; the error names the newest backup, if there is one,
/// so the blocking screen (S-90) can offer to restore it.
pub fn open_and_migrate(path: &Path) -> Result<Connection> {
    let existed = path.exists();
    let mut conn = open_connection(path).map_err(|_| damaged(path))?;
    if existed && quick_check(&conn).is_err() {
        return Err(damaged(path));
    }
    migrate::migrate(&mut conn, MIGRATIONS)?;
    Ok(conn)
}

/// `PRAGMA quick_check` — cheap enough to run at every startup.
pub fn quick_check(conn: &Connection) -> Result<()> {
    let verdict: String = conn.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if verdict == "ok" {
        Ok(())
    } else {
        Err(AppError::db("The database failed its integrity check."))
    }
}

fn damaged(path: &Path) -> AppError {
    let hint = match migrate::latest_backup(path) {
        Some(backup) => format!(" The newest backup is {}.", backup.display()),
        None => String::new(),
    };
    AppError::db(format!(
        "Pluto found a problem in its database and has left it untouched.{hint}"
    ))
}

/// `<db>.bak-<version>` next to the database.
pub(crate) fn backup_path(db: &Path, from_version: u32) -> PathBuf {
    let mut name = db.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(format!(".bak-{from_version}"));
    db.with_file_name(name)
}

#[cfg(test)]
mod scale;
#[cfg(test)]
mod tests;
