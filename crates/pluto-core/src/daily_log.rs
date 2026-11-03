//! The daily work log (PRD §6.4, R1-40 … R1-46; Schema §3.7).
//!
//! A day's log is rebuilt **from task history** (`task_events`), never from "what the tasks look
//! like now". That makes the live rollover at midnight and the reconstruction of days Pluto missed
//! the same code path, and it is why a task that was completed and then reopened the same day
//! reads correctly. Today's log is computed live and never stored; at rollover it is frozen into
//! `daily_logs`, which the database makes immutable. Titles are copied in, so a frozen log still
//! reads correctly after a task is renamed or deleted.
//!
//! What history does *not* record is approximated from the task's current row: its title,
//! priority and due date (edits to those are not in `task_events`). For a log frozen live at
//! midnight that is exact; for one reconstructed days later it can differ if those were edited
//! in between.

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::NaiveDate;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::bus::{EventBus, Recv};
use crate::clock::{start_of_day_ms, Clock};
use crate::db::Database;
use crate::error::{AppError, Result};
use crate::events::Event;
use crate::tasks::{Priority, TaskStatus};

pub const SNAPSHOT_VERSION: u32 = 1;

/// A log generated more than this long after its day ended was not produced by the live midnight
/// rollover (the machine was asleep or Pluto was closed), so it is marked reconstructed. The live
/// timer fires within milliseconds; an hour absorbs scheduling jitter.
pub const LIVE_ROLLOVER_GRACE_MS: i64 = 60 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct LogEntry {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<Priority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_at_eod: Option<TaskStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(type = "number | null", optional))]
    pub completed_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Stats {
    pub planned_count: u32,
    pub completed_count: u32,
    /// `completed / planned`, to 2 decimals; absent when nothing was planned.
    pub completion_ratio: Option<f64>,
    pub tasks_created: u32,
    pub notes_created: u32,
    /// Notes created on an earlier day and edited on this one (a note both created and edited today counts once, as created).
    pub notes_edited: u32,
    pub meetings: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Snapshot {
    pub date: String,
    pub planned: Vec<LogEntry>,
    pub in_progress: Vec<LogEntry>,
    pub completed: Vec<LogEntry>,
    pub pending: Vec<LogEntry>,
    pub cancelled: Vec<LogEntry>,
    pub overdue: Vec<LogEntry>,
    pub stats: Stats,
    /// Work sessions and app/browser time arrive with Phase 2; until then this stays null (R1-46).
    #[cfg_attr(feature = "ts", ts(type = "unknown | null"))]
    pub activity: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct DailyLog {
    pub date: String,
    /// Today's log: computed now, not yet frozen.
    pub live: bool,
    pub reconstructed: bool,
    pub snapshot: Snapshot,
}

/// The milliseconds `[start, end)` of one local day.
#[derive(Debug, Clone, Copy)]
struct DayBounds {
    start: i64,
    end: i64,
}

fn bounds(clock: &dyn Clock, date: NaiveDate) -> DayBounds {
    DayBounds {
        start: start_of_day_ms(clock, date),
        end: start_of_day_ms(clock, date.succ_opt().unwrap_or(date)),
    }
}

/// What the history says about one task as of the end of the day being built.
#[derive(Default)]
struct Folded {
    seen: bool,
    status: Option<TaskStatus>,
    planned: Option<NaiveDate>,
    created_on_day: bool,
    /// Set when it entered COMPLETED on the day and has not left it since.
    completed_at_on_day: Option<i64>,
    cancelled_on_day: bool,
}

fn parse_status(s: &Option<String>) -> Option<TaskStatus> {
    TaskStatus::ALL
        .into_iter()
        .find(|v| Some(v.as_str()) == s.as_deref())
}

fn parse_day(s: &Option<String>) -> Option<NaiveDate> {
    s.as_deref().and_then(|s| s.parse().ok())
}

fn rank(p: Option<Priority>) -> u8 {
    match p {
        Some(Priority::High) => 0,
        Some(Priority::Medium) => 1,
        Some(Priority::Low) => 2,
        None => 3,
    }
}

fn ordered(mut entries: Vec<LogEntry>) -> Vec<LogEntry> {
    entries.sort_by(|a, b| {
        (rank(a.priority), a.title.to_lowercase(), &a.id).cmp(&(
            rank(b.priority),
            b.title.to_lowercase(),
            &b.id,
        ))
    });
    entries
}

/// Build the snapshot for `date` from history, and whether anything *happened* that day.
///
/// Pure over the database contents: it never looks at the clock, only at `date` and the
/// precomputed day `bounds`. "Happened" means a task event, a note created or edited, or a
/// meeting; carry-over lists (a task still in progress, still overdue) are not activity, or a
/// single forgotten task would make every later day produce a log (R1-43).
fn build(conn: &Connection, date: NaiveDate, bounds: DayBounds) -> Result<(Snapshot, bool)> {
    let day = date.to_string();
    let mut folded: BTreeMap<String, Folded> = BTreeMap::new();

    let mut stmt = conn.prepare(
        "SELECT task_id, kind, from_status, to_status, from_date, to_date, at, local_date
           FROM task_events WHERE local_date <= ?1 ORDER BY id",
    )?;
    let rows = stmt.query_map([&day], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, Option<String>>(4)?,
            r.get::<_, Option<String>>(5)?,
            r.get::<_, i64>(6)?,
            r.get::<_, String>(7)?,
        ))
    })?;
    for row in rows {
        let (task_id, kind, from_status, to_status, from_date, to_date, at, local_date) = row?;
        let on_day = local_date == day;
        let f = folded.entry(task_id).or_default();
        f.seen = true;
        match kind.as_str() {
            "CREATED" => {
                f.status = Some(TaskStatus::Planned);
                f.planned = parse_day(&to_date);
                f.created_on_day = on_day;
            }
            "STATUS" => {
                let (from, to) = (parse_status(&from_status), parse_status(&to_status));
                f.status = to;
                // Leaving COMPLETED (a reopen) undoes the completion; entering it records it.
                if from == Some(TaskStatus::Completed) {
                    f.completed_at_on_day = None;
                }
                if from == Some(TaskStatus::Cancelled) {
                    f.cancelled_on_day = false;
                }
                if on_day && to == Some(TaskStatus::Completed) {
                    f.completed_at_on_day = Some(at);
                }
                if on_day && to == Some(TaskStatus::Cancelled) {
                    f.cancelled_on_day = true;
                }
            }
            // A defer, or an edit that moved the planned date: `to_date` is the new date, absent when cleared.
            "DEFERRED" | "EDITED" if from_date.is_some() || to_date.is_some() => {
                f.planned = parse_day(&to_date)
            }
            _ => {}
        }
    }

    // Titles, priorities and due dates are read from the current row (see the module note).
    let mut current = conn.prepare("SELECT id, title, priority, due_date FROM tasks")?;
    let mut info: BTreeMap<String, (String, Option<Priority>, Option<String>)> = BTreeMap::new();
    for row in current.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
        ))
    })? {
        let (id, title, priority, due) = row?;
        let priority = match priority.as_deref() {
            Some("HIGH") => Some(Priority::High),
            Some("MEDIUM") => Some(Priority::Medium),
            Some("LOW") => Some(Priority::Low),
            _ => None,
        };
        info.insert(id, (title, priority, due));
    }

    let (mut planned, mut in_progress, mut completed, mut pending, mut cancelled, mut overdue) =
        (vec![], vec![], vec![], vec![], vec![], vec![]);
    let mut tasks_created = 0u32;
    for (id, f) in &folded {
        let Some(status) = f.status else { continue };
        let Some((title, priority, due)) = info.get(id) else {
            continue;
        };
        let entry = |status_at_eod: Option<TaskStatus>,
                     completed_at: Option<i64>,
                     due_date: Option<String>| LogEntry {
            id: id.clone(),
            title: title.clone(),
            priority: *priority,
            status_at_eod,
            completed_at,
            due_date,
        };
        if f.created_on_day {
            tasks_created += 1;
        }
        if f.planned == Some(date) {
            planned.push(entry(Some(status), None, None));
        }
        match status {
            TaskStatus::InProgress => in_progress.push(entry(None, None, None)),
            TaskStatus::Pending => pending.push(entry(None, None, None)),
            _ => {}
        }
        if status == TaskStatus::Completed {
            if let Some(at) = f.completed_at_on_day {
                completed.push(entry(None, Some(at), None));
            }
        }
        if status == TaskStatus::Cancelled && f.cancelled_on_day {
            cancelled.push(entry(None, None, None));
        }
        let finished = matches!(status, TaskStatus::Completed | TaskStatus::Cancelled);
        if let Some(due_text) = due {
            if !finished && due_text.parse::<NaiveDate>().is_ok_and(|d| d < date) {
                overdue.push(entry(None, None, Some(due_text.clone())));
            }
        }
    }

    let count = |sql: &str, args: &[&dyn rusqlite::ToSql]| -> Result<u32> {
        Ok(conn.query_row(sql, args, |r| r.get(0))?)
    };
    let notes_created = count(
        "SELECT COUNT(*) FROM notes WHERE deleted_at IS NULL AND created_at >= ?1 AND created_at < ?2",
        params![bounds.start, bounds.end],
    )?;
    let notes_edited = count(
        "SELECT COUNT(*) FROM notes WHERE deleted_at IS NULL AND edited_at >= ?1 AND edited_at < ?2 AND created_at < ?1",
        params![bounds.start, bounds.end],
    )?;
    let meetings = count(
        "SELECT COUNT(*) FROM meetings WHERE starts_at >= ?1 AND starts_at < ?2",
        params![bounds.start, bounds.end],
    )?;

    let task_events_today = count(
        "SELECT COUNT(*) FROM task_events WHERE local_date = ?1",
        params![day],
    )?;
    let had_activity = task_events_today + notes_created + notes_edited + meetings > 0;

    let (planned_count, completed_count) = (planned.len() as u32, completed.len() as u32);
    let completion_ratio = (planned_count > 0)
        .then(|| (f64::from(completed_count) / f64::from(planned_count) * 100.0).round() / 100.0);
    let snapshot = Snapshot {
        date: day,
        planned: ordered(planned),
        in_progress: ordered(in_progress),
        completed: ordered(completed),
        pending: ordered(pending),
        cancelled: ordered(cancelled),
        overdue: ordered(overdue),
        stats: Stats {
            planned_count,
            completed_count,
            completion_ratio,
            tasks_created,
            notes_created,
            notes_edited,
            meetings,
        },
        activity: None,
    };
    Ok((snapshot, had_activity))
}

/// Today's log, computed now from current data (R1-41). Never stored.
pub async fn live(db: &Database, clock: &dyn Clock) -> Result<DailyLog> {
    let date = clock.today();
    let b = bounds(clock, date);
    let (snapshot, _) = db.read(move |conn| build(conn, date, b)).await?;
    Ok(DailyLog {
        date: date.to_string(),
        live: true,
        reconstructed: false,
        snapshot,
    })
}

/// The log for `date`: live if it is today, otherwise the frozen snapshot, or `None` if the day
/// has none (nothing happened, or it has not ended yet).
pub async fn get(db: &Database, clock: &dyn Clock, date: NaiveDate) -> Result<Option<DailyLog>> {
    if date == clock.today() {
        return live(db, clock).await.map(Some);
    }
    let key = date.to_string();
    db.read(move |conn| {
        let row: Option<(String, bool)> = conn
            .query_row(
                "SELECT snapshot, reconstructed FROM daily_logs WHERE log_date = ?1",
                [&key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map(Some)
            .or_else(|e| {
                if matches!(e, rusqlite::Error::QueryReturnedNoRows) {
                    Ok(None)
                } else {
                    Err(e)
                }
            })?;
        row.map(|(json, reconstructed)| {
            let snapshot: Snapshot = serde_json::from_str(&json)
                .map_err(|_| AppError::internal("A saved daily log couldn't be read."))?;
            Ok(DailyLog {
                date: key.clone(),
                live: false,
                reconstructed,
                snapshot,
            })
        })
        .transpose()
    })
    .await
}

/// Freeze the log for a day that has ended. Idempotent: a day already frozen is left exactly as
/// it was, and a day with no tasks planned and no activity gets no log. Returns whether a log was written.
pub async fn freeze(db: &Database, clock: &dyn Clock, date: NaiveDate) -> Result<bool> {
    let now = clock.now_ms();
    let b = bounds(clock, date);
    // Only a day that has fully ended can be frozen; today's log stays live.
    if date >= clock.today() {
        return Err(AppError::validation("date", "That day hasn't ended yet."));
    }
    let reconstructed = now - b.end > LIVE_ROLLOVER_GRACE_MS;
    db.write(move |tx| {
        let (snapshot, had_activity) = build(tx, date, b)?;
        // A day with tasks planned for it is worth a log even if nothing was done ("planned 3, did 0").
        if snapshot.planned.is_empty() && !had_activity {
            return Ok((false, vec![]));
        }
        let json = serde_json::to_string(&snapshot).map_err(|_| AppError::internal("Couldn't save the daily log."))?;
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO daily_logs (log_date, snapshot, snapshot_version, generated_at, reconstructed) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![snapshot.date, json, SNAPSHOT_VERSION, now, reconstructed],
        )?;
        let events = if inserted > 0 { vec![Event::DailyLogFrozen { at: now, log_date: snapshot.date.clone(), reconstructed }] } else { vec![] };
        Ok((inserted > 0, events))
    })
    .await
}

/// Freeze every ended day after the newest frozen log (or since the first recorded activity, if
/// there are none) that does not have one. Cheap to call: it is the startup self-heal and the
/// recovery after a missed rollover event (R1-43). Returns the days it wrote.
pub async fn reconcile(db: &Database, clock: &dyn Clock) -> Result<Vec<NaiveDate>> {
    let yesterday = clock.today().pred_opt().unwrap_or(clock.today());
    let start: Option<NaiveDate> = db
        .read(|conn| {
            let newest: Option<String> =
                conn.query_row("SELECT MAX(log_date) FROM daily_logs", [], |r| r.get(0))?;
            let first = match newest {
                Some(d) => d.parse::<NaiveDate>().ok().and_then(|d| d.succ_opt()),
                None => conn
                    .query_row("SELECT MIN(local_date) FROM task_events", [], |r| {
                        r.get::<_, Option<String>>(0)
                    })?
                    .and_then(|d| d.parse().ok()),
            };
            Ok(first)
        })
        .await?;
    let mut written = Vec::new();
    let Some(mut day) = start else {
        return Ok(written);
    };
    while day <= yesterday {
        if freeze(db, clock, day).await? {
            written.push(day);
        }
        day = day
            .succ_opt()
            .unwrap_or(yesterday.succ_opt().unwrap_or(day));
        if day > yesterday {
            break;
        }
    }
    Ok(written)
}

/// Listen for `DayRolledOver` and freeze the day that ended. On lag, fall back to [`reconcile`].
pub fn spawn_freezer(
    db: Arc<Database>,
    clock: Arc<dyn Clock>,
    bus: &EventBus,
) -> tokio::task::JoinHandle<()> {
    let mut events = bus.subscribe();
    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Recv::Event(Event::DayRolledOver { ended_date, .. }) => {
                    match ended_date.parse::<NaiveDate>() {
                        Ok(date) => {
                            if let Err(error) = freeze(&db, &*clock, date).await {
                                tracing::warn!(code = ?error.code, "couldn't freeze a daily log; it will be rebuilt at the next start");
                            }
                        }
                        Err(_) => {
                            tracing::warn!("a rollover event carried a date Pluto can't read")
                        }
                    }
                }
                Recv::Resync { .. } => {
                    if let Err(error) = reconcile(&db, &*clock).await {
                        tracing::warn!(code = ?error.code, "couldn't catch up daily logs after falling behind");
                    }
                }
                Recv::Closed => break,
                Recv::Event(_) => {}
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use chrono_tz::Tz;

    use super::*;
    use crate::bus::Subscriber;
    use crate::clock::FakeClock;
    use crate::error::ErrorCode;
    use crate::notes::{self, NoteInput, NotePatch};
    use crate::scheduler::{roll_over, LastSeenStore};
    use crate::tasks::{self, TaskInput, TaskStatus::*};

    struct Fixture {
        _dir: tempfile::TempDir,
        bus: EventBus,
        db: Arc<Database>,
        clock: FakeClock,
    }

    fn setup() -> Fixture {
        setup_in(Tz::UTC)
    }

    fn setup_in(tz: Tz) -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let bus = EventBus::default();
        let db = Arc::new(Database::open(&dir.path().join("pluto.db"), bus.clone()).unwrap());
        Fixture {
            _dir: dir,
            bus,
            db,
            clock: FakeClock::new(0, tz),
        }
    }

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    /// Move the fake clock to a UTC wall time.
    fn at_utc(f: &Fixture, y: i32, m: u32, day: u32, h: u32, min: u32) {
        f.clock.set_now_ms(
            chrono::Utc
                .with_ymd_and_hms(y, m, day, h, min, 0)
                .unwrap()
                .timestamp_millis(),
        );
    }

    /// Move the fake clock to a local wall time in the fixture's timezone.
    fn at_local(f: &Fixture, date: NaiveDate, h: u32, min: u32) {
        f.clock
            .set_now_ms(FakeClock::at_local(f.clock.tz(), date, h, min, 0).now_ms());
    }

    async fn plan(
        f: &Fixture,
        title: &str,
        date: NaiveDate,
        priority: Option<Priority>,
    ) -> tasks::Task {
        tasks::create(
            &f.db,
            &f.clock,
            TaskInput {
                title: title.into(),
                planned_date: Some(date),
                priority,
                ..Default::default()
            },
        )
        .await
        .unwrap()
    }

    async fn go(f: &Fixture, id: &str, to: TaskStatus) {
        tasks::transition(&f.db, &f.clock, id, to).await.unwrap();
    }

    fn titles(entries: &[LogEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.title.as_str()).collect()
    }

    async fn frozen(f: &Fixture, date: NaiveDate) -> DailyLog {
        get(&f.db, &f.clock, date)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("no log for {date}"))
    }

    async fn next_frozen(sub: &mut Subscriber) -> (String, bool) {
        loop {
            if let Recv::Event(Event::DailyLogFrozen {
                log_date,
                reconstructed,
                ..
            }) = sub.recv().await
            {
                return (log_date, reconstructed);
            }
        }
    }

    // ---- the ordinary day ------------------------------------------------------------------

    #[tokio::test]
    async fn a_normal_day_is_frozen_at_midnight_with_its_lists_and_stats() {
        let f = setup();
        let mut sub = f.bus.subscribe();
        let day = d(2026, 10, 1);
        at_utc(&f, 2026, 10, 1, 9, 0);
        let a = plan(&f, "Write PRD", day, Some(Priority::High)).await;
        let b = plan(&f, "Fix schema", day, None).await;
        plan(&f, "Tomorrow's job", d(2026, 10, 2), None).await;
        notes::create(
            &f.db,
            &f.clock,
            NoteInput {
                title: "ideas".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        at_utc(&f, 2026, 10, 1, 10, 0);
        go(&f, &a.id, InProgress).await;
        at_utc(&f, 2026, 10, 1, 15, 0);
        go(&f, &a.id, Completed).await;
        go(&f, &b.id, InProgress).await;
        f.db.write(|tx| {
            tx.execute("INSERT INTO meetings (id, title, starts_at, created_at, updated_at) VALUES ('m', 'sync', ?1, 0, 0)", [chrono::Utc.with_ymd_and_hms(2026, 10, 1, 16, 0, 0).unwrap().timestamp_millis()])?;
            Ok(((), vec![]))
        })
        .await
        .unwrap();

        at_utc(&f, 2026, 10, 2, 0, 0); // the live rollover fires at the stroke of midnight
        f.clock.advance_ms(5_000);
        assert!(freeze(&f.db, &f.clock, day).await.unwrap());

        let log = frozen(&f, day).await;
        assert!(!log.live && !log.reconstructed);
        let s = &log.snapshot;
        assert_eq!(
            titles(&s.planned),
            ["Write PRD", "Fix schema"],
            "tomorrow's job is not planned for this day"
        );
        assert_eq!(s.planned[0].status_at_eod, Some(Completed));
        assert_eq!(s.planned[1].status_at_eod, Some(InProgress));
        assert_eq!(titles(&s.completed), ["Write PRD"]);
        assert_eq!(
            s.completed[0].completed_at,
            Some(
                chrono::Utc
                    .with_ymd_and_hms(2026, 10, 1, 15, 0, 0)
                    .unwrap()
                    .timestamp_millis()
            )
        );
        assert_eq!(titles(&s.in_progress), ["Fix schema"]);
        assert_eq!(
            (
                s.stats.planned_count,
                s.stats.completed_count,
                s.stats.completion_ratio
            ),
            (2, 1, Some(0.5))
        );
        assert_eq!(
            (
                s.stats.tasks_created,
                s.stats.notes_created,
                s.stats.notes_edited,
                s.stats.meetings
            ),
            (3, 1, 0, 1)
        );
        assert_eq!(s.activity, None, "work-session data arrives in Phase 2");
        assert_eq!(
            next_frozen(&mut sub).await,
            ("2026-10-01".to_owned(), false)
        );
    }

    #[tokio::test]
    async fn the_snapshot_json_matches_the_documented_shape() {
        let f = setup();
        let day = d(2026, 10, 1);
        at_utc(&f, 2026, 10, 1, 9, 0);
        let t = plan(&f, "x", day, Some(Priority::High)).await;
        go(&f, &t.id, Completed).await;
        at_utc(&f, 2026, 10, 2, 0, 1);
        freeze(&f.db, &f.clock, day).await.unwrap();
        let json: serde_json::Value = serde_json::from_str(
            &f.db
                .read(|c| {
                    Ok(c.query_row("SELECT snapshot FROM daily_logs", [], |r| {
                        r.get::<_, String>(0)
                    })?)
                })
                .await
                .unwrap(),
        )
        .unwrap();
        for key in [
            "date",
            "planned",
            "in_progress",
            "completed",
            "pending",
            "cancelled",
            "overdue",
            "stats",
            "activity",
        ] {
            assert!(json.get(key).is_some(), "missing {key}");
        }
        for key in [
            "planned_count",
            "completed_count",
            "completion_ratio",
            "tasks_created",
            "notes_created",
            "notes_edited",
            "meetings",
        ] {
            assert!(json["stats"].get(key).is_some(), "missing stats.{key}");
        }
        assert_eq!(json["planned"][0]["status_at_eod"], "COMPLETED");
        assert_eq!(json["activity"], serde_json::Value::Null);
    }

    #[tokio::test]
    async fn today_is_live_and_a_day_that_has_not_ended_cannot_be_frozen() {
        let f = setup();
        at_utc(&f, 2026, 10, 3, 9, 0);
        plan(&f, "now", d(2026, 10, 3), None).await;
        let log = get(&f.db, &f.clock, d(2026, 10, 3)).await.unwrap().unwrap();
        assert!(log.live);
        assert_eq!(titles(&log.snapshot.planned), ["now"]);
        let err = freeze(&f.db, &f.clock, d(2026, 10, 3)).await.unwrap_err();
        assert_eq!(
            (err.code, err.field.as_deref()),
            (ErrorCode::Validation, Some("date"))
        );
        assert!(
            get(&f.db, &f.clock, d(2026, 10, 9))
                .await
                .unwrap()
                .is_none(),
            "the future has no log"
        );
    }

    // ---- built from history, not from what tasks look like now -------------------------------

    #[tokio::test]
    async fn completed_then_reopened_the_same_day_is_not_in_that_days_completed_list() {
        let f = setup();
        let day = d(2026, 10, 1);
        at_utc(&f, 2026, 10, 1, 9, 0);
        let t = plan(&f, "flip-flop", day, None).await;
        go(&f, &t.id, Completed).await;
        at_utc(&f, 2026, 10, 1, 18, 0);
        go(&f, &t.id, Planned).await;
        at_utc(&f, 2026, 10, 2, 0, 1);
        freeze(&f.db, &f.clock, day).await.unwrap();
        let s = frozen(&f, day).await.snapshot;
        assert!(s.completed.is_empty());
        assert_eq!(s.planned[0].status_at_eod, Some(Planned));
        assert_eq!(
            (s.stats.completed_count, s.stats.completion_ratio),
            (0, Some(0.0))
        );
    }

    #[tokio::test]
    async fn a_task_reopened_the_next_day_still_counts_as_completed_on_its_day() {
        let f = setup();
        let day = d(2026, 10, 1);
        at_utc(&f, 2026, 10, 1, 9, 0);
        let t = plan(&f, "finished", day, None).await;
        go(&f, &t.id, Completed).await;
        at_utc(&f, 2026, 10, 2, 8, 0);
        go(&f, &t.id, Planned).await; // reopened before anyone froze the 1st
        at_utc(&f, 2026, 10, 2, 20, 0);
        freeze(&f.db, &f.clock, day).await.unwrap();
        let s = frozen(&f, day).await.snapshot;
        assert_eq!(
            titles(&s.completed),
            ["finished"],
            "the day is judged by what had happened by its end"
        );
    }

    #[tokio::test]
    async fn a_task_deferred_away_during_the_day_is_not_planned_for_it() {
        let f = setup();
        at_utc(&f, 2026, 10, 1, 9, 0);
        let t = plan(&f, "slips", d(2026, 10, 1), None).await;
        at_utc(&f, 2026, 10, 1, 17, 0);
        tasks::defer(&f.db, &f.clock, &t.id, d(2026, 10, 3))
            .await
            .unwrap();
        at_utc(&f, 2026, 10, 2, 0, 1);
        freeze(&f.db, &f.clock, d(2026, 10, 1)).await.unwrap();
        let first = frozen(&f, d(2026, 10, 1)).await.snapshot;
        assert!(
            first.planned.is_empty(),
            "at the end of the 1st it was planned for the 3rd"
        );
        assert_eq!(
            first.stats.tasks_created, 1,
            "the day is still not empty: a task was created"
        );
        at_utc(&f, 2026, 10, 3, 10, 0);
        assert_eq!(
            titles(
                &get(&f.db, &f.clock, d(2026, 10, 3))
                    .await
                    .unwrap()
                    .unwrap()
                    .snapshot
                    .planned
            ),
            ["slips"]
        );
    }

    #[tokio::test]
    async fn open_work_at_the_end_of_the_day_is_listed_by_status_and_overdue_work_by_due_date() {
        let f = setup();
        at_utc(&f, 2026, 10, 1, 9, 0);
        let running = plan(&f, "running", d(2026, 10, 1), None).await;
        let paused = plan(&f, "paused", d(2026, 10, 1), None).await;
        let late = tasks::create(
            &f.db,
            &f.clock,
            TaskInput {
                title: "late".into(),
                due_date: Some(d(2026, 9, 28)),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let late_but_done = tasks::create(
            &f.db,
            &f.clock,
            TaskInput {
                title: "late but done".into(),
                due_date: Some(d(2026, 9, 28)),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let due_today = tasks::create(
            &f.db,
            &f.clock,
            TaskInput {
                title: "due today".into(),
                due_date: Some(d(2026, 10, 1)),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        go(&f, &running.id, InProgress).await;
        go(&f, &paused.id, InProgress).await;
        go(&f, &paused.id, Pending).await;
        go(&f, &late_but_done.id, Completed).await;
        let _ = (late, due_today);
        at_utc(&f, 2026, 10, 2, 0, 1);
        freeze(&f.db, &f.clock, d(2026, 10, 1)).await.unwrap();
        let s = frozen(&f, d(2026, 10, 1)).await.snapshot;
        assert_eq!(titles(&s.in_progress), ["running"]);
        assert_eq!(titles(&s.pending), ["paused"]);
        assert_eq!(
            titles(&s.overdue),
            ["late"],
            "finished and due-today tasks are not overdue"
        );
        assert_eq!(s.overdue[0].due_date.as_deref(), Some("2026-09-28"));
    }

    #[tokio::test]
    async fn cancelled_appears_only_on_the_day_it_was_cancelled() {
        let f = setup();
        at_utc(&f, 2026, 10, 1, 9, 0);
        let t = plan(&f, "dropped", d(2026, 10, 1), None).await;
        go(&f, &t.id, Cancelled).await;
        at_utc(&f, 2026, 10, 2, 9, 0);
        plan(&f, "something else", d(2026, 10, 2), None).await;
        at_utc(&f, 2026, 10, 3, 0, 1);
        freeze(&f.db, &f.clock, d(2026, 10, 1)).await.unwrap();
        freeze(&f.db, &f.clock, d(2026, 10, 2)).await.unwrap();
        assert_eq!(
            titles(&frozen(&f, d(2026, 10, 1)).await.snapshot.cancelled),
            ["dropped"]
        );
        assert!(frozen(&f, d(2026, 10, 2))
            .await
            .snapshot
            .cancelled
            .is_empty());
    }

    // ---- frozen means frozen -----------------------------------------------------------------

    #[tokio::test]
    async fn freezing_twice_keeps_the_first_log_and_announces_it_once() {
        let f = setup();
        let mut sub = f.bus.subscribe();
        at_utc(&f, 2026, 10, 1, 9, 0);
        let t = plan(&f, "original title", d(2026, 10, 1), None).await;
        at_utc(&f, 2026, 10, 2, 0, 1);
        assert!(freeze(&f.db, &f.clock, d(2026, 10, 1)).await.unwrap());
        tasks::update(
            &f.db,
            &f.clock,
            &t.id,
            tasks::TaskPatch {
                title: Some("renamed later".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(
            !freeze(&f.db, &f.clock, d(2026, 10, 1)).await.unwrap(),
            "already frozen: nothing written"
        );
        assert_eq!(
            titles(&frozen(&f, d(2026, 10, 1)).await.snapshot.planned),
            ["original title"]
        );
        assert_eq!(next_frozen(&mut sub).await.0, "2026-10-01");
        f.bus.publish(Event::AppShuttingDown { at: 0 });
        loop {
            match sub.recv().await {
                Recv::Event(Event::DailyLogFrozen { .. }) => panic!("announced twice"),
                Recv::Event(Event::AppShuttingDown { .. }) => break,
                _ => {}
            }
        }
    }

    #[tokio::test]
    async fn a_frozen_log_survives_the_task_being_renamed_and_deleted() {
        let f = setup();
        at_utc(&f, 2026, 10, 1, 9, 0);
        let t = plan(&f, "Submit report", d(2026, 10, 1), None).await;
        go(&f, &t.id, Completed).await;
        at_utc(&f, 2026, 10, 2, 0, 1);
        freeze(&f.db, &f.clock, d(2026, 10, 1)).await.unwrap();
        tasks::update(
            &f.db,
            &f.clock,
            &t.id,
            tasks::TaskPatch {
                title: Some("something else".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        tasks::delete(&f.db, &f.clock, &t.id).await.unwrap();
        let s = frozen(&f, d(2026, 10, 1)).await.snapshot;
        assert_eq!(
            (titles(&s.planned), titles(&s.completed)),
            (vec!["Submit report"], vec!["Submit report"])
        );
    }

    #[tokio::test]
    async fn the_database_itself_refuses_to_change_or_remove_a_frozen_log() {
        let f = setup();
        at_utc(&f, 2026, 10, 1, 9, 0);
        plan(&f, "x", d(2026, 10, 1), None).await;
        at_utc(&f, 2026, 10, 2, 0, 1);
        freeze(&f.db, &f.clock, d(2026, 10, 1)).await.unwrap();
        for sql in [
            "UPDATE daily_logs SET snapshot = '{}'",
            "DELETE FROM daily_logs",
        ] {
            let result =
                f.db.write(move |tx| Ok((tx.execute(sql, [])?, vec![])))
                    .await;
            assert_eq!(result.unwrap_err().code, ErrorCode::Db, "{sql}");
        }
        assert!(get(&f.db, &f.clock, d(2026, 10, 1))
            .await
            .unwrap()
            .is_some());
    }

    // ---- which days get a log ----------------------------------------------------------------

    #[tokio::test]
    async fn a_quiet_day_gets_no_log_even_while_an_old_task_stays_overdue_or_in_progress() {
        let f = setup();
        at_utc(&f, 2026, 10, 1, 9, 0);
        let t = tasks::create(
            &f.db,
            &f.clock,
            TaskInput {
                title: "ancient".into(),
                due_date: Some(d(2026, 9, 1)),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        go(&f, &t.id, InProgress).await;
        at_utc(&f, 2026, 10, 5, 12, 0);
        assert!(
            !freeze(&f.db, &f.clock, d(2026, 10, 3)).await.unwrap(),
            "carry-over lists are not activity"
        );
        assert!(get(&f.db, &f.clock, d(2026, 10, 3))
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn a_day_with_tasks_planned_for_it_gets_a_log_even_if_nothing_was_done() {
        let f = setup();
        at_utc(&f, 2026, 9, 28, 9, 0);
        plan(&f, "planned long ago for the 2nd", d(2026, 10, 2), None).await;
        at_utc(&f, 2026, 10, 3, 6, 0);
        assert!(freeze(&f.db, &f.clock, d(2026, 10, 2)).await.unwrap());
        let s = frozen(&f, d(2026, 10, 2)).await.snapshot;
        assert_eq!(
            (
                s.stats.planned_count,
                s.stats.completed_count,
                s.stats.completion_ratio
            ),
            (1, 0, Some(0.0))
        );
    }

    #[tokio::test]
    async fn the_ratio_is_rounded_to_two_places_and_absent_when_nothing_was_planned() {
        let f = setup();
        at_utc(&f, 2026, 10, 1, 9, 0);
        for title in ["a", "b", "c"] {
            let t = plan(&f, title, d(2026, 10, 1), None).await;
            if title != "c" {
                go(&f, &t.id, Completed).await;
            }
        }
        let unplanned = tasks::create(
            &f.db,
            &f.clock,
            TaskInput {
                title: "ad hoc".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        go(&f, &unplanned.id, Completed).await;
        at_utc(&f, 2026, 10, 2, 0, 1);
        freeze(&f.db, &f.clock, d(2026, 10, 1)).await.unwrap();
        let s = frozen(&f, d(2026, 10, 1)).await.snapshot;
        assert_eq!(
            (s.stats.planned_count, s.stats.completed_count),
            (3, 3),
            "an unplanned task finished that day still counts as completed"
        );
        assert_eq!(s.stats.completion_ratio, Some(1.0));
        // A day with a completion but nothing planned: no ratio.
        at_utc(&f, 2026, 10, 2, 9, 0);
        let solo = tasks::create(
            &f.db,
            &f.clock,
            TaskInput {
                title: "solo".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        go(&f, &solo.id, Completed).await;
        at_utc(&f, 2026, 10, 3, 0, 1);
        freeze(&f.db, &f.clock, d(2026, 10, 2)).await.unwrap();
        assert_eq!(
            frozen(&f, d(2026, 10, 2))
                .await
                .snapshot
                .stats
                .completion_ratio,
            None
        );
    }

    #[tokio::test]
    async fn entries_are_ordered_by_priority_then_title() {
        let f = setup();
        at_utc(&f, 2026, 10, 1, 9, 0);
        plan(&f, "banana", d(2026, 10, 1), None).await;
        plan(&f, "Apple", d(2026, 10, 1), Some(Priority::Low)).await;
        plan(&f, "cherry", d(2026, 10, 1), Some(Priority::High)).await;
        plan(&f, "avocado", d(2026, 10, 1), Some(Priority::Low)).await;
        at_utc(&f, 2026, 10, 2, 0, 1);
        freeze(&f.db, &f.clock, d(2026, 10, 1)).await.unwrap();
        assert_eq!(
            titles(&frozen(&f, d(2026, 10, 1)).await.snapshot.planned),
            ["cherry", "Apple", "avocado", "banana"]
        );
    }

    #[tokio::test]
    async fn notes_created_and_edited_are_counted_without_double_counting() {
        let f = setup();
        at_utc(&f, 2026, 10, 1, 9, 0);
        let old = notes::create(
            &f.db,
            &f.clock,
            NoteInput {
                title: "old".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        at_utc(&f, 2026, 10, 2, 9, 0);
        notes::update(
            &f.db,
            &f.clock,
            &old.id,
            NotePatch {
                body: Some("revised".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let fresh = notes::create(
            &f.db,
            &f.clock,
            NoteInput {
                title: "fresh".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        at_utc(&f, 2026, 10, 2, 10, 0);
        notes::update(
            &f.db,
            &f.clock,
            &fresh.id,
            NotePatch {
                body: Some("polished".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        at_utc(&f, 2026, 10, 3, 0, 1);
        freeze(&f.db, &f.clock, d(2026, 10, 2)).await.unwrap();
        let st = frozen(&f, d(2026, 10, 2)).await.snapshot.stats;
        assert_eq!(
            (st.notes_created, st.notes_edited),
            (1, 1),
            "old: edited; fresh: created (and edited the same day, counted once)"
        );
    }

    // ---- the awkward calendar ---------------------------------------------------------------

    #[tokio::test]
    async fn dst_days_assign_late_evening_work_to_the_right_local_day() {
        // London falls back on 2026-10-25 (25-hour day) and springs forward on 2026-03-29 (23-hour day).
        for (day, evening_utc, after_midnight_utc) in [
            (
                d(2026, 10, 25),
                (2026, 10, 25, 23, 30),
                (2026, 10, 26, 0, 10),
            ),
            (d(2026, 3, 29), (2026, 3, 29, 22, 30), (2026, 3, 29, 23, 30)),
        ] {
            let f = setup_in(chrono_tz::Europe::London);
            let at = |t: (i32, u32, u32, u32, u32)| at_utc(&f, t.0, t.1, t.2, t.3, t.4);
            at(evening_utc);
            let evening = plan(&f, "before midnight", day, None).await;
            go(&f, &evening.id, Completed).await;
            at(after_midnight_utc);
            let late = plan(&f, "after midnight", day.succ_opt().unwrap(), None).await;
            go(&f, &late.id, Completed).await;
            at_local(&f, day.succ_opt().unwrap().succ_opt().unwrap(), 9, 0);
            freeze(&f.db, &f.clock, day).await.unwrap();
            assert_eq!(
                titles(&frozen(&f, day).await.snapshot.completed),
                ["before midnight"],
                "{day}"
            );
        }
    }

    #[tokio::test]
    async fn changing_timezone_does_not_move_history_to_another_day() {
        let f = setup_in(chrono_tz::America::Los_Angeles);
        at_utc(&f, 2026, 10, 4, 6, 0); // 23:00 on the 3rd in Los Angeles
        let t = plan(&f, "late in LA", d(2026, 10, 3), None).await;
        go(&f, &t.id, Completed).await;
        f.clock.set_tz(chrono_tz::Asia::Tokyo); // flew to Tokyo; the 3rd has ended there too
        at_utc(&f, 2026, 10, 5, 3, 0);
        freeze(&f.db, &f.clock, d(2026, 10, 3)).await.unwrap();
        assert_eq!(
            titles(&frozen(&f, d(2026, 10, 3)).await.snapshot.completed),
            ["late in LA"],
            "the recorded local day wins over today's timezone"
        );
    }

    // ---- days Pluto missed --------------------------------------------------------------------

    #[tokio::test]
    async fn sleeping_through_midnight_marks_the_log_reconstructed_but_with_identical_content() {
        let live = setup();
        let late = setup();
        for f in [&live, &late] {
            at_utc(f, 2026, 10, 1, 9, 0);
            let t = plan(f, "same work", d(2026, 10, 1), None).await;
            go(f, &t.id, Completed).await;
        }
        at_utc(&live, 2026, 10, 2, 0, 0);
        live.clock.advance_ms(2_000);
        at_utc(&late, 2026, 10, 2, 7, 30); // the laptop woke at 07:30
        freeze(&live.db, &live.clock, d(2026, 10, 1)).await.unwrap();
        freeze(&late.db, &late.clock, d(2026, 10, 1)).await.unwrap();
        let (a, b) = (
            frozen(&live, d(2026, 10, 1)).await,
            frozen(&late, d(2026, 10, 1)).await,
        );
        assert_eq!((a.reconstructed, b.reconstructed), (false, true));
        // Task ids are random per fixture, so compare everything except them.
        fn without_ids(v: &mut serde_json::Value) {
            match v {
                serde_json::Value::Object(map) => {
                    map.remove("id");
                    map.remove("task_id");
                    map.values_mut().for_each(without_ids);
                }
                serde_json::Value::Array(items) => items.iter_mut().for_each(without_ids),
                _ => {}
            }
        }
        let (mut ja, mut jb) = (
            serde_json::to_value(&a.snapshot).unwrap(),
            serde_json::to_value(&b.snapshot).unwrap(),
        );
        without_ids(&mut ja);
        without_ids(&mut jb);
        assert_eq!(
            ja, jb,
            "reconstruction is the same code path, so the same answer"
        );
    }

    #[tokio::test]
    async fn after_three_days_closed_reconcile_writes_only_the_days_that_had_something() {
        let f = setup();
        at_utc(&f, 2026, 10, 1, 9, 0);
        let t = plan(&f, "day one", d(2026, 10, 1), None).await;
        go(&f, &t.id, Completed).await;
        at_utc(&f, 2026, 10, 3, 9, 0);
        plan(&f, "day three", d(2026, 10, 3), None).await;
        at_utc(&f, 2026, 10, 4, 8, 0); // Pluto opens on the morning of the 4th
        assert_eq!(
            reconcile(&f.db, &f.clock).await.unwrap(),
            vec![d(2026, 10, 1), d(2026, 10, 3)],
            "the 2nd was quiet"
        );
        assert!(frozen(&f, d(2026, 10, 1)).await.reconstructed);
        assert!(get(&f.db, &f.clock, d(2026, 10, 2))
            .await
            .unwrap()
            .is_none());
        assert!(
            reconcile(&f.db, &f.clock).await.unwrap().is_empty(),
            "second run finds nothing to do"
        );
    }

    #[tokio::test]
    async fn reconcile_with_no_history_does_nothing() {
        let f = setup();
        at_utc(&f, 2026, 10, 4, 8, 0);
        assert!(reconcile(&f.db, &f.clock).await.unwrap().is_empty());
    }

    // ---- wired to the rollover ---------------------------------------------------------------

    #[derive(Clone, Default)]
    struct Remember(Arc<std::sync::Mutex<Option<NaiveDate>>>);
    impl LastSeenStore for Remember {
        async fn get(&self) -> Result<Option<NaiveDate>> {
            Ok(*self.0.lock().unwrap())
        }
        async fn set(&self, date: NaiveDate, _at: i64) -> Result<()> {
            *self.0.lock().unwrap() = Some(date);
            Ok(())
        }
    }

    #[tokio::test]
    async fn the_freezer_turns_rollover_events_into_frozen_logs() {
        let f = setup();
        let clock: Arc<FakeClock> = Arc::new(f.clock.clone());
        let freezer = spawn_freezer(Arc::clone(&f.db), clock, &f.bus);
        let mut sub = f.bus.subscribe();
        at_utc(&f, 2026, 10, 1, 9, 0);
        plan(&f, "worked on the 1st", d(2026, 10, 1), None).await;
        at_utc(&f, 2026, 10, 2, 0, 0);
        f.bus.publish(Event::DayRolledOver {
            at: f.clock.now_ms(),
            ended_date: "2026-10-01".into(),
            new_date: "2026-10-02".into(),
        });
        assert_eq!(
            next_frozen(&mut sub).await,
            ("2026-10-01".to_owned(), false)
        );
        assert!(get(&f.db, &f.clock, d(2026, 10, 1))
            .await
            .unwrap()
            .is_some());
        freezer.abort();
    }

    #[tokio::test]
    async fn closed_for_days_the_scheduler_and_freezer_together_produce_every_missing_log() {
        let f = setup();
        let clock: Arc<FakeClock> = Arc::new(f.clock.clone());
        let freezer = spawn_freezer(Arc::clone(&f.db), clock, &f.bus);
        let mut sub = f.bus.subscribe();
        at_utc(&f, 2026, 10, 1, 9, 0);
        plan(&f, "first", d(2026, 10, 1), None).await;
        at_utc(&f, 2026, 10, 3, 9, 0);
        plan(&f, "third", d(2026, 10, 3), None).await;
        at_utc(&f, 2026, 10, 4, 8, 0);
        let store = Remember(Arc::new(std::sync::Mutex::new(Some(d(2026, 10, 1)))));
        roll_over(&f.clock, &store, &f.bus).await.unwrap();
        let mut seen = vec![next_frozen(&mut sub).await.0, next_frozen(&mut sub).await.0];
        seen.sort();
        assert_eq!(seen, ["2026-10-01", "2026-10-03"]);
        freezer.abort();
    }
}
