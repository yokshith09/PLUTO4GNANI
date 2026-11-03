//! Reminders: a title, a time, and an optional link to a note. When the time comes the scheduler
//! publishes `ReminderDue` exactly once; the shell turns that into an OS notification.
//!
//! Every mutation runs through [`Database::write`], so its event is published only after the
//! commit. Timestamps come from the injected [`Clock`].

use std::sync::Arc;
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::bus::{EventBus, Recv, Subscriber};
use crate::clock::Clock;
use crate::db::Database;
use crate::error::{AppError, Result};
use crate::events::Event;
use crate::ids::new_id;

pub const TITLE_MAX: usize = 200;

/// The longest the scheduler sleeps in one go. A reminder further away than this is reached in
/// several hops (one wake-up a day at most), which also re-anchors the timer if the machine
/// slept. This is not polling: nothing runs in between.
const MAX_SLEEP: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Reminder {
    pub id: String,
    pub title: String,
    /// When to remind, ms since the Unix epoch (UTC).
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub remind_at: i64,
    pub note_id: Option<String>,
    /// Set once `ReminderDue` has been published for the current `remind_at`.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub fired_at: Option<i64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub done_at: Option<i64>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
    /// Starred reminders come first (FR-107).
    pub starred: bool,
    /// The task this reminder is for, if any (owner 2026-10-25).
    pub task_id: Option<String>,
    /// How many minutes before the moment it was set for: `remind_at` is already that early.
    pub lead_min: u32,
    /// Play a sound when it is due.
    pub sound: bool,
}

#[cfg(test)]
impl Reminder {
    pub(crate) fn sample() -> Self {
        Self {
            id: "r1".into(),
            title: "Call the dentist".into(),
            remind_at: 1_000,
            note_id: None,
            fired_at: None,
            done_at: None,
            created_at: 1,
            starred: false,
            task_id: None,
            lead_min: 0,
            sound: true,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, optional_fields = nullable))]
#[serde(default)]
pub struct ReminderInput {
    pub title: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub remind_at: i64,
    pub note_id: Option<String>,
    pub task_id: Option<String>,
    /// Defaults to 0.
    pub lead_min: Option<u32>,
    /// Defaults to on.
    pub sound: Option<bool>,
}

/// Fields left as `None` are not touched. Changing `remind_at` makes the reminder fire again.
#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, optional_fields = nullable))]
#[serde(default)]
pub struct ReminderPatch {
    pub title: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub remind_at: Option<i64>,
    pub lead_min: Option<u32>,
    pub sound: Option<bool>,
}

fn clean_title(title: &str) -> Result<String> {
    let title = title.trim();
    match title.chars().count() {
        0 => Err(AppError::validation("title", "Give the reminder a title.")),
        n if n > TITLE_MAX => Err(AppError::validation(
            "title",
            format!("Reminder titles can be up to {TITLE_MAX} characters."),
        )),
        _ => Ok(title.to_owned()),
    }
}

fn check_time(remind_at: i64) -> Result<()> {
    if remind_at <= 0 {
        return Err(AppError::validation(
            "remind_at",
            "Pick a date and time for the reminder.",
        ));
    }
    Ok(())
}

/// The longest "minutes before" offered: one day.
pub const LEAD_MAX: u32 = 1440;

fn check_lead(lead_min: u32) -> Result<()> {
    if lead_min > LEAD_MAX {
        return Err(AppError::validation(
            "lead_min",
            "Remind at most a day before.",
        ));
    }
    Ok(())
}

fn not_found() -> AppError {
    AppError::not_found("That reminder no longer exists.")
}

const COLUMNS: &str =
    "id, title, remind_at, note_id, fired_at, done_at, created_at, starred, task_id, lead_min, sound";

fn from_row(r: &Row) -> rusqlite::Result<Reminder> {
    Ok(Reminder {
        id: r.get(0)?,
        title: r.get(1)?,
        remind_at: r.get(2)?,
        note_id: r.get(3)?,
        fired_at: r.get(4)?,
        done_at: r.get(5)?,
        created_at: r.get(6)?,
        starred: r.get(7)?,
        task_id: r.get(8)?,
        lead_min: r.get(9)?,
        sound: r.get(10)?,
    })
}

fn load(conn: &Connection, id: &str) -> Result<Reminder> {
    conn.query_row(
        &format!("SELECT {COLUMNS} FROM reminders WHERE id = ?1"),
        [id],
        from_row,
    )
    .optional()?
    .ok_or_else(not_found)
}

pub async fn create(db: &Database, clock: &dyn Clock, input: ReminderInput) -> Result<Reminder> {
    let title = clean_title(&input.title)?;
    check_time(input.remind_at)?;
    let lead_min = input.lead_min.unwrap_or(0);
    check_lead(lead_min)?;
    let (id, now) = (new_id(), clock.now_ms());
    db.write(move |tx| {
        if let Some(note_id) = &input.note_id {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM notes WHERE id = ?1)",
                [note_id],
                |r| r.get(0),
            )?;
            if !exists {
                return Err(
                    AppError::not_found("That note no longer exists.").with_field("note_id")
                );
            }
        }
        if let Some(task_id) = &input.task_id {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM tasks WHERE id = ?1)",
                [task_id],
                |r| r.get(0),
            )?;
            if !exists {
                return Err(
                    AppError::not_found("That task no longer exists.").with_field("task_id")
                );
            }
        }
        tx.execute(
            "INSERT INTO reminders (id, title, remind_at, note_id, created_at, task_id, lead_min, sound)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                title,
                input.remind_at,
                input.note_id,
                now,
                input.task_id,
                lead_min,
                input.sound.unwrap_or(true)
            ],
        )?;
        let reminder = load(tx, &id)?;
        Ok((
            reminder.clone(),
            vec![Event::ReminderCreated { at: now, reminder }],
        ))
    })
    .await
}

/// Apply a patch. A change that changes nothing writes nothing and says nothing.
pub async fn update(
    db: &Database,
    clock: &dyn Clock,
    id: &str,
    patch: ReminderPatch,
) -> Result<Reminder> {
    let title = patch.title.as_deref().map(clean_title).transpose()?;
    if let Some(t) = patch.remind_at {
        check_time(t)?;
    }
    if let Some(l) = patch.lead_min {
        check_lead(l)?;
    }
    let (id, now) = (id.to_owned(), clock.now_ms());
    db.write(move |tx| {
        let current = load(tx, &id)?;
        let new_title = title.unwrap_or_else(|| current.title.clone());
        let new_time = patch.remind_at.unwrap_or(current.remind_at);
        let new_lead = patch.lead_min.unwrap_or(current.lead_min);
        let new_sound = patch.sound.unwrap_or(current.sound);
        if new_title == current.title
            && new_time == current.remind_at
            && new_lead == current.lead_min
            && new_sound == current.sound
        {
            return Ok((current, vec![]));
        }
        // A new time is a new reminder as far as firing goes.
        let fired_at = if new_time == current.remind_at {
            current.fired_at
        } else {
            None
        };
        tx.execute(
            "UPDATE reminders SET title = ?1, remind_at = ?2, fired_at = ?3, lead_min = ?4, sound = ?5 WHERE id = ?6",
            params![new_title, new_time, fired_at, new_lead, new_sound, id],
        )?;
        let reminder = load(tx, &id)?;
        Ok((
            reminder.clone(),
            vec![Event::ReminderUpdated { at: now, reminder }],
        ))
    })
    .await
}

/// Tick a reminder off, or put it back. A reminder marked done before its time never fires.
pub async fn set_done(db: &Database, clock: &dyn Clock, id: &str, done: bool) -> Result<Reminder> {
    let (id, now) = (id.to_owned(), clock.now_ms());
    db.write(move |tx| {
        let current = load(tx, &id)?;
        if current.done_at.is_some() == done {
            return Ok((current, vec![]));
        }
        tx.execute(
            "UPDATE reminders SET done_at = ?1 WHERE id = ?2",
            params![done.then_some(now), id],
        )?;
        let reminder = load(tx, &id)?;
        Ok((
            reminder.clone(),
            vec![Event::ReminderUpdated { at: now, reminder }],
        ))
    })
    .await
}

/// Star or unstar a reminder (FR-107).
pub async fn set_starred(
    db: &Database,
    clock: &dyn Clock,
    id: &str,
    starred: bool,
) -> Result<Reminder> {
    let (id, now) = (id.to_owned(), clock.now_ms());
    db.write(move |tx| {
        let current = load(tx, &id)?;
        if current.starred == starred {
            return Ok((current, vec![]));
        }
        tx.execute(
            "UPDATE reminders SET starred = ?1 WHERE id = ?2",
            params![starred, id],
        )?;
        let reminder = load(tx, &id)?;
        Ok((
            reminder.clone(),
            vec![Event::ReminderUpdated { at: now, reminder }],
        ))
    })
    .await
}

pub async fn delete(db: &Database, clock: &dyn Clock, id: &str) -> Result<()> {
    let (id, now) = (id.to_owned(), clock.now_ms());
    db.write(move |tx| {
        if tx.execute("DELETE FROM reminders WHERE id = ?1", [&id])? == 0 {
            return Err(not_found());
        }
        Ok(((), vec![Event::ReminderDeleted { at: now, id }]))
    })
    .await
}

/// Pending ones first (starred first, FR-107) by time, then (with `include_done`) the finished ones, also by time.
pub async fn list(db: &Database, include_done: bool) -> Result<Vec<Reminder>> {
    db.read(move |conn| {
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM reminders
              WHERE (?1 OR done_at IS NULL)
              ORDER BY (done_at IS NOT NULL), starred DESC, remind_at ASC, id ASC"
        ))?;
        let rows = stmt
            .query_map([include_done], from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    })
    .await
}

/// The time of the earliest reminder that has not fired and is not done.
pub async fn next_pending(db: &Database) -> Result<Option<i64>> {
    db.read(|conn| {
        Ok(conn.query_row(
            "SELECT MIN(remind_at) FROM reminders WHERE done_at IS NULL AND fired_at IS NULL",
            [],
            |r| r.get(0),
        )?)
    })
    .await
}

/// Reminders whose time has come and that have not fired yet, oldest first.
pub async fn due(db: &Database, now: i64) -> Result<Vec<Reminder>> {
    db.read(move |conn| {
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM reminders
              WHERE done_at IS NULL AND fired_at IS NULL AND remind_at <= ?1
              ORDER BY remind_at ASC, id ASC"
        ))?;
        let rows = stmt
            .query_map([now], from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    })
    .await
}

/// Record that a reminder fired and publish `ReminderDue` for it. Returns `None` if it had
/// already fired, is done, is not due yet or is gone, so a reminder fires once however many
/// times this is called.
pub async fn mark_fired(db: &Database, clock: &dyn Clock, id: &str) -> Result<Option<Reminder>> {
    let (id, now) = (id.to_owned(), clock.now_ms());
    db.write(move |tx| {
        let changed = tx.execute(
            "UPDATE reminders SET fired_at = ?1
              WHERE id = ?2 AND fired_at IS NULL AND done_at IS NULL AND remind_at <= ?1",
            params![now, id],
        )?;
        if changed == 0 {
            return Ok((None, vec![]));
        }
        let reminder = load(tx, &id)?;
        Ok((
            Some(reminder.clone()),
            vec![Event::ReminderDue { at: now, reminder }],
        ))
    })
    .await
}

/// Fire everything that is due now. Returns what fired.
pub async fn fire_due(db: &Database, clock: &dyn Clock) -> Result<Vec<Reminder>> {
    let mut fired = Vec::new();
    for reminder in due(db, clock.now_ms()).await? {
        if let Some(r) = mark_fired(db, clock, &reminder.id).await? {
            fired.push(r);
        }
    }
    Ok(fired)
}

/// Wakes the reminder scheduler early. Cheap to clone-share via the scheduler.
pub struct ReminderScheduler {
    task: JoinHandle<()>,
    rearm: Arc<Notify>,
}

/// Start the reminder scheduler: fire what is already due (so reminders missed while Pluto was
/// closed surface once), then sleep until the next pending `remind_at`. It re-plans whenever it
/// hears a `Reminder*` event, and stops when the app shuts down. It never polls.
pub fn spawn_scheduler(
    db: Arc<Database>,
    clock: Arc<dyn Clock>,
    bus: &EventBus,
) -> ReminderScheduler {
    let mut events = bus.subscribe();
    let rearm = Arc::new(Notify::new());
    let wake = Arc::clone(&rearm);
    let task = tokio::spawn(async move {
        loop {
            if let Err(error) = fire_due(&db, &*clock).await {
                tracing::warn!(code = ?error.code, "couldn't fire due reminders; will retry at the next change");
            }
            let deadline = match next_pending(&db).await {
                Ok(Some(at)) => {
                    let wait = Duration::from_millis((at - clock.now_ms()).max(0) as u64);
                    Some(Instant::now() + wait.min(MAX_SLEEP))
                }
                Ok(None) => None,
                Err(error) => {
                    tracing::warn!(code = ?error.code, "couldn't look up the next reminder");
                    None
                }
            };
            if !wait_for_change(&mut events, &wake, deadline).await {
                break;
            }
        }
    });
    ReminderScheduler { task, rearm }
}

/// Park until the deadline, a `Reminder*` event, a resync or a manual re-arm. `false` means stop.
async fn wait_for_change(
    events: &mut Subscriber,
    wake: &Notify,
    deadline: Option<Instant>,
) -> bool {
    loop {
        let heard = async {
            match deadline {
                Some(at) => tokio::select! {
                    () = tokio::time::sleep_until(at) => Heard::Replan,
                    heard = next_event(events) => heard,
                },
                None => next_event(events).await,
            }
        };
        tokio::select! {
            heard = heard => match heard {
                Heard::Replan => return true,
                Heard::Ignore => {}
                Heard::Stop => return false,
            },
            () = wake.notified() => return true,
        }
    }
}

enum Heard {
    Replan,
    Ignore,
    Stop,
}

async fn next_event(events: &mut Subscriber) -> Heard {
    match events.recv().await {
        Recv::Event(
            Event::ReminderCreated { .. }
            | Event::ReminderUpdated { .. }
            | Event::ReminderDeleted { .. }
            | Event::ReminderDue { .. },
        )
        | Recv::Resync { .. } => Heard::Replan,
        Recv::Event(Event::AppShuttingDown { .. }) | Recv::Closed => Heard::Stop,
        Recv::Event(_) => Heard::Ignore,
    }
}

impl ReminderScheduler {
    /// Call when the OS resumes from sleep or the clock jumps: the sleeping timer was set against
    /// a clock that has since moved, so work out where we are now.
    pub fn rearm(&self) {
        self.rearm.notify_one();
    }

    pub fn stop(&self) {
        self.task.abort();
    }
}

#[cfg(test)]
impl ReminderScheduler {
    /// Wait for the task to end by itself (it must not be aborted).
    async fn task_finished(mut self) {
        let task = std::mem::replace(&mut self.task, tokio::spawn(async {}));
        task.await.expect("the scheduler ends cleanly");
    }
}

impl Drop for ReminderScheduler {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[cfg(test)]
mod tests {
    use chrono_tz::Tz;

    use super::*;
    use crate::clock::{FakeClock, SystemClock};
    use crate::error::ErrorCode;
    use crate::notes::{self, NoteInput};

    struct Fixture {
        _dir: tempfile::TempDir,
        bus: EventBus,
        db: Arc<Database>,
        clock: FakeClock,
    }

    fn setup() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let bus = EventBus::default();
        let db = Arc::new(Database::open(&dir.path().join("pluto.db"), bus.clone()).unwrap());
        Fixture {
            _dir: dir,
            bus,
            db,
            clock: FakeClock::new(10_000, Tz::UTC),
        }
    }

    fn input(title: &str, remind_at: i64) -> ReminderInput {
        ReminderInput {
            title: title.into(),
            remind_at,
            ..Default::default()
        }
    }

    async fn event(sub: &mut Subscriber) -> Event {
        match sub.recv().await {
            Recv::Event(e) => e,
            other => panic!("expected an event, got {other:?}"),
        }
    }

    // ---- validation and CRUD --------------------------------------------------------------------

    #[tokio::test]
    async fn a_new_reminder_is_trimmed_pending_and_announced() {
        let f = setup();
        let mut sub = f.bus.subscribe();
        let r = create(&f.db, &f.clock, input("  Call Sam  ", 50_000))
            .await
            .unwrap();
        assert_eq!(r.title, "Call Sam");
        assert_eq!(
            (r.remind_at, r.fired_at, r.done_at, r.created_at),
            (50_000, None, None, 10_000)
        );
        assert_eq!(
            event(&mut sub).await,
            Event::ReminderCreated {
                at: 10_000,
                reminder: r
            }
        );
    }

    #[tokio::test]
    async fn a_task_reminder_keeps_its_task_lead_and_sound() {
        use crate::tasks::{self, TaskInput};
        let f = setup();
        let task = tasks::create(
            &f.db,
            &f.clock,
            TaskInput {
                title: "Submit report".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let r = create(
            &f.db,
            &f.clock,
            ReminderInput {
                title: "Submit report".into(),
                remind_at: 50_000,
                task_id: Some(task.id.clone()),
                lead_min: Some(5),
                sound: Some(false),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(
            (r.task_id.as_deref(), r.lead_min, r.sound),
            (Some(task.id.as_str()), 5, false)
        );
        // Sound defaults to on; lead and sound can change; more than a day is refused.
        assert!(create(&f.db, &f.clock, input("x", 5)).await.unwrap().sound);
        let r = update(
            &f.db,
            &f.clock,
            &r.id,
            ReminderPatch {
                lead_min: Some(15),
                sound: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!((r.lead_min, r.sound), (15, true));
        let err = update(
            &f.db,
            &f.clock,
            &r.id,
            ReminderPatch {
                lead_min: Some(LEAD_MAX + 1),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
        assert_eq!(err.field.as_deref(), Some("lead_min"));
        // A task that is gone can't get a reminder.
        let err = create(
            &f.db,
            &f.clock,
            ReminderInput {
                title: "y".into(),
                remind_at: 5,
                task_id: Some("nope".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
        assert_eq!(err.field.as_deref(), Some("task_id"));
    }

    #[tokio::test]
    async fn titles_and_times_are_validated_with_the_field_named() {
        let f = setup();
        for bad in ["", "   "] {
            let err = create(&f.db, &f.clock, input(bad, 5)).await.unwrap_err();
            assert_eq!(
                (err.code, err.field.as_deref()),
                (ErrorCode::Validation, Some("title"))
            );
        }
        let err = create(&f.db, &f.clock, input(&"x".repeat(201), 5))
            .await
            .unwrap_err();
        assert_eq!(err.field.as_deref(), Some("title"));
        assert!(create(&f.db, &f.clock, input(&"é".repeat(200), 5))
            .await
            .is_ok());
        for bad in [0, -5] {
            let err = create(&f.db, &f.clock, input("t", bad)).await.unwrap_err();
            assert_eq!(
                (err.code, err.field.as_deref()),
                (ErrorCode::Validation, Some("remind_at"))
            );
        }
        assert_eq!(
            list(&f.db, true).await.unwrap().len(),
            1,
            "rejected ones were not saved"
        );
    }

    #[tokio::test]
    async fn a_past_time_is_allowed_and_a_missing_note_is_not() {
        let f = setup();
        assert!(create(&f.db, &f.clock, input("late", 1)).await.is_ok());
        let err = create(
            &f.db,
            &f.clock,
            ReminderInput {
                note_id: Some("nope".into()),
                ..input("t", 5)
            },
        )
        .await
        .unwrap_err();
        assert_eq!(
            (err.code, err.field.as_deref()),
            (ErrorCode::NotFound, Some("note_id"))
        );
    }

    #[tokio::test]
    async fn the_list_is_by_time_with_done_ones_last_and_hidden_unless_asked() {
        let f = setup();
        let late = create(&f.db, &f.clock, input("late", 300)).await.unwrap();
        let early = create(&f.db, &f.clock, input("early", 100)).await.unwrap();
        let done = create(&f.db, &f.clock, input("done", 50)).await.unwrap();
        set_done(&f.db, &f.clock, &done.id, true).await.unwrap();
        let ids = |v: Vec<Reminder>| v.into_iter().map(|r| r.id).collect::<Vec<_>>();
        assert_eq!(
            ids(list(&f.db, false).await.unwrap()),
            [early.id.clone(), late.id.clone()]
        );
        assert_eq!(
            ids(list(&f.db, true).await.unwrap()),
            [early.id, late.id, done.id]
        );
    }

    #[tokio::test]
    async fn a_starred_reminder_comes_first_among_the_pending_ones() {
        let f = setup();
        let early = create(&f.db, &f.clock, input("early", 100)).await.unwrap();
        let late = create(&f.db, &f.clock, input("late", 300)).await.unwrap();
        let starred = set_starred(&f.db, &f.clock, &late.id, true).await.unwrap();
        assert!(starred.starred);
        let ids = |v: Vec<Reminder>| v.into_iter().map(|r| r.id).collect::<Vec<_>>();
        assert_eq!(
            ids(list(&f.db, false).await.unwrap()),
            [late.id.clone(), early.id.clone()]
        );
        assert!(
            !set_starred(&f.db, &f.clock, &late.id, false)
                .await
                .unwrap()
                .starred
        );
        assert_eq!(
            set_starred(&f.db, &f.clock, "nope", true)
                .await
                .unwrap_err()
                .code,
            crate::error::ErrorCode::NotFound
        );
    }

    #[tokio::test]
    async fn changing_the_time_clears_fired_at_but_changing_the_title_does_not() {
        let f = setup();
        let r = create(&f.db, &f.clock, input("t", 5_000)).await.unwrap();
        let fired = mark_fired(&f.db, &f.clock, &r.id).await.unwrap().unwrap();
        assert_eq!(fired.fired_at, Some(10_000));

        let renamed = update(
            &f.db,
            &f.clock,
            &r.id,
            ReminderPatch {
                title: Some("renamed".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(
            (renamed.title.as_str(), renamed.fired_at),
            ("renamed", Some(10_000))
        );

        let moved = update(
            &f.db,
            &f.clock,
            &r.id,
            ReminderPatch {
                remind_at: Some(99_000),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!((moved.remind_at, moved.fired_at), (99_000, None));
    }

    #[tokio::test]
    async fn an_update_that_changes_nothing_says_nothing() {
        let f = setup();
        let r = create(&f.db, &f.clock, input("t", 5_000)).await.unwrap();
        let mut sub = f.bus.subscribe();
        let same = update(
            &f.db,
            &f.clock,
            &r.id,
            ReminderPatch {
                title: Some(" t ".into()),
                remind_at: Some(5_000),
                lead_min: Some(0),
                sound: Some(true),
            },
        )
        .await
        .unwrap();
        assert_eq!(same, r);
        f.bus.publish(Event::AppReady {
            at: 0,
            startup_ms: 0,
        });
        assert!(
            matches!(event(&mut sub).await, Event::AppReady { .. }),
            "no event for a no-op"
        );
        let err = update(
            &f.db,
            &f.clock,
            &r.id,
            ReminderPatch {
                title: Some("".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
        assert_eq!(err.field.as_deref(), Some("title"));
    }

    #[tokio::test]
    async fn done_can_be_toggled_and_delete_removes_the_reminder() {
        let f = setup();
        let r = create(&f.db, &f.clock, input("t", 5_000)).await.unwrap();
        let mut sub = f.bus.subscribe();
        let done = set_done(&f.db, &f.clock, &r.id, true).await.unwrap();
        assert_eq!(done.done_at, Some(10_000));
        assert!(matches!(
            event(&mut sub).await,
            Event::ReminderUpdated { .. }
        ));
        assert_eq!(
            set_done(&f.db, &f.clock, &r.id, false)
                .await
                .unwrap()
                .done_at,
            None
        );
        delete(&f.db, &f.clock, &r.id).await.unwrap();
        assert!(matches!(
            event(&mut sub).await,
            Event::ReminderUpdated { .. }
        ));
        assert_eq!(
            event(&mut sub).await,
            Event::ReminderDeleted {
                at: 10_000,
                id: r.id.clone()
            }
        );
        assert_eq!(
            delete(&f.db, &f.clock, &r.id).await.unwrap_err().code,
            ErrorCode::NotFound
        );
        assert_eq!(
            set_done(&f.db, &f.clock, &r.id, true)
                .await
                .unwrap_err()
                .code,
            ErrorCode::NotFound
        );
    }

    // ---- what is due ----------------------------------------------------------------------------

    #[tokio::test]
    async fn due_and_next_pending_ignore_fired_and_done_reminders() {
        let f = setup();
        let a = create(&f.db, &f.clock, input("a", 100)).await.unwrap();
        let b = create(&f.db, &f.clock, input("b", 200)).await.unwrap();
        let c = create(&f.db, &f.clock, input("c", 900)).await.unwrap();
        assert_eq!(next_pending(&f.db).await.unwrap(), Some(100));
        let ids: Vec<_> = due(&f.db, 500)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(ids, [a.id.clone(), b.id.clone()]);

        f.clock.set_now_ms(500);
        mark_fired(&f.db, &f.clock, &a.id).await.unwrap();
        set_done(&f.db, &f.clock, &b.id, true).await.unwrap();
        assert!(due(&f.db, 500).await.unwrap().is_empty());
        assert_eq!(next_pending(&f.db).await.unwrap(), Some(900));
        set_done(&f.db, &f.clock, &c.id, true).await.unwrap();
        assert_eq!(next_pending(&f.db).await.unwrap(), None);
    }

    #[tokio::test]
    async fn a_reminder_fires_once_and_not_before_its_time() {
        let f = setup();
        let r = create(&f.db, &f.clock, input("t", 20_000)).await.unwrap();
        assert!(
            mark_fired(&f.db, &f.clock, &r.id).await.unwrap().is_none(),
            "not due yet"
        );
        f.clock.set_now_ms(20_000);
        let mut sub = f.bus.subscribe();
        let fired = mark_fired(&f.db, &f.clock, &r.id).await.unwrap().unwrap();
        assert_eq!(fired.fired_at, Some(20_000));
        assert_eq!(
            event(&mut sub).await,
            Event::ReminderDue {
                at: 20_000,
                reminder: fired
            }
        );
        assert!(
            mark_fired(&f.db, &f.clock, &r.id).await.unwrap().is_none(),
            "already fired"
        );
    }

    #[tokio::test]
    async fn deleting_a_note_for_good_unlinks_its_reminder() {
        let f = setup();
        let note = notes::create(
            &f.db,
            &f.clock,
            NoteInput {
                title: "n".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let r = create(
            &f.db,
            &f.clock,
            ReminderInput {
                note_id: Some(note.id.clone()),
                ..input("t", 5_000)
            },
        )
        .await
        .unwrap();
        assert_eq!(r.note_id.as_deref(), Some(note.id.as_str()));
        notes::delete(&f.db, &f.clock, &note.id).await.unwrap();
        notes::purge(&f.db, &f.clock, &note.id).await.unwrap();
        assert_eq!(list(&f.db, true).await.unwrap()[0].note_id, None);
    }

    // ---- the scheduler ----------------------------------------------------------------------------
    //
    // These use real (short) delays rather than tokio's paused time: the database runs on its own
    // threads, and paused time jumps forward whenever the test is only waiting on one of them,
    // which would fire reminders before the test could change them.

    fn clock() -> Arc<SystemClock> {
        Arc::new(SystemClock::new())
    }

    /// Next `ReminderDue` seen by the subscriber, skipping every other event.
    async fn next_due(sub: &mut Subscriber) -> (i64, Reminder) {
        let wait = async {
            loop {
                if let Event::ReminderDue { at, reminder } = event(sub).await {
                    return (at, reminder);
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(10), wait)
            .await
            .expect("a reminder should have fired")
    }

    /// Everything the subscriber hears for `ms` more milliseconds must contain no `ReminderDue`.
    async fn assert_no_due_for(sub: &mut Subscriber, ms: u64) {
        let quiet = async {
            loop {
                if let Event::ReminderDue { .. } = event(sub).await {
                    return;
                }
            }
        };
        let heard = tokio::time::timeout(Duration::from_millis(ms), quiet).await;
        assert!(heard.is_err(), "a reminder fired when none should have");
    }

    #[tokio::test]
    async fn the_scheduler_sleeps_until_the_time_then_fires_exactly_once() {
        let f = setup();
        let clock = clock();
        let mut sub = f.bus.subscribe();
        let scheduler = spawn_scheduler(f.db.clone(), clock.clone(), &f.bus);
        let at = clock.now_ms() + 300;
        let r = create(&f.db, &*clock, input("stand up", at)).await.unwrap();

        let (fired_at, fired) = next_due(&mut sub).await;
        assert_eq!(fired.id, r.id);
        assert!(
            fired_at >= at,
            "fired at its time, not before: {fired_at} < {at}"
        );
        assert_eq!(fired.fired_at, Some(fired_at));

        assert_no_due_for(&mut sub, 400).await;
        assert!(due(&f.db, clock.now_ms()).await.unwrap().is_empty());
        scheduler.stop();
    }

    #[tokio::test]
    async fn reminders_already_past_due_at_startup_fire_once() {
        let f = setup();
        let clock = clock();
        create(&f.db, &*clock, input("missed", 1_000))
            .await
            .unwrap();
        let mut sub = f.bus.subscribe();
        let scheduler = spawn_scheduler(f.db.clone(), clock.clone(), &f.bus);
        let (_, fired) = next_due(&mut sub).await;
        assert_eq!(fired.title, "missed");
        scheduler.stop();

        // The next launch does not fire it again.
        let again = spawn_scheduler(f.db.clone(), clock.clone(), &f.bus);
        assert_no_due_for(&mut sub, 300).await;
        again.stop();
    }

    #[tokio::test]
    async fn a_sooner_reminder_created_while_sleeping_is_not_overslept() {
        let f = setup();
        let clock = clock();
        let scheduler = spawn_scheduler(f.db.clone(), clock.clone(), &f.bus);
        let mut sub = f.bus.subscribe();
        create(&f.db, &*clock, input("far", clock.now_ms() + 3_600_000))
            .await
            .unwrap();
        let soon = clock.now_ms() + 300;
        create(&f.db, &*clock, input("soon", soon)).await.unwrap();
        let (at, fired) = next_due(&mut sub).await;
        assert_eq!(fired.title, "soon");
        assert!((soon..soon + 2_000).contains(&at), "{at}");
        assert_no_due_for(&mut sub, 300).await;
        scheduler.stop();
    }

    #[tokio::test]
    async fn moving_deleting_or_finishing_a_reminder_cancels_the_old_time() {
        let f = setup();
        let clock = clock();
        let scheduler = spawn_scheduler(f.db.clone(), clock.clone(), &f.bus);
        let mut sub = f.bus.subscribe();
        let base = clock.now_ms();
        let moved = create(&f.db, &*clock, input("moved", base + 1_500))
            .await
            .unwrap();
        let gone = create(&f.db, &*clock, input("gone", base + 1_600))
            .await
            .unwrap();
        let done = create(&f.db, &*clock, input("done", base + 1_700))
            .await
            .unwrap();
        let later = base + 2_500;
        update(
            &f.db,
            &*clock,
            &moved.id,
            ReminderPatch {
                remind_at: Some(later),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        delete(&f.db, &*clock, &gone.id).await.unwrap();
        set_done(&f.db, &*clock, &done.id, true).await.unwrap();

        let (at, fired) = next_due(&mut sub).await;
        assert_eq!(fired.id, moved.id, "only the moved one is left to fire");
        assert!(at >= later, "{at} < {later}");
        scheduler.stop();
    }

    #[tokio::test]
    async fn the_scheduler_stops_when_the_app_shuts_down() {
        let f = setup();
        let scheduler = spawn_scheduler(f.db.clone(), clock(), &f.bus);
        f.bus.publish(Event::AppShuttingDown { at: 0 });
        tokio::time::timeout(Duration::from_secs(5), scheduler.task_finished())
            .await
            .expect("the scheduler ends by itself");
    }

    #[test]
    fn the_scheduler_has_no_polling_loop() {
        let src = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/reminders.rs"),
        )
        .unwrap();
        let production = src.split("#[cfg(test)]").next().unwrap();
        for banned in ["interval(", "thread::sleep"] {
            assert!(
                !production.contains(banned),
                "{banned} would be polling (Principle 1)"
            );
        }
    }
}
