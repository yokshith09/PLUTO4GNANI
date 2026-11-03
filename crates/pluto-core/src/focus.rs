//! Focus sessions (FR-05, owner 2026-10-23): a focus or break countdown, saved so it survives a
//! restart and its minutes count later (study hours).
//!
//! Time left is never stored; it is worked out from `started_at`, `planned_ms` and the time spent
//! paused ([`remaining_ms`], pure and tested). One session runs at a time (a unique index). A
//! session whose time is up is finished by [`finish`] (the pet or dashboard calls it at zero) or,
//! after a restart, by [`current`]. Finishing is idempotent, so both windows may call it.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::clock::Clock;
use crate::db::Database;
use crate::error::{AppError, Result};
use crate::events::Event;
use crate::ids::new_id;
use crate::tasks;

pub const MINUTES_MAX: u32 = 240;
pub const EXTEND_MAX: u32 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FocusKind {
    Focus,
    Break,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FocusOutcome {
    /// The time ran out.
    Done,
    /// Stopped early; the minutes worked still count.
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FocusSession {
    pub id: String,
    pub kind: FocusKind,
    pub task_id: Option<String>,
    /// The task's title when it started; empty when no task.
    pub label: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub planned_ms: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub started_at: i64,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub paused_at: Option<i64>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub paused_ms: i64,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub ended_at: Option<i64>,
    pub outcome: Option<FocusOutcome>,
    pub local_date: String,
}

/// What the dashboard sends to start one.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, optional_fields = nullable))]
pub struct FocusStart {
    pub kind: FocusKind,
    pub minutes: u32,
    #[serde(default)]
    pub task_id: Option<String>,
}

/// Time counted so far: from the start to now (or the end, or the pause), minus paused time.
pub fn worked_ms(s: &FocusSession, now: i64) -> i64 {
    let until = s.ended_at.or(s.paused_at).unwrap_or(now);
    (until - s.started_at - s.paused_ms).clamp(0, s.planned_ms)
}

/// Time left; 0 once it has run out.
pub fn remaining_ms(s: &FocusSession, now: i64) -> i64 {
    s.planned_ms - worked_ms(s, now)
}

const COLUMNS: &str = "id, kind, task_id, label, planned_ms, started_at, paused_at, paused_ms, ended_at, outcome, local_date";

fn from_row(r: &rusqlite::Row) -> rusqlite::Result<FocusSession> {
    let kind: String = r.get(1)?;
    let outcome: Option<String> = r.get(9)?;
    Ok(FocusSession {
        id: r.get(0)?,
        kind: if kind == "BREAK" {
            FocusKind::Break
        } else {
            FocusKind::Focus
        },
        task_id: r.get(2)?,
        label: r.get(3)?,
        planned_ms: r.get(4)?,
        started_at: r.get(5)?,
        paused_at: r.get(6)?,
        paused_ms: r.get(7)?,
        ended_at: r.get(8)?,
        outcome: outcome.map(|o| {
            if o == "STOPPED" {
                FocusOutcome::Stopped
            } else {
                FocusOutcome::Done
            }
        }),
        local_date: r.get(10)?,
    })
}

fn running(conn: &Connection) -> Result<Option<FocusSession>> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLUMNS} FROM focus_sessions WHERE ended_at IS NULL"),
            [],
            from_row,
        )
        .optional()?)
}

fn load(conn: &Connection, id: &str) -> Result<FocusSession> {
    conn.query_row(
        &format!("SELECT {COLUMNS} FROM focus_sessions WHERE id = ?1"),
        [id],
        from_row,
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("That focus session no longer exists."))
}

fn nothing_running() -> AppError {
    AppError::invalid_transition("No focus session is running.")
}

/// End `s` as of `at` with `outcome`, and say so.
fn end(
    tx: &Connection,
    s: &FocusSession,
    at: i64,
    outcome: FocusOutcome,
) -> Result<(FocusSession, Vec<Event>)> {
    // A paused session ends where it was paused: the pause isn't counted.
    let (ended_at, paused_ms) = match s.paused_at {
        Some(p) => (at, s.paused_ms + (at - p)),
        None => (at, s.paused_ms),
    };
    let text = if outcome == FocusOutcome::Stopped {
        "STOPPED"
    } else {
        "DONE"
    };
    tx.execute(
        "UPDATE focus_sessions SET ended_at = ?1, paused_ms = ?2, paused_at = NULL, outcome = ?3 WHERE id = ?4",
        params![ended_at, paused_ms, text, s.id],
    )?;
    let session = load(tx, &s.id)?;
    Ok((session.clone(), vec![Event::FocusEnded { at, session }]))
}

/// The moment a running, unpaused session ran out.
fn ran_out_at(s: &FocusSession) -> i64 {
    s.started_at + s.paused_ms + s.planned_ms
}

/// Start a focus session or a break. Refused while another one runs.
pub async fn start(db: &Database, clock: &dyn Clock, input: FocusStart) -> Result<FocusSession> {
    if input.minutes == 0 || input.minutes > MINUTES_MAX {
        return Err(AppError::validation(
            "minutes",
            format!("Pick 1 to {MINUTES_MAX} minutes."),
        ));
    }
    let (id, now) = (new_id(), clock.now_ms());
    let local = clock.local_date(now).to_string();
    db.write(move |tx| {
        if let Some(r) = running(tx)? {
            // One that has already run out doesn't block a new one.
            if r.paused_at.is_none() && remaining_ms(&r, now) == 0 {
                end(tx, &r, ran_out_at(&r), FocusOutcome::Done)?;
            } else {
                return Err(AppError::conflict("A focus session is already running. Stop it first."));
            }
        }
        let label = match &input.task_id {
            Some(t) => tasks::load(tx, t)?.title,
            None => String::new(),
        };
        let kind = if input.kind == FocusKind::Break { "BREAK" } else { "FOCUS" };
        tx.execute(
            "INSERT INTO focus_sessions (id, kind, task_id, label, planned_ms, started_at, local_date) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, kind, input.task_id, label, i64::from(input.minutes) * 60_000, now, local],
        )?;
        let session = load(tx, &id)?;
        Ok((session.clone(), vec![Event::FocusChanged { at: now, session }]))
    })
    .await
}

/// Time worked on a task across its finished focus sessions (paused time excluded).
pub async fn task_total_ms(db: &Database, task_id: &str) -> Result<i64> {
    let task_id = task_id.to_owned();
    db.read(move |conn| {
        Ok(conn.query_row(
            "SELECT COALESCE(SUM(MAX(ended_at - started_at - paused_ms, 0)), 0) FROM focus_sessions
              WHERE task_id = ?1 AND kind = 'FOCUS' AND ended_at IS NOT NULL",
            [&task_id],
            |r| r.get(0),
        )?)
    })
    .await
}

/// The running session, if any. One whose time ran out (say, while Pluto was closed) is finished
/// here, at the moment it ran out.
pub async fn current(db: &Database, clock: &dyn Clock) -> Result<Option<FocusSession>> {
    let now = clock.now_ms();
    db.write(move |tx| {
        let Some(s) = running(tx)? else {
            return Ok((None, vec![]));
        };
        if s.paused_at.is_none() && remaining_ms(&s, now) == 0 {
            let (_, events) = end(tx, &s, ran_out_at(&s), FocusOutcome::Done)?;
            return Ok((None, events));
        }
        Ok((Some(s), vec![]))
    })
    .await
}

/// Pause or resume the running session.
pub async fn set_paused(db: &Database, clock: &dyn Clock, paused: bool) -> Result<FocusSession> {
    let now = clock.now_ms();
    db.write(move |tx| {
        let s = running(tx)?.ok_or_else(nothing_running)?;
        match (paused, s.paused_at) {
            (true, None) => tx.execute("UPDATE focus_sessions SET paused_at = ?1 WHERE id = ?2", params![now, s.id])?,
            (false, Some(p)) => tx.execute(
                "UPDATE focus_sessions SET paused_at = NULL, paused_ms = paused_ms + ?1 WHERE id = ?2",
                params![now - p, s.id],
            )?,
            _ => return Ok((s, vec![])),
        };
        let session = load(tx, &s.id)?;
        Ok((session.clone(), vec![Event::FocusChanged { at: now, session }]))
    })
    .await
}

/// Add minutes to the running session ("+5 min").
pub async fn extend(db: &Database, clock: &dyn Clock, minutes: u32) -> Result<FocusSession> {
    if minutes == 0 || minutes > EXTEND_MAX {
        return Err(AppError::validation(
            "minutes",
            format!("Add 1 to {EXTEND_MAX} minutes."),
        ));
    }
    let now = clock.now_ms();
    db.write(move |tx| {
        let s = running(tx)?.ok_or_else(nothing_running)?;
        let planned =
            (s.planned_ms + i64::from(minutes) * 60_000).min(i64::from(MINUTES_MAX) * 60_000);
        tx.execute(
            "UPDATE focus_sessions SET planned_ms = ?1 WHERE id = ?2",
            params![planned, s.id],
        )?;
        let session = load(tx, &s.id)?;
        Ok((
            session.clone(),
            vec![Event::FocusChanged { at: now, session }],
        ))
    })
    .await
}

/// Stop early. The minutes worked still count.
pub async fn stop(db: &Database, clock: &dyn Clock) -> Result<FocusSession> {
    let now = clock.now_ms();
    db.write(move |tx| {
        let s = running(tx)?.ok_or_else(nothing_running)?;
        end(tx, &s, now, FocusOutcome::Stopped)
    })
    .await
}

/// The time is up: finish session `id`. Calling it again, or for a session already ended, just
/// returns it; calling it early (more than 2 s left) is refused.
pub async fn finish(db: &Database, clock: &dyn Clock, id: &str) -> Result<FocusSession> {
    let (id, now) = (id.to_owned(), clock.now_ms());
    db.write(move |tx| {
        let s = load(tx, &id)?;
        if s.ended_at.is_some() {
            return Ok((s, vec![]));
        }
        if s.paused_at.is_some() || remaining_ms(&s, now) > 2_000 {
            return Err(AppError::invalid_transition(
                "This session still has time left.",
            ));
        }
        end(tx, &s, ran_out_at(&s).min(now), FocusOutcome::Done)
    })
    .await
}

/// Finished focus sessions (not breaks) on a local day, oldest first, for study hours.
pub async fn on_day(db: &Database, day: chrono::NaiveDate) -> Result<Vec<FocusSession>> {
    db.read(move |conn| {
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM focus_sessions WHERE local_date = ?1 AND kind = 'FOCUS' AND ended_at IS NOT NULL ORDER BY started_at"
        ))?;
        let rows = stmt.query_map([day.to_string()], from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    })
    .await
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use chrono_tz::Tz;

    use super::*;
    use crate::bus::{EventBus, Recv, Subscriber};
    use crate::clock::FakeClock;
    use crate::error::ErrorCode;
    use crate::tasks::TaskInput;

    const MIN: i64 = 60_000;

    fn session(planned: i64, started: i64) -> FocusSession {
        FocusSession {
            id: "f".into(),
            kind: FocusKind::Focus,
            task_id: None,
            label: String::new(),
            planned_ms: planned,
            started_at: started,
            paused_at: None,
            paused_ms: 0,
            ended_at: None,
            outcome: None,
            local_date: "2026-10-23".into(),
        }
    }

    #[test]
    fn time_left_leaves_out_pauses_and_stops_at_zero() {
        let mut s = session(25 * MIN, 0);
        assert_eq!(remaining_ms(&s, 10 * MIN), 15 * MIN);
        s.paused_at = Some(10 * MIN);
        assert_eq!(remaining_ms(&s, 50 * MIN), 15 * MIN, "frozen while paused");
        s.paused_at = None;
        s.paused_ms = 40 * MIN;
        assert_eq!(
            remaining_ms(&s, 55 * MIN),
            10 * MIN,
            "the pause doesn't count"
        );
        assert_eq!(remaining_ms(&s, 999 * MIN), 0);
        assert_eq!(
            worked_ms(&s, 999 * MIN),
            25 * MIN,
            "never more than planned"
        );
    }

    struct Fixture {
        db: Database,
        clock: FakeClock,
        bus: EventBus,
        _dir: tempfile::TempDir,
    }

    fn setup() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let bus = EventBus::default();
        let db = Database::open(&dir.path().join("pluto.db"), bus.clone()).unwrap();
        let clock = FakeClock::at_local(
            Tz::Asia__Kolkata,
            NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(),
            10,
            0,
            0,
        );
        Fixture {
            db,
            clock,
            bus,
            _dir: dir,
        }
    }

    fn focus(minutes: u32) -> FocusStart {
        FocusStart {
            kind: FocusKind::Focus,
            minutes,
            task_id: None,
        }
    }

    #[tokio::test]
    async fn a_session_starts_with_its_task_and_only_one_runs_at_a_time() {
        let f = setup();
        let task = tasks::create(
            &f.db,
            &f.clock,
            TaskInput {
                title: "Physics ch. 4".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let s = start(
            &f.db,
            &f.clock,
            FocusStart {
                task_id: Some(task.id.clone()),
                ..focus(25)
            },
        )
        .await
        .unwrap();
        assert_eq!(
            (s.label.as_str(), s.planned_ms, s.local_date.as_str()),
            ("Physics ch. 4", 25 * MIN, "2026-10-23")
        );
        assert_eq!(
            start(&f.db, &f.clock, focus(25)).await.unwrap_err().code,
            ErrorCode::Conflict
        );
        assert_eq!(
            current(&f.db, &f.clock).await.unwrap().map(|c| c.id),
            Some(s.id)
        );
        for bad in [0, MINUTES_MAX + 1] {
            assert_eq!(
                start(&f.db, &f.clock, focus(bad)).await.unwrap_err().code,
                ErrorCode::Validation
            );
        }
        assert_eq!(
            start(
                &f.db,
                &f.clock,
                FocusStart {
                    task_id: Some("nope".into()),
                    ..focus(5)
                }
            )
            .await
            .unwrap_err()
            .code,
            ErrorCode::Conflict,
            "still the running one first"
        );
    }

    #[tokio::test]
    async fn pause_resume_extend_and_finish_on_time() {
        let f = setup();
        let s = start(&f.db, &f.clock, focus(25)).await.unwrap();
        f.clock.advance_ms(10 * MIN);
        let p = set_paused(&f.db, &f.clock, true).await.unwrap();
        f.clock.advance_ms(30 * MIN); // a long pause
        assert_eq!(remaining_ms(&p, f.clock.now_ms()), 15 * MIN);
        let r = set_paused(&f.db, &f.clock, false).await.unwrap();
        assert_eq!(r.paused_ms, 30 * MIN);
        let e = extend(&f.db, &f.clock, 5).await.unwrap();
        assert_eq!(remaining_ms(&e, f.clock.now_ms()), 20 * MIN);
        assert_eq!(
            finish(&f.db, &f.clock, &s.id).await.unwrap_err().code,
            ErrorCode::InvalidTransition,
            "not yet"
        );

        let mut sub = f.bus.subscribe();
        f.clock.advance_ms(20 * MIN);
        let done = finish(&f.db, &f.clock, &s.id).await.unwrap();
        assert_eq!(done.outcome, Some(FocusOutcome::Done));
        assert_eq!(worked_ms(&done, f.clock.now_ms()), 30 * MIN);
        assert!(matches!(next(&mut sub).await, Event::FocusEnded { .. }));
        // Again: same session, no second event.
        assert_eq!(finish(&f.db, &f.clock, &s.id).await.unwrap(), done);
        assert!(current(&f.db, &f.clock).await.unwrap().is_none());
    }

    async fn next(sub: &mut Subscriber) -> Event {
        match tokio::time::timeout(std::time::Duration::from_secs(1), sub.recv())
            .await
            .unwrap()
        {
            Recv::Event(e) => e,
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn a_task_adds_up_the_focus_time_worked_on_it() {
        let f = setup();
        let t = crate::tasks::quick_add(&f.db, &f.clock, "Essay")
            .await
            .unwrap();
        let with_task = |minutes| FocusStart {
            kind: FocusKind::Focus,
            minutes,
            task_id: Some(t.id.clone()),
        };
        assert_eq!(task_total_ms(&f.db, &t.id).await.unwrap(), 0);
        start(&f.db, &f.clock, with_task(25)).await.unwrap();
        f.clock.advance_ms(10 * MIN);
        stop(&f.db, &f.clock).await.unwrap();
        start(&f.db, &f.clock, with_task(25)).await.unwrap();
        f.clock.advance_ms(7 * MIN);
        stop(&f.db, &f.clock).await.unwrap();
        // A running session and a break on the same task are not counted.
        start(&f.db, &f.clock, with_task(25)).await.unwrap();
        assert_eq!(task_total_ms(&f.db, &t.id).await.unwrap(), 17 * MIN);
    }

    #[tokio::test]
    async fn stopping_early_keeps_the_minutes_worked_and_frees_the_slot() {
        let f = setup();
        start(&f.db, &f.clock, focus(50)).await.unwrap();
        f.clock.advance_ms(12 * MIN);
        let s = stop(&f.db, &f.clock).await.unwrap();
        assert_eq!(s.outcome, Some(FocusOutcome::Stopped));
        assert_eq!(worked_ms(&s, f.clock.now_ms()), 12 * MIN);
        assert_eq!(
            stop(&f.db, &f.clock).await.unwrap_err().code,
            ErrorCode::InvalidTransition
        );
        start(
            &f.db,
            &f.clock,
            FocusStart {
                kind: FocusKind::Break,
                minutes: 5,
                task_id: None,
            },
        )
        .await
        .unwrap();
        let day = on_day(&f.db, NaiveDate::from_ymd_opt(2026, 10, 23).unwrap())
            .await
            .unwrap();
        assert_eq!(
            day.len(),
            1,
            "the running break isn't counted; the stopped focus is"
        );
    }

    #[tokio::test]
    async fn after_a_restart_a_session_that_ran_out_is_finished_at_the_right_time() {
        let f = setup();
        let s = start(&f.db, &f.clock, focus(25)).await.unwrap();
        f.clock.advance_ms(3 * 60 * MIN); // Pluto was closed for three hours
        assert!(current(&f.db, &f.clock).await.unwrap().is_none());
        let ended = finish(&f.db, &f.clock, &s.id).await.unwrap();
        assert_eq!(ended.ended_at, Some(s.started_at + 25 * MIN));
        // And a new one can start straight away, even if nothing called current() first.
        start(&f.db, &f.clock, focus(25)).await.unwrap();
        f.clock.advance_ms(30 * MIN);
        start(&f.db, &f.clock, focus(10)).await.unwrap();
    }

    #[tokio::test]
    async fn deleting_the_task_keeps_the_session_and_its_label() {
        let f = setup();
        let task = tasks::create(
            &f.db,
            &f.clock,
            TaskInput {
                title: "Essay".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        start(
            &f.db,
            &f.clock,
            FocusStart {
                task_id: Some(task.id.clone()),
                ..focus(5)
            },
        )
        .await
        .unwrap();
        f.clock.advance_ms(5 * MIN);
        current(&f.db, &f.clock).await.unwrap();
        tasks::transition(&f.db, &f.clock, &task.id, tasks::TaskStatus::Cancelled)
            .await
            .unwrap();
        tasks::delete(&f.db, &f.clock, &task.id).await.unwrap();
        let day = on_day(&f.db, NaiveDate::from_ymd_opt(2026, 10, 23).unwrap())
            .await
            .unwrap();
        assert_eq!(
            (day[0].task_id.as_deref(), day[0].label.as_str()),
            (None, "Essay")
        );
    }
}
