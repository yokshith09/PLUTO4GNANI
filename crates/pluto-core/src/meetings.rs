//! Meetings (PRD §6.3, R1-30…R1-35; TRD §4.3): a title and time, participants, Markdown notes,
//! decisions, action items and a follow-up date. An action item can become a task; the item then
//! shows that task's status, read fresh every time.
//!
//! A meeting is saved as a whole (`update` replaces participants, decisions and action items in
//! one transaction). Decisions and action items keep their ids across saves, so a converted
//! action item stays linked to its task. New items may bring their own id (the UI makes one) so
//! the page never has to match rows up after a save.

use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::clock::Clock;
use crate::db::Database;
use crate::error::{AppError, Result};
use crate::events::Event;
use crate::ids::new_id;
use crate::tasks::{self, TaskInput, TaskStatus};

pub const TITLE_MAX: usize = 200;
pub const NOTES_MAX: usize = 200_000;
pub const PARTICIPANT_MAX: usize = 200;
pub const PARTICIPANTS_MAX: usize = 50;
pub const DECISION_MAX: usize = 2_000;
pub const ACTION_MAX: usize = 300;
pub const ITEMS_MAX: usize = 100;
/// Ids the UI may choose for new items: short, plain ASCII.
const ID_MAX: usize = 64;

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    pub id: String,
    pub body: String,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionItem {
    pub id: String,
    pub body: String,
    pub assignee: Option<String>,
    /// The task made from this item, if any (and if it still exists).
    pub task_id: Option<String>,
    /// That task's status now (R1-34: "shows linked task status live").
    pub task_status: Option<TaskStatus>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meeting {
    pub id: String,
    pub title: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub starts_at: i64,
    pub notes: String,
    pub follow_up_date: Option<NaiveDate>,
    pub participants: Vec<String>,
    pub decisions: Vec<Decision>,
    pub action_items: Vec<ActionItem>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub updated_at: i64,
}

/// One row of the meetings list.
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeetingSummary {
    pub id: String,
    pub title: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub starts_at: i64,
    pub participants: Vec<String>,
    pub follow_up_date: Option<NaiveDate>,
    pub decisions: u32,
    pub action_items: u32,
    /// Action items not yet turned into a task, or whose task is not finished.
    pub open_action_items: u32,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionInput {
    /// Keep an existing decision's id; `None` (or an id the UI just made) adds one.
    pub id: Option<String>,
    pub body: String,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionItemInput {
    pub id: Option<String>,
    pub body: String,
    pub assignee: Option<String>,
}

/// The whole meeting as the editor holds it. Blank decisions and action items are dropped.
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeetingInput {
    pub title: String,
    /// Defaults to now (R1-30).
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub starts_at: Option<i64>,
    #[serde(default)]
    pub notes: String,
    pub follow_up_date: Option<NaiveDate>,
    #[serde(default)]
    pub participants: Vec<String>,
    #[serde(default)]
    pub decisions: Vec<DecisionInput>,
    #[serde(default)]
    pub action_items: Vec<ActionItemInput>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeetingFilters {
    /// Meetings with this participant (case-insensitive, exact name).
    pub participant: Option<String>,
    /// Starting at or after (ms).
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub from: Option<i64>,
    /// Starting before (ms).
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub to: Option<i64>,
}

// ---- validation ----------------------------------------------------------------------------

fn too_long(field: &str, what: &str, max: usize) -> AppError {
    AppError::validation(field, format!("{what} can be up to {max} characters."))
}

fn clean_title(title: &str) -> Result<String> {
    let t = title.trim();
    match t.chars().count() {
        0 => Err(AppError::validation("title", "Give the meeting a title.")),
        n if n > TITLE_MAX => Err(too_long("title", "Meeting titles", TITLE_MAX)),
        _ => Ok(t.to_owned()),
    }
}

/// Trimmed, blanks dropped, duplicates (ignoring case) dropped, order kept.
fn clean_participants(names: &[String]) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for name in names.iter().map(|n| n.trim()).filter(|n| !n.is_empty()) {
        if name.chars().count() > PARTICIPANT_MAX {
            return Err(too_long("participants", "Names", PARTICIPANT_MAX));
        }
        if !out.iter().any(|o| o.to_lowercase() == name.to_lowercase()) {
            out.push(name.to_owned());
        }
    }
    if out.len() > PARTICIPANTS_MAX {
        return Err(AppError::validation(
            "participants",
            format!("A meeting can have up to {PARTICIPANTS_MAX} participants."),
        ));
    }
    Ok(out)
}

fn clean_id(id: &Option<String>, field: &str) -> Result<Option<String>> {
    match id.as_deref().map(str::trim).filter(|i| !i.is_empty()) {
        None => Ok(None),
        Some(i)
            if i.len() <= ID_MAX
                && i.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') =>
        {
            Ok(Some(i.to_owned()))
        }
        Some(_) => Err(AppError::validation(field, "That item id isn't valid.")),
    }
}

struct Clean {
    title: String,
    starts_at: Option<i64>,
    notes: String,
    follow_up_date: Option<NaiveDate>,
    participants: Vec<String>,
    decisions: Vec<(Option<String>, String)>,
    actions: Vec<(Option<String>, String, Option<String>)>,
}

fn clean(input: MeetingInput) -> Result<Clean> {
    if input.notes.chars().count() > NOTES_MAX {
        return Err(too_long("notes", "Meeting notes", NOTES_MAX));
    }
    let mut decisions = Vec::new();
    for d in &input.decisions {
        let body = d.body.trim();
        if body.is_empty() {
            continue;
        }
        if body.chars().count() > DECISION_MAX {
            return Err(too_long("decisions", "Decisions", DECISION_MAX));
        }
        decisions.push((clean_id(&d.id, "decisions")?, body.to_owned()));
    }
    let mut actions = Vec::new();
    for a in &input.action_items {
        let body = a.body.trim();
        if body.is_empty() {
            continue;
        }
        if body.chars().count() > ACTION_MAX {
            return Err(too_long("action_items", "Action items", ACTION_MAX));
        }
        let assignee = a
            .assignee
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        if assignee
            .as_deref()
            .is_some_and(|s| s.chars().count() > PARTICIPANT_MAX)
        {
            return Err(too_long("action_items", "Names", PARTICIPANT_MAX));
        }
        actions.push((clean_id(&a.id, "action_items")?, body.to_owned(), assignee));
    }
    if decisions.len() > ITEMS_MAX || actions.len() > ITEMS_MAX {
        return Err(AppError::validation(
            "decisions",
            format!("A meeting can have up to {ITEMS_MAX} decisions and {ITEMS_MAX} action items."),
        ));
    }
    if let Some(t) = input.starts_at {
        if t <= 0 {
            return Err(AppError::validation(
                "starts_at",
                "Pick a date and time for the meeting.",
            ));
        }
    }
    Ok(Clean {
        title: clean_title(&input.title)?,
        starts_at: input.starts_at,
        notes: input.notes,
        follow_up_date: input.follow_up_date,
        participants: clean_participants(&input.participants)?,
        decisions,
        actions,
    })
}

// ---- storage -------------------------------------------------------------------------------

fn not_found() -> AppError {
    AppError::not_found("That meeting no longer exists.")
}

fn parse_date(s: Option<String>) -> Option<NaiveDate> {
    s.and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok())
}

fn participants_of(conn: &Connection, id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare_cached(
        "SELECT name FROM meeting_participants WHERE meeting_id = ?1 ORDER BY position",
    )?;
    let rows = stmt
        .query_map([id], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    Ok(rows)
}

fn load(conn: &Connection, id: &str) -> Result<Meeting> {
    let head = conn
        .query_row(
            "SELECT id, title, starts_at, notes, follow_up_date, created_at, updated_at FROM meetings WHERE id = ?1",
            [id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, i64>(6)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(not_found)?;
    let decisions = {
        let mut stmt = conn.prepare_cached(
            "SELECT id, body FROM meeting_decisions WHERE meeting_id = ?1 ORDER BY position",
        )?;
        let rows = stmt
            .query_map([id], |r| {
                Ok(Decision {
                    id: r.get(0)?,
                    body: r.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    let action_items = {
        let mut stmt = conn.prepare_cached(
            "SELECT a.id, a.body, a.assignee, t.id, t.status
             FROM meeting_action_items a LEFT JOIN tasks t ON t.id = a.task_id
             WHERE a.meeting_id = ?1 ORDER BY a.position",
        )?;
        let rows = stmt
            .query_map([id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(id, body, assignee, task_id, status)| ActionItem {
                id,
                body,
                assignee,
                task_id,
                task_status: status
                    .as_deref()
                    .and_then(|s| TaskStatus::ALL.into_iter().find(|t| t.as_str() == s)),
            })
            .collect()
    };
    Ok(Meeting {
        participants: participants_of(conn, id)?,
        id: head.0,
        title: head.1,
        starts_at: head.2,
        notes: head.3,
        follow_up_date: parse_date(head.4),
        decisions,
        action_items,
        created_at: head.5,
        updated_at: head.6,
    })
}

/// Replace participants, decisions and action items. Items keep their id, and an action item
/// that keeps its id keeps its task.
fn write_parts(tx: &Connection, id: &str, c: &Clean) -> Result<()> {
    tx.execute(
        "DELETE FROM meeting_participants WHERE meeting_id = ?1",
        [id],
    )?;
    for (i, name) in c.participants.iter().enumerate() {
        tx.execute(
            "INSERT INTO meeting_participants (meeting_id, position, name) VALUES (?1, ?2, ?3)",
            params![id, i as i64, name],
        )?;
    }

    let keep: Vec<String> = c.decisions.iter().filter_map(|(i, _)| i.clone()).collect();
    delete_missing(tx, "meeting_decisions", id, &keep)?;
    for (pos, (item_id, body)) in c.decisions.iter().enumerate() {
        let item_id = item_id.clone().unwrap_or_else(new_id);
        owned_by(tx, "meeting_decisions", &item_id, id)?;
        tx.execute(
            "INSERT INTO meeting_decisions (id, meeting_id, position, body) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET position = excluded.position, body = excluded.body",
            params![item_id, id, pos as i64, body],
        )?;
    }

    let keep: Vec<String> = c.actions.iter().filter_map(|(i, _, _)| i.clone()).collect();
    delete_missing(tx, "meeting_action_items", id, &keep)?;
    for (pos, (item_id, body, assignee)) in c.actions.iter().enumerate() {
        let item_id = item_id.clone().unwrap_or_else(new_id);
        owned_by(tx, "meeting_action_items", &item_id, id)?;
        tx.execute(
            "INSERT INTO meeting_action_items (id, meeting_id, position, body, assignee) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET position = excluded.position, body = excluded.body, assignee = excluded.assignee",
            params![item_id, id, pos as i64, body, assignee],
        )?;
    }
    Ok(())
}

/// Delete this meeting's rows in `table` whose id is not in `keep`.
fn delete_missing(tx: &Connection, table: &str, meeting_id: &str, keep: &[String]) -> Result<()> {
    let mut stmt = tx.prepare(&format!("SELECT id FROM {table} WHERE meeting_id = ?1"))?;
    let existing = stmt
        .query_map([meeting_id], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for old in existing.iter().filter(|e| !keep.contains(e)) {
        tx.execute(&format!("DELETE FROM {table} WHERE id = ?1"), [old])?;
    }
    Ok(())
}

/// An item id that already belongs to another meeting is refused (it would move the row).
fn owned_by(tx: &Connection, table: &str, item_id: &str, meeting_id: &str) -> Result<()> {
    let owner: Option<String> = tx
        .query_row(
            &format!("SELECT meeting_id FROM {table} WHERE id = ?1"),
            [item_id],
            |r| r.get(0),
        )
        .optional()?;
    match owner {
        Some(o) if o != meeting_id => {
            Err(AppError::conflict("That item belongs to another meeting."))
        }
        _ => Ok(()),
    }
}

fn date_text(d: Option<NaiveDate>) -> Option<String> {
    d.map(|d| d.format("%Y-%m-%d").to_string())
}

// ---- operations ----------------------------------------------------------------------------

pub async fn get(db: &Database, id: &str) -> Result<Meeting> {
    let id = id.to_owned();
    db.read(move |conn| load(conn, &id)).await
}

pub async fn create(db: &Database, clock: &dyn Clock, input: MeetingInput) -> Result<Meeting> {
    let c = clean(input)?;
    let (id, now) = (new_id(), clock.now_ms());
    db.write(move |tx| {
        tx.execute(
            "INSERT INTO meetings (id, title, starts_at, notes, follow_up_date, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            params![id, c.title, c.starts_at.unwrap_or(now), c.notes, date_text(c.follow_up_date), now],
        )?;
        write_parts(tx, &id, &c)?;
        let meeting = load(tx, &id)?;
        Ok((meeting.clone(), vec![Event::MeetingCreated { at: now, meeting }]))
    })
    .await
}

/// Save the whole meeting. A save that changes nothing writes nothing and says nothing.
pub async fn update(
    db: &Database,
    clock: &dyn Clock,
    id: &str,
    input: MeetingInput,
) -> Result<Meeting> {
    let c = clean(input)?;
    let (id, now) = (id.to_owned(), clock.now_ms());
    db.write(move |tx| {
        let before = load(tx, &id)?;
        tx.execute(
            "UPDATE meetings SET title = ?1, starts_at = ?2, notes = ?3, follow_up_date = ?4 WHERE id = ?5",
            params![c.title, c.starts_at.unwrap_or(before.starts_at), c.notes, date_text(c.follow_up_date), id],
        )?;
        write_parts(tx, &id, &c)?;
        let after = load(tx, &id)?;
        let unchanged = Meeting { updated_at: before.updated_at, ..after.clone() } == before;
        if unchanged {
            return Ok((before, vec![]));
        }
        tx.execute("UPDATE meetings SET updated_at = ?1 WHERE id = ?2", params![now, id])?;
        let meeting = load(tx, &id)?;
        Ok((meeting.clone(), vec![Event::MeetingUpdated { at: now, meeting }]))
    })
    .await
}

/// Delete a meeting with its participants, decisions and action items. Tasks made from its
/// action items stay.
pub async fn delete(db: &Database, clock: &dyn Clock, id: &str) -> Result<()> {
    let (id, now) = (id.to_owned(), clock.now_ms());
    db.write(move |tx| {
        if tx.execute("DELETE FROM meetings WHERE id = ?1", [&id])? == 0 {
            return Err(not_found());
        }
        Ok(((), vec![Event::MeetingDeleted { at: now, id }]))
    })
    .await
}

/// Newest first (R1-35), optionally only those with a participant or within a time range.
pub async fn list(db: &Database, filters: MeetingFilters) -> Result<Vec<MeetingSummary>> {
    db.read(move |conn| {
        let participant = filters
            .participant
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_owned);
        let mut stmt = conn.prepare(
            "SELECT m.id, m.title, m.starts_at, m.follow_up_date,
                (SELECT COUNT(*) FROM meeting_decisions d WHERE d.meeting_id = m.id),
                (SELECT COUNT(*) FROM meeting_action_items a WHERE a.meeting_id = m.id),
                (SELECT COUNT(*) FROM meeting_action_items a LEFT JOIN tasks t ON t.id = a.task_id
                   WHERE a.meeting_id = m.id AND (t.id IS NULL OR t.status NOT IN ('COMPLETED','CANCELLED')))
             FROM meetings m
             WHERE (?1 IS NULL OR m.starts_at >= ?1) AND (?2 IS NULL OR m.starts_at < ?2)
               AND (?3 IS NULL OR EXISTS (SELECT 1 FROM meeting_participants p
                                          WHERE p.meeting_id = m.id AND lower(p.name) = lower(?3)))
             ORDER BY m.starts_at DESC, m.id",
        )?;
        let heads = stmt
            .query_map(params![filters.from, filters.to, participant], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, u32>(4)?,
                    r.get::<_, u32>(5)?,
                    r.get::<_, u32>(6)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        heads
            .into_iter()
            .map(|(id, title, starts_at, follow, d, a, open)| {
                Ok(MeetingSummary {
                    participants: participants_of(conn, &id)?,
                    id,
                    title,
                    starts_at,
                    follow_up_date: parse_date(follow),
                    decisions: d,
                    action_items: a,
                    open_action_items: open,
                })
            })
            .collect()
    })
    .await
}

/// Names used in earlier meetings that start with `prefix` (R1-31), most used first.
pub async fn suggest_participants(db: &Database, prefix: &str) -> Result<Vec<String>> {
    let pattern = format!(
        "{}%",
        prefix
            .trim()
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    );
    db.read(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT name FROM meeting_participants WHERE name LIKE ?1 ESCAPE '\\' COLLATE NOCASE
             GROUP BY lower(name) ORDER BY COUNT(*) DESC, name LIMIT 8",
        )?;
        let names = stmt
            .query_map([pattern], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(names)
    })
    .await
}

/// Turn an action item into a task (R1-34): title = the item, project = the meeting's title,
/// due = the follow-up date, and a description naming the assignee and the meeting. It is
/// planned for today so it shows on Today with the meeting named (FR-52, owner Q6).
pub async fn convert_action_item(
    db: &Database,
    clock: &dyn Clock,
    action_item_id: &str,
) -> Result<tasks::Task> {
    let (item_id, now) = (action_item_id.to_owned(), clock.now_ms());
    let (task_id, local) = (new_id(), clock.local_date(now).to_string());
    let tz_date = |ms: i64| clock.local_date(ms).format("%b %-d, %Y").to_string();
    // Read what the task needs first (the formatting uses the clock, which the writer can't).
    let (meeting_id, body, assignee, linked, title, starts_at, follow): (String, String, Option<String>, Option<String>, String, i64, Option<String>) = db
        .read({
            let item_id = item_id.clone();
            move |conn| {
                conn.query_row(
                    "SELECT a.meeting_id, a.body, a.assignee, t.id, m.title, m.starts_at, m.follow_up_date
                     FROM meeting_action_items a JOIN meetings m ON m.id = a.meeting_id
                     LEFT JOIN tasks t ON t.id = a.task_id WHERE a.id = ?1",
                    [&item_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
                )
                .optional()?
                .ok_or_else(|| AppError::not_found("That action item no longer exists."))
            }
        })
        .await?;
    if linked.is_some() {
        return Err(AppError::conflict("This action item is already a task."));
    }
    let mut description = String::new();
    if let Some(a) = &assignee {
        description.push_str(&format!("Assignee: {a}\n"));
    }
    description.push_str(&format!(
        "From the meeting “{title}” on {}.",
        tz_date(starts_at)
    ));
    let input = tasks::clean_input(TaskInput {
        title: body,
        description,
        project: Some(title.chars().take(tasks::PROJECT_MAX).collect()),
        due_date: parse_date(follow),
        planned_date: Some(clock.local_date(now)),
        ..Default::default()
    })?;
    db.write(move |tx| {
        // Re-check inside the transaction: someone may have converted it meanwhile.
        let still_free: Option<Option<String>> = tx
            .query_row(
                "SELECT task_id FROM meeting_action_items WHERE id = ?1",
                [&item_id],
                |r| r.get(0),
            )
            .optional()?;
        match still_free {
            None => return Err(AppError::not_found("That action item no longer exists.")),
            Some(Some(t)) if tasks_exists(tx, &t)? => {
                return Err(AppError::conflict("This action item is already a task."))
            }
            _ => {}
        }
        let task = tasks::insert(tx, &task_id, now, &local, &input, Some(&item_id))?;
        tx.execute(
            "UPDATE meeting_action_items SET task_id = ?1 WHERE id = ?2",
            params![task_id, item_id],
        )?;
        tx.execute(
            "UPDATE meetings SET updated_at = ?1 WHERE id = ?2",
            params![now, meeting_id],
        )?;
        let meeting = load(tx, &meeting_id)?;
        Ok((
            task.clone(),
            vec![
                Event::TaskCreated { at: now, task },
                Event::MeetingUpdated { at: now, meeting },
            ],
        ))
    })
    .await
}

fn tasks_exists(conn: &Connection, id: &str) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM tasks WHERE id = ?1)",
        [id],
        |r| r.get(0),
    )?)
}

#[cfg(test)]
impl Meeting {
    pub(crate) fn sample() -> Self {
        Self {
            id: "m1".into(),
            title: "Sprint planning".into(),
            starts_at: 1_000,
            notes: String::new(),
            follow_up_date: None,
            participants: vec!["Asha".into()],
            decisions: vec![],
            action_items: vec![],
            created_at: 1,
            updated_at: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono_tz::Tz;

    use super::*;
    use crate::bus::{EventBus, Recv};
    use crate::clock::FakeClock;

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
            clock: FakeClock::new(1_760_000_000_000, Tz::UTC),
        }
    }

    fn input(title: &str) -> MeetingInput {
        MeetingInput {
            title: title.into(),
            ..Default::default()
        }
    }

    fn action(id: Option<&str>, body: &str, who: Option<&str>) -> ActionItemInput {
        ActionItemInput {
            id: id.map(Into::into),
            body: body.into(),
            assignee: who.map(Into::into),
        }
    }

    #[tokio::test]
    async fn create_defaults_the_time_to_now_and_cleans_everything() {
        let f = setup();
        let m = create(
            &f.db,
            &f.clock,
            MeetingInput {
                title: "  Sprint planning ".into(),
                participants: vec![" Asha ".into(), "asha".into(), "".into(), "Ben".into()],
                decisions: vec![
                    DecisionInput {
                        id: None,
                        body: "Ship Friday".into(),
                    },
                    DecisionInput {
                        id: None,
                        body: "  ".into(),
                    },
                ],
                action_items: vec![
                    action(None, "Write notes", Some(" Ben ")),
                    action(None, "", None),
                ],
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(m.title, "Sprint planning");
        assert_eq!(m.starts_at, f.clock.now_ms());
        assert_eq!(m.participants, ["Asha", "Ben"]);
        assert_eq!(m.decisions.len(), 1);
        assert_eq!(m.action_items.len(), 1);
        assert_eq!(m.action_items[0].assignee.as_deref(), Some("Ben"));
        assert_eq!(get(&f.db, &m.id).await.unwrap(), m);
    }

    #[tokio::test]
    async fn titles_are_required_and_bounded() {
        let f = setup();
        assert_eq!(
            create(&f.db, &f.clock, input("  "))
                .await
                .unwrap_err()
                .field
                .as_deref(),
            Some("title")
        );
        assert!(create(&f.db, &f.clock, input(&"x".repeat(TITLE_MAX + 1)))
            .await
            .is_err());
        let bad_id = MeetingInput {
            action_items: vec![action(Some("no spaces!"), "x", None)],
            ..input("m")
        };
        assert!(create(&f.db, &f.clock, bad_id).await.is_err());
    }

    #[tokio::test]
    async fn saving_keeps_item_ids_and_their_tasks_and_drops_removed_items() {
        let f = setup();
        let m = create(
            &f.db,
            &f.clock,
            MeetingInput {
                action_items: vec![
                    action(Some("a-1"), "Book room", None),
                    action(Some("a-2"), "Send recap", None),
                ],
                decisions: vec![DecisionInput {
                    id: Some("d-1".into()),
                    body: "Go".into(),
                }],
                ..input("Review")
            },
        )
        .await
        .unwrap();
        let task = convert_action_item(&f.db, &f.clock, "a-1").await.unwrap();
        // Reorder, edit, drop one, add one.
        let saved = update(
            &f.db,
            &f.clock,
            &m.id,
            MeetingInput {
                action_items: vec![
                    action(Some("a-3"), "New item", None),
                    action(Some("a-1"), "Book the big room", Some("Ben")),
                ],
                decisions: vec![],
                ..input("Review")
            },
        )
        .await
        .unwrap();
        let ids: Vec<_> = saved.action_items.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["a-3", "a-1"]);
        assert_eq!(
            saved.action_items[1].task_id.as_deref(),
            Some(task.id.as_str())
        );
        assert_eq!(saved.action_items[1].task_status, Some(TaskStatus::Planned));
        assert!(saved.decisions.is_empty());
    }

    #[tokio::test]
    async fn a_save_that_changes_nothing_says_nothing() {
        let f = setup();
        let m = create(&f.db, &f.clock, input("Same")).await.unwrap();
        let mut events = f.bus.subscribe();
        f.clock.advance_ms(5_000);
        let again = update(
            &f.db,
            &f.clock,
            &m.id,
            MeetingInput {
                starts_at: Some(m.starts_at),
                ..input("Same")
            },
        )
        .await
        .unwrap();
        assert_eq!(again.updated_at, m.updated_at);
        let changed = update(&f.db, &f.clock, &m.id, input("Renamed"))
            .await
            .unwrap();
        assert_eq!(changed.updated_at, f.clock.now_ms());
        match events.recv().await {
            Recv::Event(Event::MeetingUpdated { meeting, .. }) => {
                assert_eq!(meeting.title, "Renamed")
            }
            other => panic!("expected MeetingUpdated, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn an_item_id_from_another_meeting_is_refused() {
        let f = setup();
        create(
            &f.db,
            &f.clock,
            MeetingInput {
                action_items: vec![action(Some("shared"), "x", None)],
                ..input("One")
            },
        )
        .await
        .unwrap();
        let two = create(&f.db, &f.clock, input("Two")).await.unwrap();
        let err = update(
            &f.db,
            &f.clock,
            &two.id,
            MeetingInput {
                action_items: vec![action(Some("shared"), "y", None)],
                ..input("Two")
            },
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, crate::error::ErrorCode::Conflict);
    }

    #[tokio::test]
    async fn converting_an_action_item_makes_a_linked_task_once() {
        let f = setup();
        let m = create(
            &f.db,
            &f.clock,
            MeetingInput {
                follow_up_date: NaiveDate::from_ymd_opt(2025, 10, 20),
                action_items: vec![action(Some("a"), "Draft the plan", Some("Asha"))],
                ..input("Kickoff")
            },
        )
        .await
        .unwrap();
        let task = convert_action_item(&f.db, &f.clock, "a").await.unwrap();
        assert_eq!(task.title, "Draft the plan");
        assert_eq!(task.project.as_deref(), Some("Kickoff"));
        assert_eq!(task.due_date, NaiveDate::from_ymd_opt(2025, 10, 20));
        assert_eq!(task.source_action_item_id.as_deref(), Some("a"));
        // FR-52: planned for today, so it appears on Today.
        assert_eq!(task.planned_date, Some(f.clock.today()));
        assert!(task.description.contains("Assignee: Asha"));
        assert!(task.description.contains("Kickoff"));
        let again = convert_action_item(&f.db, &f.clock, "a").await.unwrap_err();
        assert_eq!(again.code, crate::error::ErrorCode::Conflict);
        // The item shows the task's status as it changes.
        tasks::transition(&f.db, &f.clock, &task.id, TaskStatus::Completed)
            .await
            .unwrap();
        let now = get(&f.db, &m.id).await.unwrap();
        assert_eq!(now.action_items[0].task_status, Some(TaskStatus::Completed));
        assert_eq!(
            list(&f.db, MeetingFilters::default()).await.unwrap()[0].open_action_items,
            0
        );
        // Deleting the task unlinks the item; it can be converted again.
        tasks::delete(&f.db, &f.clock, &task.id).await.unwrap();
        assert_eq!(
            get(&f.db, &m.id).await.unwrap().action_items[0].task_id,
            None
        );
        assert!(convert_action_item(&f.db, &f.clock, "a").await.is_ok());
        assert!(convert_action_item(&f.db, &f.clock, "missing")
            .await
            .is_err());
    }

    #[tokio::test]
    async fn the_list_is_newest_first_and_filters_by_participant_and_time() {
        let f = setup();
        for (title, at, who) in [
            ("Old", 1_000, "Asha"),
            ("New", 3_000, "ben"),
            ("Mid", 2_000, "Ben"),
        ] {
            create(
                &f.db,
                &f.clock,
                MeetingInput {
                    starts_at: Some(at),
                    participants: vec![who.into()],
                    ..input(title)
                },
            )
            .await
            .unwrap();
        }
        let titles = |v: Vec<MeetingSummary>| v.into_iter().map(|m| m.title).collect::<Vec<_>>();
        assert_eq!(
            titles(list(&f.db, MeetingFilters::default()).await.unwrap()),
            ["New", "Mid", "Old"]
        );
        let ben = MeetingFilters {
            participant: Some("BEN".into()),
            ..Default::default()
        };
        assert_eq!(titles(list(&f.db, ben).await.unwrap()), ["New", "Mid"]);
        let range = MeetingFilters {
            from: Some(1_500),
            to: Some(3_000),
            ..Default::default()
        };
        assert_eq!(titles(list(&f.db, range).await.unwrap()), ["Mid"]);
    }

    #[tokio::test]
    async fn participant_suggestions_match_prefixes_most_used_first() {
        let f = setup();
        for who in [vec!["Asha", "Ben"], vec!["asha", "Bea"], vec!["Asha"]] {
            create(
                &f.db,
                &f.clock,
                MeetingInput {
                    participants: who.into_iter().map(Into::into).collect(),
                    ..input("m")
                },
            )
            .await
            .unwrap();
        }
        let a = suggest_participants(&f.db, "as").await.unwrap();
        assert_eq!(a.len(), 1);
        assert!(a[0].eq_ignore_ascii_case("asha"));
        assert_eq!(suggest_participants(&f.db, "B").await.unwrap().len(), 2);
        assert!(suggest_participants(&f.db, "%").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn deleting_a_meeting_keeps_its_tasks() {
        let f = setup();
        let m = create(
            &f.db,
            &f.clock,
            MeetingInput {
                action_items: vec![action(Some("a"), "Keep me", None)],
                ..input("Gone")
            },
        )
        .await
        .unwrap();
        let task = convert_action_item(&f.db, &f.clock, "a").await.unwrap();
        delete(&f.db, &f.clock, &m.id).await.unwrap();
        assert!(get(&f.db, &m.id).await.is_err());
        assert_eq!(tasks::get(&f.db, &task.id).await.unwrap().title, "Keep me");
        assert!(delete(&f.db, &f.clock, &m.id).await.is_err());
    }
}
