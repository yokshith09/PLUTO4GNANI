//! Crash-safe time tracking (migration 011).
//!
//! The stretch in front right now lives in the tracker's memory and is written when it ends. If
//! Pluto is killed, Windows crashes or the power goes, that stretch used to vanish. So every look
//! (about a minute) the collector also *checkpoints* it here: one row per kind (`app`, `site`)
//! holding what is in front, when it started and when it was last seen. At the next start
//! [`UsageService::recover_open`] turns each row into a normal session that ends at `last_seen`
//! and clears the table.
//!
//! * The most that can be lost is the time since the last checkpoint (one look), which is the
//!   physical limit: a power cut cannot be saved from.
//! * Recovery never invents time: a stretch ends at the last time it was actually seen, never at
//!   the moment of recovery, so a night off does not turn into a night of work.
//! * Recovery never double-counts: a stretch that was also written normally (same name and same
//!   start) is skipped.
//! * Recovery goes through the same checks as any session (minimum, 12 h cap, not in the future,
//!   untracked sites refused).
//! * Deleting usage data, forgetting a site and "delete everything" all clear the journal, so
//!   nothing deleted can come back.

use rusqlite::{params, Transaction};

use super::{
    clean_name, normalize_host, store, AppSession, DomainSession, UsageRecord, UsageService,
};
use crate::error::{ErrorCode, Result};
use crate::events::Event;

const APP: &str = "app";
const SITE: &str = "site";

/// What is written for one kind.
struct Row {
    kind: &'static str,
    name: String,
    category: String,
    is_browser: bool,
    browser: String,
    started_at: i64,
    last_seen: i64,
}

impl UsageService {
    /// Remember the app in front (`Some`), or that none is (`None`). `open.ended_at` is when it
    /// was last seen to be in use.
    pub async fn checkpoint_app(&self, open: Option<&AppSession>) -> Result<()> {
        let row = match open {
            Some(s) => Some(Row {
                kind: APP,
                name: clean_name("app", &s.app)?,
                category: s.category.clone(),
                is_browser: s.is_browser,
                browser: String::new(),
                started_at: s.started_at,
                last_seen: s.ended_at.max(s.started_at),
            }),
            None => None,
        };
        self.write_checkpoint(APP, row).await
    }

    /// Remember the site in front (`Some`), or that none is (`None`).
    pub async fn checkpoint_site(&self, open: Option<&DomainSession>) -> Result<()> {
        let row = match open {
            Some(s) => Some(Row {
                kind: SITE,
                name: normalize_host(&s.domain)?,
                category: "Other".to_owned(),
                is_browser: true,
                browser: clean_name("browser", &s.browser)?,
                started_at: s.started_at,
                last_seen: s.ended_at.max(s.started_at),
            }),
            None => None,
        };
        self.write_checkpoint(SITE, row).await
    }

    async fn write_checkpoint(&self, kind: &'static str, row: Option<Row>) -> Result<()> {
        self.db
            .write(move |tx| {
                match &row {
                    Some(r) => {
                        tx.execute(
                            "INSERT INTO usage_open (kind, name, category, is_browser, browser, started_at, last_seen)
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                             ON CONFLICT(kind) DO UPDATE SET
                               name = excluded.name, category = excluded.category,
                               is_browser = excluded.is_browser, browser = excluded.browser,
                               started_at = excluded.started_at, last_seen = excluded.last_seen",
                            params![r.kind, r.name, r.category, r.is_browser, r.browser, r.started_at, r.last_seen],
                        )?;
                    }
                    None => {
                        tx.execute("DELETE FROM usage_open WHERE kind = ?1", [kind])?;
                    }
                }
                Ok(((), vec![]))
            })
            .await
    }

    /// Turn whatever a previous run left in the journal into sessions. Call once at start, before
    /// tracking begins. Returns how many sessions were saved.
    pub async fn recover_open(&self) -> Result<u64> {
        let now = self.clock.now_ms();
        self.db
            .write(move |tx| {
                let rows = read_rows(tx)?;
                let mut saved = 0u64;
                for row in rows {
                    // Never later than now (a clock set back), never before it started.
                    let ended_at = row.last_seen.min(now).max(row.started_at);
                    let record = if row.kind == SITE {
                        UsageRecord::Domain(DomainSession {
                            browser: row.browser,
                            domain: row.name,
                            started_at: row.started_at,
                            ended_at,
                        })
                    } else {
                        UsageRecord::App(AppSession {
                            app: row.name,
                            category: row.category,
                            is_browser: row.is_browser,
                            started_at: row.started_at,
                            ended_at,
                        })
                    };
                    if already_stored(tx, &record)? {
                        continue;
                    }
                    match store(tx, &record, now) {
                        Ok(true) => saved += 1,
                        Ok(false) => {}
                        Err(e) if e.code == ErrorCode::Validation => {
                            tracing::warn!("dropped an unrecoverable open stretch");
                        }
                        Err(e) => return Err(e),
                    }
                }
                tx.execute("DELETE FROM usage_open", [])?;
                let events = if saved > 0 {
                    vec![Event::UsageUpdated { at: now }]
                } else {
                    vec![]
                };
                Ok((saved, events))
            })
            .await
    }
}

fn read_rows(tx: &Transaction) -> Result<Vec<Row>> {
    let mut stmt = tx.prepare(
        "SELECT kind, name, category, is_browser, browser, started_at, last_seen FROM usage_open",
    )?;
    let rows = stmt
        .query_map([], |r| {
            let kind: String = r.get(0)?;
            Ok(Row {
                kind: if kind == SITE { SITE } else { APP },
                name: r.get(1)?,
                category: r.get(2)?,
                is_browser: r.get(3)?,
                browser: r.get(4)?,
                started_at: r.get(5)?,
                last_seen: r.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// The stretch was also written normally (same name, same start): the journal copy is redundant.
fn already_stored(tx: &Transaction, record: &UsageRecord) -> Result<bool> {
    let found: bool = match record {
        UsageRecord::App(s) => match clean_name("app", &s.app) {
            Ok(app) => tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM app_sessions WHERE app = ?1 AND started_at = ?2)",
                params![app, s.started_at],
                |r| r.get(0),
            )?,
            Err(_) => false,
        },
        UsageRecord::Domain(s) => match normalize_host(&s.domain) {
            Ok(domain) => tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM domain_sessions WHERE domain = ?1 AND started_at = ?2)",
                params![domain, s.started_at],
                |r| r.get(0),
            )?,
            Err(_) => false,
        },
    };
    Ok(found)
}

impl UsageService {
    /// Forget journal rows overlapping `[from, to)` (used by the delete actions).
    pub(super) fn clear_range(tx: &Transaction, from: i64, to: i64) -> Result<()> {
        tx.execute(
            "DELETE FROM usage_open WHERE started_at < ?2 AND last_seen > ?1",
            params![from, to],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
