//! Auto-delete (FR-46, rules approved by the owner 2026-10-23).
//!
//! Only two kinds of data ever go: **time history** (app and website sessions) and **completed
//! tasks**. Notes, meetings, reminders and open tasks are never touched. Each has its own age
//! (never / 30 / 90 / 180 / 365 days; never by default). Completed tasks move to the task Bin
//! and are deleted for good after [`BIN_DAYS`] there; time history has no Bin and is deleted.
//! [`preview`] counts what [`apply`] would do, so turning a rule on can show it first.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::clock::Clock;
use crate::db::Database;
use crate::error::{AppError, Result};
use crate::events::Event;
use crate::tasks::{self, Task};

/// Ages a rule may use, in days. 0 = never.
pub const ALLOWED_DAYS: [u32; 5] = [0, 30, 90, 180, 365];
pub const BIN_DAYS: i64 = 30;
const DAY_MS: i64 = 86_400_000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RetentionRules {
    /// Delete app and website time older than this many days. 0 = never.
    pub time_days: u32,
    /// Move tasks completed longer ago than this to the Bin. 0 = never.
    pub tasks_days: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RetentionCounts {
    /// App and website sessions deleted.
    pub time_sessions: u32,
    /// Completed tasks moved to the Bin.
    pub tasks_binned: u32,
    /// Tasks deleted for good after their time in the Bin.
    pub tasks_purged: u32,
}

fn check(rules: RetentionRules) -> Result<()> {
    for (field, d) in [
        ("time_days", rules.time_days),
        ("tasks_days", rules.tasks_days),
    ] {
        if !ALLOWED_DAYS.contains(&d) {
            return Err(AppError::validation(
                field,
                "Pick Never, 30 days, 90 days, 6 months or 1 year.",
            ));
        }
    }
    Ok(())
}

fn cutoff(now: i64, days: u32) -> i64 {
    now - i64::from(days) * DAY_MS
}

fn count(conn: &Connection, sql: &str, at: i64) -> Result<u32> {
    Ok(conn.query_row(sql, [at], |r| r.get::<_, i64>(0))? as u32)
}

const OLD_TIME: [&str; 2] = [
    "SELECT COUNT(*) FROM app_sessions WHERE ended_at < ?1",
    "SELECT COUNT(*) FROM domain_sessions WHERE ended_at < ?1",
];
const OLD_DONE: &str = "SELECT COUNT(*) FROM tasks WHERE status = 'COMPLETED' AND binned_at IS NULL AND completed_at < ?1";
const BIN_EXPIRED: &str =
    "SELECT COUNT(*) FROM tasks WHERE binned_at IS NOT NULL AND binned_at < ?1";

fn counts(conn: &Connection, rules: RetentionRules, now: i64) -> Result<RetentionCounts> {
    let mut c = RetentionCounts::default();
    if rules.time_days > 0 {
        let at = cutoff(now, rules.time_days);
        c.time_sessions = count(conn, OLD_TIME[0], at)? + count(conn, OLD_TIME[1], at)?;
    }
    if rules.tasks_days > 0 {
        c.tasks_binned = count(conn, OLD_DONE, cutoff(now, rules.tasks_days))?;
    }
    c.tasks_purged = count(conn, BIN_EXPIRED, now - BIN_DAYS * DAY_MS)?;
    Ok(c)
}

/// What [`apply`] would do now with these rules.
pub async fn preview(
    db: &Database,
    clock: &dyn Clock,
    rules: RetentionRules,
) -> Result<RetentionCounts> {
    check(rules)?;
    let now = clock.now_ms();
    db.read(move |conn| counts(conn, rules, now)).await
}

/// Apply the rules: delete old time history, move old completed tasks to the Bin, and empty
/// tasks that have been in the Bin for [`BIN_DAYS`]. Says so with one `RetentionApplied` event
/// when anything changed.
pub async fn apply(
    db: &Database,
    clock: &dyn Clock,
    rules: RetentionRules,
) -> Result<RetentionCounts> {
    check(rules)?;
    let now = clock.now_ms();
    db.write(move |tx| {
        let c = counts(tx, rules, now)?;
        if rules.time_days > 0 {
            let at = cutoff(now, rules.time_days);
            tx.execute("DELETE FROM app_sessions WHERE ended_at < ?1", [at])?;
            tx.execute("DELETE FROM domain_sessions WHERE ended_at < ?1", [at])?;
        }
        if rules.tasks_days > 0 {
            tx.execute(
                "UPDATE tasks SET binned_at = ?1 WHERE status = 'COMPLETED' AND binned_at IS NULL AND completed_at < ?2",
                params![now, cutoff(now, rules.tasks_days)],
            )?;
        }
        tx.execute("DELETE FROM tasks WHERE binned_at IS NOT NULL AND binned_at < ?1", [now - BIN_DAYS * DAY_MS])?;
        let events = if c == RetentionCounts::default() { vec![] } else { vec![Event::RetentionApplied { at: now, counts: c }] };
        Ok((c, events))
    })
    .await
}

/// Tasks in the Bin, most recently binned first.
pub async fn binned_tasks(db: &Database) -> Result<Vec<Task>> {
    db.read(|conn| {
        let mut stmt = conn.prepare("SELECT id FROM tasks WHERE binned_at IS NOT NULL ORDER BY binned_at DESC, completed_at DESC")?;
        let ids = stmt.query_map([], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        ids.iter().map(|id| tasks::load(conn, id)).collect()
    })
    .await
}

/// Put a task from the Bin back (still completed).
pub async fn restore_task(db: &Database, clock: &dyn Clock, id: &str) -> Result<Task> {
    let (id, now) = (id.to_owned(), clock.now_ms());
    db.write(move |tx| {
        if tx.execute(
            "UPDATE tasks SET binned_at = NULL WHERE id = ?1 AND binned_at IS NOT NULL",
            [&id],
        )? == 0
        {
            return Err(AppError::not_found("That task isn't in the Bin."));
        }
        let task = tasks::load(tx, &id)?;
        Ok((task.clone(), vec![Event::TaskUpdated { at: now, task }]))
    })
    .await
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use chrono_tz::Tz;

    use super::*;
    use crate::bus::EventBus;
    use crate::clock::FakeClock;
    use crate::error::ErrorCode;
    use crate::tasks::{TaskFilters, TaskInput, TaskStatus, TaskView};

    struct Fixture {
        db: Database,
        clock: FakeClock,
        _dir: tempfile::TempDir,
    }

    fn setup() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("pluto.db"), EventBus::default()).unwrap();
        let clock = FakeClock::at_local(
            Tz::UTC,
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            12,
            0,
            0,
        );
        Fixture {
            db,
            clock,
            _dir: dir,
        }
    }

    async fn task(f: &Fixture, title: &str, done: bool) -> Task {
        let t = tasks::create(
            &f.db,
            &f.clock,
            TaskInput {
                title: title.into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        if done {
            tasks::transition(&f.db, &f.clock, &t.id, TaskStatus::Completed)
                .await
                .unwrap();
        }
        t
    }

    async fn session(f: &Fixture, id: &str, start: i64) {
        let id = id.to_owned();
        f.db.write(move |tx| {
            tx.execute("INSERT INTO app_sessions (id, app, category, is_browser, started_at, ended_at) VALUES (?1, 'Code', 'Coding', 0, ?2, ?3)", params![id, start, start + 1000])?;
            Ok(((), vec![]))
        })
        .await
        .unwrap();
    }

    const NEVER: RetentionRules = RetentionRules {
        time_days: 0,
        tasks_days: 0,
    };

    #[tokio::test]
    async fn never_by_default_touches_nothing() {
        let f = setup();
        task(&f, "old done", true).await;
        session(&f, "s", f.clock.now_ms()).await;
        f.clock.advance_ms(400 * DAY_MS);
        assert_eq!(
            apply(&f.db, &f.clock, NEVER).await.unwrap(),
            RetentionCounts::default()
        );
        assert_eq!(
            tasks::query(&f.db, &f.clock, TaskView::Completed, TaskFilters::default())
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn only_old_time_and_old_completed_tasks_go_and_the_preview_matches() {
        let f = setup();
        let old = task(&f, "old done", true).await;
        task(&f, "old but open", false).await;
        session(&f, "old", f.clock.now_ms()).await;
        f.clock.advance_ms(40 * DAY_MS);
        task(&f, "recent done", true).await;
        session(&f, "new", f.clock.now_ms()).await;

        let rules = RetentionRules {
            time_days: 30,
            tasks_days: 30,
        };
        let seen = preview(&f.db, &f.clock, rules).await.unwrap();
        assert_eq!(
            seen,
            RetentionCounts {
                time_sessions: 1,
                tasks_binned: 1,
                tasks_purged: 0
            }
        );
        assert_eq!(apply(&f.db, &f.clock, rules).await.unwrap(), seen);

        let done: Vec<_> =
            tasks::query(&f.db, &f.clock, TaskView::Completed, TaskFilters::default())
                .await
                .unwrap()
                .into_iter()
                .map(|r| r.task.title)
                .collect();
        assert_eq!(done, ["recent done"]);
        assert_eq!(
            tasks::query(&f.db, &f.clock, TaskView::All, TaskFilters::default())
                .await
                .unwrap()
                .len(),
            2,
            "the open task stays"
        );
        assert_eq!(binned_tasks(&f.db).await.unwrap()[0].id, old.id);
        let left: i64 =
            f.db.read(|c| Ok(c.query_row("SELECT COUNT(*) FROM app_sessions", [], |r| r.get(0))?))
                .await
                .unwrap();
        assert_eq!(left, 1);
        // Running again changes nothing.
        assert_eq!(
            apply(&f.db, &f.clock, rules).await.unwrap(),
            RetentionCounts::default()
        );
    }

    #[tokio::test]
    async fn a_binned_task_can_be_restored_or_is_deleted_after_30_days() {
        let f = setup();
        let a = task(&f, "a", true).await;
        let b = task(&f, "b", true).await;
        f.clock.advance_ms(100 * DAY_MS);
        let rules = RetentionRules {
            time_days: 0,
            tasks_days: 90,
        };
        apply(&f.db, &f.clock, rules).await.unwrap();
        restore_task(&f.db, &f.clock, &a.id).await.unwrap();
        assert_eq!(
            restore_task(&f.db, &f.clock, &a.id).await.unwrap_err().code,
            ErrorCode::NotFound
        );
        f.clock.advance_ms(31 * DAY_MS);
        let c = apply(
            &f.db,
            &f.clock,
            RetentionRules {
                tasks_days: 0,
                ..rules
            },
        )
        .await
        .unwrap();
        assert_eq!(c.tasks_purged, 1);
        assert_eq!(
            tasks::get(&f.db, &b.id).await.unwrap_err().code,
            ErrorCode::NotFound
        );
        assert!(
            tasks::get(&f.db, &a.id).await.is_ok(),
            "the restored one stays"
        );
    }

    #[tokio::test]
    async fn only_the_offered_ages_are_accepted() {
        let f = setup();
        let bad = RetentionRules {
            time_days: 7,
            tasks_days: 0,
        };
        assert_eq!(
            apply(&f.db, &f.clock, bad).await.unwrap_err().code,
            ErrorCode::Validation
        );
    }
}
