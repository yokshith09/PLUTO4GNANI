//! Forward-only, tested migrations (ADR-007).
//!
//! **The runner owns the transaction and `PRAGMA user_version`.** It applies each pending file in
//! its own transaction and sets the version only after the file applies cleanly. A migration file
//! therefore contains no `BEGIN`/`COMMIT` and no `user_version` write; [`validate_chain`] enforces
//! that by executing the files, which is exact where a text search is not (trigger bodies contain
//! `BEGIN … END` legitimately).

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::error::{AppError, Result};

pub struct Migration {
    pub version: u32,
    pub sql: &'static str,
}

/// The schema source of truth is the documented migration file; there is no second copy.
pub static MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        sql: include_str!(
            "../../../../desktop-pet-docs/docs/05-backend/migrations/001_initial.sql"
        ),
    },
    Migration {
        version: 2,
        sql: include_str!(
            "../../../../desktop-pet-docs/docs/05-backend/migrations/002_bin_and_reminders.sql"
        ),
    },
    Migration {
        version: 3,
        sql: include_str!(
            "../../../../desktop-pet-docs/docs/05-backend/migrations/003_usage_tracking.sql"
        ),
    },
    Migration {
        version: 4,
        sql: include_str!(
            "../../../../desktop-pet-docs/docs/05-backend/migrations/004_task_plans.sql"
        ),
    },
    Migration {
        version: 5,
        sql: include_str!(
            "../../../../desktop-pet-docs/docs/05-backend/migrations/005_focus_sessions.sql"
        ),
    },
    Migration {
        version: 6,
        sql: include_str!(
            "../../../../desktop-pet-docs/docs/05-backend/migrations/006_task_bin.sql"
        ),
    },
    Migration {
        version: 7,
        sql: include_str!(
            "../../../../desktop-pet-docs/docs/05-backend/migrations/007_projects_and_task_reminders.sql"
        ),
    },
    Migration {
        version: 8,
        sql: include_str!(
            "../../../../desktop-pet-docs/docs/05-backend/migrations/008_courses_and_exams.sql"
        ),
    },
    Migration {
        version: 9,
        sql: include_str!(
            "../../../../desktop-pet-docs/docs/05-backend/migrations/009_diary.sql"
        ),
    },
    Migration {
        version: 10,
        sql: include_str!(
            "../../../../desktop-pet-docs/docs/05-backend/migrations/010_diary_recovery.sql"
        ),
    },
    Migration {
        version: 11,
        sql: include_str!(
            "../../../../desktop-pet-docs/docs/05-backend/migrations/011_usage_journal.sql"
        ),
    },
];

/// How many pre-migration backups to keep (newest by version).
pub const KEEP_BACKUPS: usize = 3;

pub fn user_version(conn: &Connection) -> Result<u32> {
    Ok(conn.query_row("PRAGMA user_version", [], |row| row.get(0))?)
}

/// Apply every migration newer than the database's `user_version`. Returns the resulting version.
///
/// Before changing an existing database it writes `<db>.bak-<from>` (not for a brand-new one).
/// A failing migration rolls back, leaving the file at the previous version.
pub fn migrate(conn: &mut Connection, migrations: &[Migration]) -> Result<u32> {
    let current = user_version(conn)?;
    let target = migrations.last().map_or(0, |m| m.version);
    if current > target {
        return Err(AppError::db(
            "This data was created by a newer version of Pluto. Update Pluto to open it; nothing was changed.",
        ));
    }
    let pending: Vec<&Migration> = migrations.iter().filter(|m| m.version > current).collect();
    if pending.is_empty() {
        return Ok(current);
    }
    if current > 0 {
        backup(conn, current)?;
    }
    for m in pending {
        let tx = conn.transaction()?;
        tx.execute_batch(m.sql).map_err(|_| {
            // The failing SQL is ours, not the user's, so naming the version is safe and useful.
            AppError::db(format!(
                "Pluto couldn't update its database (step {}). Your data is unchanged.",
                m.version
            ))
        })?;
        tx.pragma_update(None, "user_version", m.version)?;
        tx.commit()?;
    }
    Ok(target)
}

/// Copy the database to `<db>.bak-<from_version>` (`VACUUM INTO` is consistent under WAL), then
/// prune to the newest [`KEEP_BACKUPS`]. In-memory databases have nothing to copy.
fn backup(conn: &Connection, from_version: u32) -> Result<()> {
    let Some(db) = conn.path().filter(|p| !p.is_empty()).map(PathBuf::from) else {
        return Ok(());
    };
    let target = super::backup_path(&db, from_version);
    if target.exists() {
        fs::remove_file(&target)?;
    }
    let target_str = target
        .to_str()
        .ok_or_else(|| AppError::io("The data folder path isn't valid text."))?;
    conn.execute("VACUUM INTO ?1", [target_str])?;
    prune_backups(&db, KEEP_BACKUPS)?;
    Ok(())
}

/// `(version, path)` of every `<db>.bak-N` beside `db`, newest version first.
fn backups(db: &Path) -> Vec<(u32, PathBuf)> {
    let (Some(dir), Some(name)) = (db.parent(), db.file_name().and_then(|n| n.to_str())) else {
        return Vec::new();
    };
    let prefix = format!("{name}.bak-");
    let mut found: Vec<(u32, PathBuf)> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let file = entry.file_name().to_str()?.to_owned();
            let version = file.strip_prefix(&prefix)?.parse().ok()?;
            Some((version, entry.path()))
        })
        .collect();
    found.sort_by_key(|(version, _)| std::cmp::Reverse(*version));
    found
}

pub fn latest_backup(db: &Path) -> Option<PathBuf> {
    backups(db).into_iter().next().map(|(_, p)| p)
}

pub fn prune_backups(db: &Path, keep: usize) -> Result<()> {
    for (_, path) in backups(db).into_iter().skip(keep) {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Check that a migration chain is well-formed by running it from scratch on a scratch database
/// exactly as the runner would: versions are 1, 2, 3, … with no gaps, and no file starts or ends
/// the transaction or touches `user_version`.
pub fn validate_chain(migrations: &[Migration]) -> std::result::Result<(), String> {
    let conn = Connection::open_in_memory().map_err(|e| e.to_string())?;
    for (i, m) in migrations.iter().enumerate() {
        let expected = i as u32 + 1;
        if m.version != expected {
            return Err(format!(
                "migration versions must be 1, 2, 3, …; found {} at position {expected}",
                m.version
            ));
        }
        conn.execute_batch("BEGIN").map_err(|e| e.to_string())?;
        if let Err(e) = conn.execute_batch(m.sql) {
            let text = e.to_string();
            return Err(if text.contains("within a transaction") {
                format!(
                    "migration {} starts its own transaction (the runner owns it)",
                    m.version
                )
            } else {
                format!("migration {} does not apply: {text}", m.version)
            });
        }
        if conn.is_autocommit() {
            return Err(format!(
                "migration {} ends the transaction with COMMIT/END/ROLLBACK (the runner owns it)",
                m.version
            ));
        }
        let version: u32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if version != 0 {
            return Err(format!(
                "migration {} sets user_version (the runner owns it)",
                m.version
            ));
        }
        conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;
    }
    Ok(())
}
