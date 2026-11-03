//! Tasks (PRD §6.2, R1-20 … R1-29): the model, the state machine, and the operations that
//! persist them together with their history.
//!
//! Status changes go through [`transition_rules`], a pure function checked against every cell of
//! the PRD §6.2.1 table. Every create, status change, defer and edit also writes one row to
//! `task_events` in the same transaction; that table is what the daily log is rebuilt from.

use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::clock::Clock;
use crate::db::Database;
use crate::error::{AppError, Result};
use crate::events::Event;
use crate::ids::new_id;
pub use crate::task_plans::{Repeat, TaskLink};

pub const TITLE_MAX: usize = 300;
/// Not fixed by the PRD; bounded so one task cannot make the list or the daily log heavy.
pub const DESCRIPTION_MAX: usize = 20_000;
pub const PROJECT_MAX: usize = 100;

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskStatus {
    Planned,
    InProgress,
    Pending,
    Completed,
    Cancelled,
}

impl TaskStatus {
    pub const ALL: [TaskStatus; 5] = [
        Self::Planned,
        Self::InProgress,
        Self::Pending,
        Self::Completed,
        Self::Cancelled,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "PLANNED",
            Self::InProgress => "IN_PROGRESS",
            Self::Pending => "PENDING",
            Self::Completed => "COMPLETED",
            Self::Cancelled => "CANCELLED",
        }
    }

    fn from_db(s: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|v| v.as_str() == s)
            .ok_or_else(|| AppError::internal("A task has a status Pluto doesn't know."))
    }

    /// PRD §6.2.1, authoritative. Completing straight from PLANNED or PENDING is allowed.
    pub fn can_move_to(self, to: Self) -> bool {
        use TaskStatus::*;
        matches!(
            (self, to),
            (Planned, InProgress | Completed | Cancelled)
                | (InProgress, Pending | Completed | Cancelled)
                | (Pending, InProgress | Completed | Cancelled)
                | (Completed | Cancelled, Planned)
        )
    }

    /// The buttons the UI should offer, in display order, each with its label. Invalid moves are
    /// never offered (R1-22).
    pub fn available_transitions(self) -> Vec<(TaskStatus, &'static str)> {
        Self::ALL
            .into_iter()
            .filter(|to| self.can_move_to(*to))
            .map(|to| (to, action_label(self, to)))
            .collect()
    }
}

fn action_label(from: TaskStatus, to: TaskStatus) -> &'static str {
    match (from, to) {
        (TaskStatus::Planned, TaskStatus::InProgress) => "Start",
        (TaskStatus::InProgress, TaskStatus::Pending) => "Pause",
        (TaskStatus::Pending, TaskStatus::InProgress) => "Resume",
        (_, TaskStatus::Completed) => "Complete",
        (_, TaskStatus::Cancelled) => "Cancel",
        (_, TaskStatus::Planned) => "Reopen",
        (_, TaskStatus::InProgress) => "Start",
        (_, TaskStatus::Pending) => "Pause",
    }
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Priority {
    Low,
    Medium,
    High,
}

impl Priority {
    fn as_str(self) -> &'static str {
        match self {
            Self::Low => "LOW",
            Self::Medium => "MEDIUM",
            Self::High => "HIGH",
        }
    }

    fn from_db(s: &str) -> Result<Self> {
        [Self::Low, Self::Medium, Self::High]
            .into_iter()
            .find(|p| p.as_str() == s)
            .ok_or_else(|| AppError::internal("A task has a priority Pluto doesn't know."))
    }
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub priority: Option<Priority>,
    pub project: Option<String>,
    pub planned_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    pub note_id: Option<String>,
    pub source_action_item_id: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub updated_at: i64,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub started_at: Option<i64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub completed_at: Option<i64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub cancelled_at: Option<i64>,
    /// Starred tasks come first (FR-107).
    pub starred: bool,
    /// How it repeats, counted from `planned_date` (FR-109). Each day is ticked on its own.
    pub repeat: Option<Repeat>,
    pub repeat_until: Option<NaiveDate>,
    /// The specific days it is planned on, when there is more than one (FR-108). Sorted.
    pub days: Vec<NaiveDate>,
    /// Links shown next to the task (FR-106), in order.
    pub links: Vec<TaskLink>,
}

impl Task {
    /// Overdue = due before today and not finished (R1-28).
    pub fn is_overdue(&self, today: NaiveDate) -> bool {
        self.due_date.is_some_and(|d| d < today)
            && !matches!(self.status, TaskStatus::Completed | TaskStatus::Cancelled)
    }

    #[cfg(test)]
    pub(crate) fn sample() -> Self {
        Self {
            id: "t1".into(),
            title: "Write TRD".into(),
            description: String::new(),
            status: TaskStatus::InProgress,
            priority: Some(Priority::High),
            project: Some("Pluto".into()),
            planned_date: NaiveDate::from_ymd_opt(2026, 10, 3),
            due_date: NaiveDate::from_ymd_opt(2026, 10, 4),
            note_id: None,
            source_action_item_id: None,
            created_at: 1,
            updated_at: 2,
            started_at: Some(2),
            completed_at: None,
            cancelled_at: None,
            starred: false,
            repeat: None,
            repeat_until: None,
            days: Vec::new(),
            links: Vec::new(),
        }
    }
}

// ---- the state machine (pure) --------------------------------------------------------------

/// The three lifecycle timestamps. `started_at` is set on the first move to IN_PROGRESS and never
/// cleared; `completed_at` / `cancelled_at` are set on entering those states and cleared on reopen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stamps {
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub cancelled_at: Option<i64>,
}

/// Apply a status change to the timestamps, or refuse it with `INVALID_TRANSITION`.
pub fn transition_rules(
    from: TaskStatus,
    to: TaskStatus,
    stamps: Stamps,
    now: i64,
) -> Result<Stamps> {
    if !from.can_move_to(to) {
        return Err(AppError::invalid_transition(format!(
            "A {} task can't go straight to {}.",
            from.as_str().to_lowercase().replace('_', " "),
            to.as_str().to_lowercase().replace('_', " "),
        )));
    }
    let mut next = stamps;
    match to {
        TaskStatus::InProgress => next.started_at = stamps.started_at.or(Some(now)),
        TaskStatus::Completed => next.completed_at = Some(now),
        TaskStatus::Cancelled => next.cancelled_at = Some(now),
        TaskStatus::Planned => {
            next.completed_at = None;
            next.cancelled_at = None;
        }
        TaskStatus::Pending => {}
    }
    Ok(next)
}

// ---- input ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct TaskInput {
    pub title: String,
    pub description: String,
    pub priority: Option<Priority>,
    pub project: Option<String>,
    pub planned_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    pub note_id: Option<String>,
}

/// `None` leaves a field alone. For the nullable ones, `Some(None)` clears it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TaskPatch {
    pub title: Option<String>,
    pub description: Option<String>,
    pub priority: Option<Option<Priority>>,
    pub project: Option<Option<String>>,
    pub planned_date: Option<Option<NaiveDate>>,
    pub due_date: Option<Option<NaiveDate>>,
    pub note_id: Option<Option<String>>,
}

/// What the task editors send: the whole editable state of a task, so a cleared field means
/// "clear it". Converted to a [`TaskPatch`] that sets every field. Status is not here; use
/// [`transition`]. An empty project becomes "no project".
#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, optional_fields = nullable))]
#[serde(default)]
pub struct TaskEdit {
    pub title: String,
    pub description: String,
    pub project: Option<String>,
    pub priority: Option<Priority>,
    pub planned_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    pub note_id: Option<String>,
}

impl From<TaskEdit> for TaskPatch {
    fn from(e: TaskEdit) -> Self {
        let project = e
            .project
            .map(|p| p.trim().to_owned())
            .filter(|p| !p.is_empty());
        Self {
            title: Some(e.title),
            description: Some(e.description),
            project: Some(project),
            priority: Some(e.priority),
            planned_date: Some(e.planned_date),
            due_date: Some(e.due_date),
            note_id: Some(e.note_id),
        }
    }
}

fn clean_title(title: &str) -> Result<String> {
    let title = title.trim();
    match title.chars().count() {
        0 => Err(AppError::validation("title", "Give the task a title.")),
        n if n > TITLE_MAX => Err(AppError::validation(
            "title",
            format!("Titles can be up to {TITLE_MAX} characters."),
        )),
        _ => Ok(title.to_owned()),
    }
}

fn clean_description(description: &str) -> Result<String> {
    if description.chars().count() > DESCRIPTION_MAX {
        return Err(AppError::validation(
            "description",
            format!("Descriptions can be up to {DESCRIPTION_MAX} characters."),
        ));
    }
    Ok(description.to_owned())
}

/// Trimmed; blank means "no project".
fn clean_project(project: Option<&str>) -> Result<Option<String>> {
    let Some(p) = project.map(str::trim).filter(|p| !p.is_empty()) else {
        return Ok(None);
    };
    if p.chars().count() > PROJECT_MAX {
        return Err(AppError::validation(
            "project",
            format!("Project names can be up to {PROJECT_MAX} characters."),
        ));
    }
    Ok(Some(p.to_owned()))
}

// ---- storage -------------------------------------------------------------------------------

const COLUMNS: &str = "id, title, description, status, priority, project, planned_date, due_date, note_id, source_action_item_id,
                       created_at, updated_at, started_at, completed_at, cancelled_at, starred, repeat_rule, repeat_until";

/// One `tasks` row as stored, before the text columns are parsed into types.
struct Raw {
    id: String,
    title: String,
    description: String,
    status: String,
    priority: Option<String>,
    project: Option<String>,
    planned: Option<String>,
    due: Option<String>,
    note_id: Option<String>,
    source: Option<String>,
    created_at: i64,
    updated_at: i64,
    started_at: Option<i64>,
    completed_at: Option<i64>,
    cancelled_at: Option<i64>,
    starred: bool,
    repeat: Option<String>,
    repeat_until: Option<String>,
}

fn read_raw(r: &rusqlite::Row) -> rusqlite::Result<Raw> {
    Ok(Raw {
        id: r.get(0)?,
        title: r.get(1)?,
        description: r.get(2)?,
        status: r.get(3)?,
        priority: r.get(4)?,
        project: r.get(5)?,
        planned: r.get(6)?,
        due: r.get(7)?,
        note_id: r.get(8)?,
        source: r.get(9)?,
        created_at: r.get(10)?,
        updated_at: r.get(11)?,
        started_at: r.get(12)?,
        completed_at: r.get(13)?,
        cancelled_at: r.get(14)?,
        starred: r.get(15)?,
        repeat: r.get(16)?,
        repeat_until: r.get(17)?,
    })
}

fn parse_date(s: Option<String>) -> Result<Option<NaiveDate>> {
    s.map(|s| {
        s.parse::<NaiveDate>()
            .map_err(|_| AppError::internal("A task has a date Pluto can't read."))
    })
    .transpose()
}

fn build(raw: Raw) -> Result<Task> {
    Ok(Task {
        id: raw.id,
        title: raw.title,
        description: raw.description,
        status: TaskStatus::from_db(&raw.status)?,
        priority: raw.priority.as_deref().map(Priority::from_db).transpose()?,
        project: raw.project,
        planned_date: parse_date(raw.planned)?,
        due_date: parse_date(raw.due)?,
        note_id: raw.note_id,
        source_action_item_id: raw.source,
        created_at: raw.created_at,
        updated_at: raw.updated_at,
        started_at: raw.started_at,
        completed_at: raw.completed_at,
        cancelled_at: raw.cancelled_at,
        starred: raw.starred,
        repeat: raw.repeat.as_deref().map(Repeat::from_db).transpose()?,
        repeat_until: parse_date(raw.repeat_until)?,
        days: Vec::new(),
        links: Vec::new(),
    })
}

/// A stored row plus its days and links.
fn build_full(conn: &Connection, raw: Raw) -> Result<Task> {
    let mut task = build(raw)?;
    crate::task_plans::fill(conn, &mut task)?;
    Ok(task)
}

pub(crate) fn load(conn: &Connection, id: &str) -> Result<Task> {
    let raw = conn
        .query_row(
            &format!("SELECT {COLUMNS} FROM tasks WHERE id = ?1"),
            [id],
            read_raw,
        )
        .optional()?;
    build_full(conn, raw.ok_or_else(not_found)?)
}

pub(crate) fn not_found() -> AppError {
    AppError::not_found("That task no longer exists.")
}

pub(crate) fn date_text(d: Option<NaiveDate>) -> Option<String> {
    d.map(|d| d.to_string())
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskEvent {
    pub kind: String,
    pub from_status: Option<String>,
    pub to_status: Option<String>,
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub at: i64,
    pub local_date: String,
}

/// One history row, written in the same transaction as the change. `local_date` is fixed at write
/// time so a later timezone change cannot move the entry to another day.
pub(crate) fn record(conn: &Connection, task_id: &str, e: &TaskEvent) -> Result<()> {
    conn.execute(
        "INSERT INTO task_events (task_id, kind, from_status, to_status, from_date, to_date, at, local_date)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![task_id, e.kind, e.from_status, e.to_status, e.from_date, e.to_date, e.at, e.local_date],
    )?;
    Ok(())
}

pub(crate) fn history_row(kind: &str, at: i64, local_date: &str) -> TaskEvent {
    TaskEvent {
        kind: kind.into(),
        from_status: None,
        to_status: None,
        from_date: None,
        to_date: None,
        at,
        local_date: local_date.into(),
    }
}

fn check_note_exists(conn: &Connection, note_id: &Option<String>) -> Result<()> {
    if let Some(id) = note_id {
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM notes WHERE id = ?1)",
            [id],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(AppError::not_found("That note no longer exists.").with_field("note_id"));
        }
    }
    Ok(())
}

pub async fn get(db: &Database, id: &str) -> Result<Task> {
    let id = id.to_owned();
    db.read(move |conn| load(conn, &id)).await
}

/// The history of one task, oldest first.
pub async fn history(db: &Database, id: &str) -> Result<Vec<TaskEvent>> {
    let id = id.to_owned();
    db.read(move |conn| {
        load(conn, &id)?; // NOT_FOUND for an unknown task rather than an empty list
        let mut stmt = conn.prepare("SELECT kind, from_status, to_status, from_date, to_date, at, local_date FROM task_events WHERE task_id = ?1 ORDER BY id")?;
        let rows = stmt
            .query_map([&id], |r| Ok(TaskEvent { kind: r.get(0)?, from_status: r.get(1)?, to_status: r.get(2)?, from_date: r.get(3)?, to_date: r.get(4)?, at: r.get(5)?, local_date: r.get(6)? }))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    })
    .await
}

pub async fn create(db: &Database, clock: &dyn Clock, input: TaskInput) -> Result<Task> {
    let (id, now) = (new_id(), clock.now_ms());
    let local = clock.local_date(now).to_string();
    let input = clean_input(input)?;
    db.write(move |tx| {
        let task = insert(tx, &id, now, &local, &input, None)?;
        Ok((task.clone(), vec![Event::TaskCreated { at: now, task }]))
    })
    .await
}

/// Validate and trim a new task's fields (outside the writer).
pub(crate) fn clean_input(input: TaskInput) -> Result<TaskInput> {
    Ok(TaskInput {
        title: clean_title(&input.title)?,
        description: clean_description(&input.description)?,
        project: clean_project(input.project.as_deref())?,
        ..input
    })
}

/// Insert a cleaned task inside a write transaction, with its CREATED history row. Meetings use
/// it to turn an action item into a task in the same transaction (`source_action_item_id`).
pub(crate) fn insert(
    tx: &Connection,
    id: &str,
    now: i64,
    local: &str,
    input: &TaskInput,
    source_action_item_id: Option<&str>,
) -> Result<Task> {
    check_note_exists(tx, &input.note_id)?;
    tx.execute(
        "INSERT INTO tasks (id, title, description, status, priority, project, planned_date, due_date, note_id, source_action_item_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, 'PLANNED', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
        params![id, input.title, input.description, input.priority.map(Priority::as_str), input.project, date_text(input.planned_date), date_text(input.due_date), input.note_id, source_action_item_id, now],
    )?;
    record(
        tx,
        id,
        &TaskEvent {
            to_status: Some("PLANNED".into()),
            to_date: date_text(input.planned_date),
            ..history_row("CREATED", now, local)
        },
    )?;
    load(tx, id)
}

/// The single-line quick-add (R1-21): a title, planned for today.
pub async fn quick_add(db: &Database, clock: &dyn Clock, title: &str) -> Result<Task> {
    create(
        db,
        clock,
        TaskInput {
            title: title.into(),
            planned_date: Some(clock.today()),
            ..Default::default()
        },
    )
    .await
}

/// Move a task along its lifecycle. Invalid moves are refused with `INVALID_TRANSITION` and change
/// nothing: no row, no history, no event.
pub async fn transition(
    db: &Database,
    clock: &dyn Clock,
    id: &str,
    to: TaskStatus,
) -> Result<Task> {
    let (id, now) = (id.to_owned(), clock.now_ms());
    let local = clock.local_date(now).to_string();
    db.write(move |tx| {
        let task = load(tx, &id)?;
        let stamps = transition_rules(task.status, to, Stamps { started_at: task.started_at, completed_at: task.completed_at, cancelled_at: task.cancelled_at }, now)?;
        tx.execute(
            "UPDATE tasks SET status = ?1, started_at = ?2, completed_at = ?3, cancelled_at = ?4, updated_at = ?5 WHERE id = ?6",
            params![to.as_str(), stamps.started_at, stamps.completed_at, stamps.cancelled_at, now, id],
        )?;
        record(tx, &id, &TaskEvent { from_status: Some(task.status.as_str().into()), to_status: Some(to.as_str().into()), ..history_row("STATUS", now, &local) })?;
        let updated = load(tx, &id)?;
        Ok((updated.clone(), vec![Event::TaskStatusChanged { at: now, id, from: task.status, to }, Event::TaskUpdated { at: now, task: updated }]))
    })
    .await
}

fn apply(task: &Task, patch: &TaskPatch) -> Result<Task> {
    let mut next = task.clone();
    if let Some(t) = &patch.title {
        next.title = clean_title(t)?;
    }
    if let Some(d) = &patch.description {
        next.description = clean_description(d)?;
    }
    if let Some(p) = patch.priority {
        next.priority = p;
    }
    if let Some(p) = &patch.project {
        next.project = clean_project(p.as_deref())?;
    }
    if let Some(d) = patch.planned_date {
        next.planned_date = d;
    }
    if let Some(d) = patch.due_date {
        next.due_date = d;
    }
    if let Some(n) = &patch.note_id {
        next.note_id = n.clone();
    }
    Ok(next)
}

/// Edit fields. A patch that changes nothing writes nothing and publishes nothing. Status is not
/// editable here; use [`transition`].
pub async fn update(db: &Database, clock: &dyn Clock, id: &str, patch: TaskPatch) -> Result<Task> {
    let current = get(db, id).await?;
    if apply(&current, &patch)? == current {
        return Ok(current);
    }
    let (id, now) = (id.to_owned(), clock.now_ms());
    let local = clock.local_date(now).to_string();
    db.write(move |tx| {
        let task = load(tx, &id)?;
        let next = apply(&task, &patch)?;
        if next == task {
            return Ok((task, vec![]));
        }
        check_note_exists(tx, &next.note_id)?;
        tx.execute(
            "UPDATE tasks SET title = ?1, description = ?2, priority = ?3, project = ?4, planned_date = ?5, due_date = ?6, note_id = ?7, updated_at = ?8 WHERE id = ?9",
            params![next.title, next.description, next.priority.map(Priority::as_str), next.project, date_text(next.planned_date), date_text(next.due_date), next.note_id, now, id],
        )?;
        let dates_moved = next.planned_date != task.planned_date;
        record(
            tx,
            &id,
            &TaskEvent {
                from_date: dates_moved.then(|| date_text(task.planned_date)).flatten(),
                to_date: dates_moved.then(|| date_text(next.planned_date)).flatten(),
                ..history_row("EDITED", now, &local)
            },
        )?;
        let updated = load(tx, &id)?;
        Ok((updated.clone(), vec![Event::TaskUpdated { at: now, task: updated }]))
    })
    .await
}

/// Push the planned date out (R1-24). Status is unchanged. The new date must be later than the
/// current planned date (or than today if there is none), and a finished task can't be deferred.
pub async fn defer(db: &Database, clock: &dyn Clock, id: &str, to_date: NaiveDate) -> Result<Task> {
    let (id, now, today) = (id.to_owned(), clock.now_ms(), clock.today());
    let local = clock.local_date(now).to_string();
    db.write(move |tx| {
        let task = load(tx, &id)?;
        if matches!(task.status, TaskStatus::Completed | TaskStatus::Cancelled) {
            return Err(AppError::invalid_transition(
                "Reopen this task before deferring it.",
            ));
        }
        if to_date <= task.planned_date.unwrap_or(today) {
            return Err(AppError::validation(
                "to_date",
                "Pick a later date than the one it's planned for.",
            ));
        }
        tx.execute(
            "UPDATE tasks SET planned_date = ?1, updated_at = ?2 WHERE id = ?3",
            params![to_date.to_string(), now, id],
        )?;
        record(
            tx,
            &id,
            &TaskEvent {
                from_date: date_text(task.planned_date),
                to_date: Some(to_date.to_string()),
                ..history_row("DEFERRED", now, &local)
            },
        )?;
        let updated = load(tx, &id)?;
        Ok((
            updated.clone(),
            vec![
                Event::TaskDeferred {
                    at: now,
                    id,
                    from_date: date_text(task.planned_date),
                    to_date: to_date.to_string(),
                },
                Event::TaskUpdated {
                    at: now,
                    task: updated,
                },
            ],
        ))
    })
    .await
}

/// Delete a finished task (R1-29). Active work can't be deleted by accident: complete or cancel
/// it first. Its history goes with it; frozen daily logs are unaffected because they copy titles.
pub async fn delete(db: &Database, clock: &dyn Clock, id: &str) -> Result<()> {
    let (id, now) = (id.to_owned(), clock.now_ms());
    db.write(move |tx| {
        let task = load(tx, &id)?;
        if !matches!(task.status, TaskStatus::Completed | TaskStatus::Cancelled) {
            return Err(AppError::invalid_transition(
                "Complete or cancel this task before deleting it.",
            ));
        }
        tx.execute("DELETE FROM tasks WHERE id = ?1", [&id])?;
        Ok(((), vec![Event::TaskDeleted { at: now, id }]))
    })
    .await
}

// ---- work updates (R1-25) ------------------------------------------------------------------

pub const WORK_UPDATE_MAX: usize = 5_000;
/// A work update can be deleted for this long after it was written; after that it is permanent.
pub const WORK_UPDATE_DELETE_WINDOW_MS: i64 = 5 * 60 * 1_000;

/// A short, timestamped progress note on a task. Append-only: there is no edit.
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkUpdate {
    pub id: String,
    pub task_id: String,
    pub body: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
}

pub async fn add_work_update(
    db: &Database,
    clock: &dyn Clock,
    task_id: &str,
    body: &str,
) -> Result<WorkUpdate> {
    let body = body.trim();
    match body.chars().count() {
        0 => return Err(AppError::validation("body", "Write a short update first.")),
        n if n > WORK_UPDATE_MAX => {
            return Err(AppError::validation(
                "body",
                format!("Updates can be up to {WORK_UPDATE_MAX} characters."),
            ))
        }
        _ => {}
    }
    let (task_id, body, now) = (task_id.to_owned(), body.to_owned(), clock.now_ms());
    db.write(move |tx| {
        let task = load(tx, &task_id)?;
        let update = WorkUpdate {
            id: new_id(),
            task_id: task_id.clone(),
            body,
            created_at: now,
        };
        tx.execute(
            "INSERT INTO task_work_updates (id, task_id, body, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![update.id, update.task_id, update.body, update.created_at],
        )?;
        Ok((update, vec![Event::TaskUpdated { at: now, task }]))
    })
    .await
}

/// Newest first.
pub async fn work_updates(db: &Database, task_id: &str) -> Result<Vec<WorkUpdate>> {
    let task_id = task_id.to_owned();
    db.read(move |conn| {
        load(conn, &task_id)?;
        let mut stmt = conn.prepare(
            "SELECT id, task_id, body, created_at FROM task_work_updates WHERE task_id = ?1 ORDER BY created_at DESC, id DESC",
        )?;
        let rows = stmt
            .query_map([&task_id], |r| Ok(WorkUpdate { id: r.get(0)?, task_id: r.get(1)?, body: r.get(2)?, created_at: r.get(3)? }))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    })
    .await
}

/// Only within [`WORK_UPDATE_DELETE_WINDOW_MS`] of writing it.
pub async fn delete_work_update(db: &Database, clock: &dyn Clock, id: &str) -> Result<()> {
    let (id, now) = (id.to_owned(), clock.now_ms());
    db.write(move |tx| {
        let found: Option<(String, i64)> = tx
            .query_row(
                "SELECT task_id, created_at FROM task_work_updates WHERE id = ?1",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((task_id, created_at)) = found else {
            return Err(AppError::not_found("That update no longer exists."));
        };
        if now - created_at > WORK_UPDATE_DELETE_WINDOW_MS {
            return Err(AppError::conflict(
                "Updates can only be deleted in the first 5 minutes.",
            ));
        }
        tx.execute("DELETE FROM task_work_updates WHERE id = ?1", [&id])?;
        let task = load(tx, &task_id)?;
        Ok(((), vec![Event::TaskUpdated { at: now, task }]))
    })
    .await
}

// ---- views (R1-27) -------------------------------------------------------------------------

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskView {
    /// Open tasks planned today, plus anything overdue, plus anything in progress.
    Today,
    /// Open tasks planned for tomorrow through the next 7 days.
    Upcoming,
    Pending,
    /// Every task, finished ones last.
    All,
    /// Completed tasks, newest first (the history).
    Completed,
}

/// `priority` and `project` narrow every view. `status` and the date range apply where they make
/// sense: `status` and a *planned*-date range in `All`, a *completion*-date range in `Completed`.
/// Dates are inclusive local days.
#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, optional_fields = nullable))]
#[serde(default)]
pub struct TaskFilters {
    pub status: Option<TaskStatus>,
    pub priority: Option<Priority>,
    pub project: Option<String>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    /// Only tasks linked to this note (FR-42: "Linked tasks" in the note).
    pub note_id: Option<String>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskRow {
    pub task: Task,
    pub overdue: bool,
    /// A repeating task already ticked for today (FR-109).
    pub done_today: bool,
}

const PRIORITY_RANK: &str =
    "CASE priority WHEN 'HIGH' THEN 0 WHEN 'MEDIUM' THEN 1 WHEN 'LOW' THEN 2 ELSE 3 END";
const OPEN: &str = "status NOT IN ('COMPLETED', 'CANCELLED')";
/// Planned on today through its chosen days (FR-108); the `?` is today.
const ON_DAY: &str = "EXISTS (SELECT 1 FROM task_days d WHERE d.task_id = tasks.id AND d.day = ?)";

pub async fn query(
    db: &Database,
    clock: &dyn Clock,
    view: TaskView,
    filters: TaskFilters,
) -> Result<Vec<TaskRow>> {
    use rusqlite::types::Value;

    let today = clock.today();
    let (today_text, horizon) = (
        today.to_string(),
        (today + chrono::Days::new(7)).to_string(),
    );
    // Completion times are instants, so the inclusive local-day range becomes [start of `from`, start of the day after `to`).
    let done_from = filters
        .from
        .map(|d| crate::clock::start_of_day_ms(clock, d));
    let done_before = filters
        .to
        .map(|d| crate::clock::start_of_day_ms(clock, d.succ_opt().unwrap_or(d)));

    db.read(move |conn| {
        let mut clauses: Vec<String> = Vec::new();
        let mut args: Vec<Value> = Vec::new();
        let mut order_args: Vec<Value> = Vec::new();

        let order = match view {
            TaskView::Today => {
                // Repeating tasks are all fetched and kept below only on their days (FR-109).
                clauses.push(format!("{OPEN} AND (planned_date = ? OR (due_date IS NOT NULL AND due_date < ?) OR status = 'IN_PROGRESS' OR {ON_DAY} OR repeat_rule IS NOT NULL)"));
                args.extend([today_text.clone().into(), today_text.clone().into(), today_text.clone().into()]);
                order_args.push(today_text.clone().into());
                format!("starred DESC, CASE WHEN status = 'IN_PROGRESS' THEN 0 WHEN due_date IS NOT NULL AND due_date < ? THEN 1 ELSE 2 END, {PRIORITY_RANK}, due_date IS NULL, due_date, created_at")
            }
            TaskView::Upcoming => {
                clauses.push(format!("{OPEN} AND ((planned_date > ? AND planned_date <= ?) OR EXISTS (SELECT 1 FROM task_days d WHERE d.task_id = tasks.id AND d.day > ? AND d.day <= ?))"));
                args.extend([today_text.clone().into(), horizon.clone().into(), today_text.clone().into(), horizon.clone().into()]);
                format!("starred DESC, planned_date, {PRIORITY_RANK}, created_at")
            }
            TaskView::Pending => {
                clauses.push("status = 'PENDING'".into());
                "updated_at DESC, created_at DESC".to_owned()
            }
            TaskView::Completed => {
                clauses.push("status = 'COMPLETED'".into());
                if let Some(from) = done_from {
                    clauses.push("completed_at >= ?".into());
                    args.push(from.into());
                }
                if let Some(before) = done_before {
                    clauses.push("completed_at < ?".into());
                    args.push(before.into());
                }
                "completed_at DESC".to_owned()
            }
            TaskView::All => {
                if let Some(status) = filters.status {
                    clauses.push("status = ?".into());
                    args.push(status.as_str().to_owned().into());
                }
                if let Some(from) = filters.from {
                    clauses.push("planned_date >= ?".into());
                    args.push(from.to_string().into());
                }
                if let Some(to) = filters.to {
                    clauses.push("planned_date <= ?".into());
                    args.push(to.to_string().into());
                }
                format!("CASE WHEN status IN ('COMPLETED', 'CANCELLED') THEN 1 ELSE 0 END, starred DESC, {PRIORITY_RANK}, planned_date IS NULL, planned_date, created_at DESC")
            }
        };
        if let Some(priority) = filters.priority {
            clauses.push("priority = ?".into());
            args.push(priority.as_str().to_owned().into());
        }
        if let Some(project) = &filters.project {
            clauses.push("lower(project) = lower(?)".into());
            args.push(project.trim().to_owned().into());
        }
        if let Some(note_id) = &filters.note_id {
            clauses.push("note_id = ?".into());
            args.push(note_id.clone().into());
        }
        args.extend(order_args);

        // Tasks in the Bin (FR-46) are in no view.
        clauses.push("binned_at IS NULL".into());
        let where_sql = format!("WHERE {}", clauses.join(" AND "));
        // Every fragment above is a constant or a `?` placeholder; user values only ever travel in `args`.
        let sql = format!("SELECT {COLUMNS} FROM tasks {where_sql} ORDER BY {order}");
        let mut stmt = conn.prepare(&sql)?;
        let raws = stmt.query_map(rusqlite::params_from_iter(args), read_raw)?.collect::<rusqlite::Result<Vec<_>>>()?;
        let mut rows = Vec::with_capacity(raws.len());
        for raw in raws {
            let task = build_full(conn, raw)?;
            let done_today = task.repeat.is_some() && crate::task_plans::ticked(conn, &task.id, today)?;
            // On Today a repeating task shows only on its days, until it is ticked.
            if view == TaskView::Today && task.repeat.is_some() && (done_today || !crate::task_plans::occurs(&task, today)) {
                continue;
            }
            let overdue = task.repeat.is_none() && task.is_overdue(today);
            rows.push(TaskRow { task, overdue, done_today });
        }
        Ok(rows)
    })
    .await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono_tz::Tz;

    use super::*;
    use crate::bus::{EventBus, Recv, Subscriber};
    use crate::clock::FakeClock;
    use crate::error::ErrorCode;
    use TaskStatus::*;

    // ---- the state machine, against the PRD table ------------------------------------------

    /// PRD §6.2.1 verbatim. Rows are "from", columns are "to", in this order:
    /// PLANNED, IN_PROGRESS, PENDING, COMPLETED, CANCELLED.  `.` = not allowed, `x` = allowed.
    const PRD_TABLE: [(TaskStatus, &str); 5] = [
        (Planned, ". x . x x"),
        (InProgress, ". . x x x"),
        (Pending, ". x . x x"),
        (Completed, "x . . . ."),
        (Cancelled, "x . . . ."),
    ];

    #[test]
    fn every_one_of_the_25_cells_matches_the_prd_table() {
        for (from, row) in PRD_TABLE {
            let cells: Vec<bool> = row.split(' ').map(|c| c == "x").collect();
            for (to, allowed) in TaskStatus::ALL.into_iter().zip(cells) {
                assert_eq!(from.can_move_to(to), allowed, "{from:?} -> {to:?}");
            }
        }
    }

    #[test]
    fn no_state_moves_to_itself() {
        for s in TaskStatus::ALL {
            assert!(!s.can_move_to(s), "{s:?}");
        }
    }

    #[test]
    fn the_ui_is_offered_exactly_the_valid_moves_with_their_labels() {
        assert_eq!(
            Planned.available_transitions(),
            [
                (InProgress, "Start"),
                (Completed, "Complete"),
                (Cancelled, "Cancel")
            ]
        );
        assert_eq!(
            InProgress.available_transitions(),
            [
                (Pending, "Pause"),
                (Completed, "Complete"),
                (Cancelled, "Cancel")
            ]
        );
        assert_eq!(
            Pending.available_transitions(),
            [
                (InProgress, "Resume"),
                (Completed, "Complete"),
                (Cancelled, "Cancel")
            ]
        );
        assert_eq!(Completed.available_transitions(), [(Planned, "Reopen")]);
        assert_eq!(Cancelled.available_transitions(), [(Planned, "Reopen")]);
    }

    #[test]
    fn the_offered_moves_and_the_rules_never_disagree() {
        for from in TaskStatus::ALL {
            for to in TaskStatus::ALL {
                let offered = from.available_transitions().iter().any(|(t, _)| *t == to);
                assert_eq!(
                    offered,
                    transition_rules(from, to, Stamps::default(), 0).is_ok(),
                    "{from:?} -> {to:?}"
                );
            }
        }
    }

    #[test]
    fn refused_moves_say_why_in_plain_words() {
        let err = transition_rules(Planned, Pending, Stamps::default(), 0).unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidTransition);
        assert_eq!(err.message, "A planned task can't go straight to pending.");
        let err = transition_rules(InProgress, Planned, Stamps::default(), 0).unwrap_err();
        assert_eq!(
            err.message,
            "A in progress task can't go straight to planned."
        );
    }

    // ---- timestamps ------------------------------------------------------------------------

    fn walk(steps: &[TaskStatus], start: TaskStatus) -> (TaskStatus, Stamps) {
        let (mut status, mut stamps) = (start, Stamps::default());
        for (i, to) in steps.iter().enumerate() {
            stamps = transition_rules(status, *to, stamps, 100 + i as i64).unwrap();
            status = *to;
        }
        (status, stamps)
    }

    #[test]
    fn started_at_is_set_once_and_survives_pause_resume_and_reopen() {
        let (_, s) = walk(
            &[
                InProgress, Pending, InProgress, Completed, Planned, InProgress,
            ],
            Planned,
        );
        assert_eq!(
            s.started_at,
            Some(100),
            "the first start, never overwritten"
        );
    }

    #[test]
    fn completing_without_starting_leaves_started_at_empty() {
        let (status, s) = walk(&[Completed], Planned);
        assert_eq!(
            (status, s.started_at, s.completed_at),
            (Completed, None, Some(100))
        );
    }

    #[test]
    fn reopening_clears_completed_and_cancelled_but_not_started() {
        let (_, s) = walk(&[InProgress, Completed, Planned], Planned);
        assert_eq!(
            (s.started_at, s.completed_at, s.cancelled_at),
            (Some(100), None, None)
        );
        let (_, s) = walk(&[InProgress, Cancelled, Planned], Planned);
        assert_eq!(
            (s.started_at, s.completed_at, s.cancelled_at),
            (Some(100), None, None)
        );
    }

    /// A tiny deterministic generator, so a failing sequence can be replayed exactly.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self, bound: usize) -> usize {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((self.0 >> 33) as usize) % bound
        }
    }

    #[test]
    fn over_20000_random_moves_the_timestamp_invariants_always_hold_and_refusals_change_nothing() {
        let mut rng = Lcg(42);
        let (mut status, mut stamps) = (Planned, Stamps::default());
        let mut first_started: Option<i64> = None;
        for step in 0..20_000 {
            let to = TaskStatus::ALL[rng.next(5)];
            let before = (status, stamps);
            match transition_rules(status, to, stamps, step) {
                Ok(next) => {
                    status = to;
                    stamps = next;
                }
                Err(_) => assert_eq!(
                    (status, stamps),
                    before,
                    "a refused move must change nothing"
                ),
            }
            assert_eq!(
                stamps.completed_at.is_some(),
                status == Completed,
                "step {step}: {status:?} {stamps:?}"
            );
            assert_eq!(
                stamps.cancelled_at.is_some(),
                status == Cancelled,
                "step {step}: {status:?} {stamps:?}"
            );
            if let Some(started) = stamps.started_at {
                assert_eq!(
                    *first_started.get_or_insert(started),
                    started,
                    "started_at must never change once set"
                );
            } else {
                assert!(first_started.is_none(), "started_at must never be cleared");
            }
            if status == InProgress {
                assert!(stamps.started_at.is_some());
            }
        }
    }

    // ---- persistence -----------------------------------------------------------------------

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
        let noon = chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2026, 10, 3, 12, 0, 0)
            .unwrap()
            .timestamp_millis();
        Fixture {
            _dir: dir,
            bus,
            db,
            clock: FakeClock::new(noon, Tz::UTC),
        }
    }

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn titled(title: &str) -> TaskInput {
        TaskInput {
            title: title.into(),
            ..Default::default()
        }
    }

    async fn next(sub: &mut Subscriber) -> Event {
        match sub.recv().await {
            Recv::Event(e) => e,
            other => panic!("expected an event, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_new_task_is_planned_with_history_and_an_event() {
        let f = setup();
        let mut sub = f.bus.subscribe();
        let task = create(
            &f.db,
            &f.clock,
            TaskInput {
                description: "Cover events and IPC".into(),
                priority: Some(Priority::High),
                project: Some("  Pluto ".into()),
                planned_date: Some(d(2026, 10, 3)),
                due_date: Some(d(2026, 10, 5)),
                ..titled("  Write TRD ")
            },
        )
        .await
        .unwrap();
        assert_eq!(
            (task.title.as_str(), task.status),
            ("Write TRD", Planned),
            "title is trimmed; status is PLANNED"
        );
        assert_eq!(
            (task.project.as_deref(), task.priority),
            (Some("Pluto"), Some(Priority::High))
        );
        assert_eq!(
            (task.planned_date, task.due_date),
            (Some(d(2026, 10, 3)), Some(d(2026, 10, 5)))
        );
        assert_eq!(
            (task.started_at, task.completed_at, task.cancelled_at),
            (None, None, None)
        );
        assert!(matches!(next(&mut sub).await, Event::TaskCreated { task: t, .. } if t == task));
        let h = history(&f.db, &task.id).await.unwrap();
        assert_eq!(h.len(), 1);
        assert_eq!(
            (
                h[0].kind.as_str(),
                h[0].to_status.as_deref(),
                h[0].to_date.as_deref()
            ),
            ("CREATED", Some("PLANNED"), Some("2026-10-03"))
        );
    }

    #[tokio::test]
    async fn quick_add_plans_the_task_for_today() {
        let f = setup();
        let task = quick_add(&f.db, &f.clock, "Reply to team email")
            .await
            .unwrap();
        assert_eq!(task.planned_date, Some(f.clock.today()));
        assert_eq!(task.due_date, None);
    }

    #[tokio::test]
    async fn titles_descriptions_and_projects_are_validated_and_name_their_field() {
        let f = setup();
        for (input, field) in [
            (titled("   "), "title"),
            (titled(&"x".repeat(301)), "title"),
            (
                TaskInput {
                    description: "x".repeat(DESCRIPTION_MAX + 1),
                    ..titled("ok")
                },
                "description",
            ),
            (
                TaskInput {
                    project: Some("p".repeat(PROJECT_MAX + 1)),
                    ..titled("ok")
                },
                "project",
            ),
        ] {
            let err = create(&f.db, &f.clock, input).await.unwrap_err();
            assert_eq!(
                (err.code, err.field.as_deref()),
                (ErrorCode::Validation, Some(field))
            );
        }
        assert!(
            create(&f.db, &f.clock, titled(&"é".repeat(300)))
                .await
                .is_ok(),
            "300 characters is fine"
        );
        assert!(
            create(
                &f.db,
                &f.clock,
                TaskInput {
                    project: Some("   ".into()),
                    ..titled("blank project")
                }
            )
            .await
            .unwrap()
            .project
            .is_none(),
            "blank means none"
        );
    }

    #[tokio::test]
    async fn linking_a_note_that_does_not_exist_is_refused() {
        let f = setup();
        let err = create(
            &f.db,
            &f.clock,
            TaskInput {
                note_id: Some("nope".into()),
                ..titled("x")
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
    async fn walking_a_task_through_its_life_records_every_step_and_publishes_it() {
        let f = setup();
        let task = quick_add(&f.db, &f.clock, "Ship it").await.unwrap();
        let mut sub = f.bus.subscribe();

        f.clock.advance_ms(1_000);
        let started = transition(&f.db, &f.clock, &task.id, InProgress)
            .await
            .unwrap();
        assert_eq!(
            (started.status, started.started_at),
            (InProgress, Some(f.clock.now_ms()))
        );
        assert_eq!(
            next(&mut sub).await,
            Event::TaskStatusChanged {
                at: f.clock.now_ms(),
                id: task.id.clone(),
                from: Planned,
                to: InProgress
            }
        );
        assert!(matches!(next(&mut sub).await, Event::TaskUpdated { .. }));

        f.clock.advance_ms(1_000);
        transition(&f.db, &f.clock, &task.id, Pending)
            .await
            .unwrap();
        f.clock.advance_ms(1_000);
        transition(&f.db, &f.clock, &task.id, InProgress)
            .await
            .unwrap();
        f.clock.advance_ms(1_000);
        let done = transition(&f.db, &f.clock, &task.id, Completed)
            .await
            .unwrap();
        assert_eq!(
            (done.status, done.completed_at),
            (Completed, Some(f.clock.now_ms()))
        );
        assert_eq!(
            done.started_at, started.started_at,
            "a resume does not restart the clock"
        );

        f.clock.advance_ms(1_000);
        let reopened = transition(&f.db, &f.clock, &task.id, Planned)
            .await
            .unwrap();
        assert_eq!(
            (reopened.status, reopened.completed_at, reopened.started_at),
            (Planned, None, started.started_at)
        );

        let moves: Vec<_> = history(&f.db, &task.id)
            .await
            .unwrap()
            .into_iter()
            .map(|e| (e.kind, e.from_status, e.to_status))
            .collect();
        assert_eq!(
            moves
                .iter()
                .map(|(k, f, t)| format!(
                    "{k}:{}>{}",
                    f.as_deref().unwrap_or("-"),
                    t.as_deref().unwrap_or("-")
                ))
                .collect::<Vec<_>>(),
            [
                "CREATED:->PLANNED",
                "STATUS:PLANNED>IN_PROGRESS",
                "STATUS:IN_PROGRESS>PENDING",
                "STATUS:PENDING>IN_PROGRESS",
                "STATUS:IN_PROGRESS>COMPLETED",
                "STATUS:COMPLETED>PLANNED"
            ]
        );
    }

    #[tokio::test]
    async fn a_refused_move_leaves_no_trace_anywhere() {
        let f = setup();
        let task = quick_add(&f.db, &f.clock, "x").await.unwrap();
        let mut sub = f.bus.subscribe();
        let commits = f.db.committed_transactions();
        let err = transition(&f.db, &f.clock, &task.id, Pending)
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidTransition);
        assert_eq!(get(&f.db, &task.id).await.unwrap(), task, "task unchanged");
        assert_eq!(
            history(&f.db, &task.id).await.unwrap().len(),
            1,
            "no history row"
        );
        assert_eq!(f.db.committed_transactions(), commits, "nothing committed");
        f.bus.publish(Event::AppShuttingDown { at: 0 });
        assert!(
            matches!(next(&mut sub).await, Event::AppShuttingDown { .. }),
            "no event"
        );
    }

    #[tokio::test]
    async fn completing_a_task_straight_from_planned_is_allowed() {
        let f = setup();
        let task = quick_add(&f.db, &f.clock, "tiny job").await.unwrap();
        let done = transition(&f.db, &f.clock, &task.id, Completed)
            .await
            .unwrap();
        assert_eq!(
            (
                done.status,
                done.started_at.is_none(),
                done.completed_at.is_some()
            ),
            (Completed, true, true)
        );
    }

    #[tokio::test]
    async fn a_random_walk_through_the_real_database_never_violates_a_constraint() {
        let f = setup();
        let task = quick_add(&f.db, &f.clock, "walker").await.unwrap();
        let mut rng = Lcg(7);
        let mut applied = 0;
        for _ in 0..300 {
            f.clock.advance_ms(10);
            if transition(&f.db, &f.clock, &task.id, TaskStatus::ALL[rng.next(5)])
                .await
                .is_ok()
            {
                applied += 1;
            }
        }
        let t = get(&f.db, &task.id).await.unwrap();
        assert_eq!(t.completed_at.is_some(), t.status == Completed);
        assert_eq!(
            history(&f.db, &task.id).await.unwrap().len(),
            applied + 1,
            "one history row per applied move, plus CREATED"
        );
    }

    #[tokio::test]
    async fn history_records_the_local_day_at_write_time_not_the_current_one() {
        let f = setup();
        f.clock.set_tz(chrono_tz::America::Los_Angeles);
        // 12:00 UTC is 05:00 the same day in Los Angeles; step to 02:00 UTC next day = 19:00 on the 3rd in LA.
        f.clock.set_now_ms(
            chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2026, 10, 4, 2, 0, 0)
                .unwrap()
                .timestamp_millis(),
        );
        let task = quick_add(&f.db, &f.clock, "late").await.unwrap();
        f.clock.set_tz(chrono_tz::Asia::Tokyo); // the user travels; "today" is now the 4th
        assert_eq!(
            history(&f.db, &task.id).await.unwrap()[0].local_date,
            "2026-10-03",
            "history stays on the day it happened"
        );
    }

    // ---- editing, deferring, deleting --------------------------------------------------------

    #[tokio::test]
    async fn editing_changes_only_what_was_given_and_can_clear_optional_fields() {
        let f = setup();
        let task = create(
            &f.db,
            &f.clock,
            TaskInput {
                priority: Some(Priority::Low),
                project: Some("Pluto".into()),
                due_date: Some(d(2026, 10, 9)),
                ..titled("old")
            },
        )
        .await
        .unwrap();
        f.clock.advance_ms(5);
        let edited = update(
            &f.db,
            &f.clock,
            &task.id,
            TaskPatch {
                title: Some("new".into()),
                priority: Some(None),
                due_date: Some(None),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(
            (edited.title.as_str(), edited.priority, edited.due_date),
            ("new", None, None)
        );
        assert_eq!(edited.project.as_deref(), Some("Pluto"), "untouched");
        assert_eq!(edited.updated_at, f.clock.now_ms());
        assert_eq!(
            history(&f.db, &task.id).await.unwrap().last().unwrap().kind,
            "EDITED"
        );
    }

    #[tokio::test]
    async fn the_today_editor_sets_and_clears_priority_and_deadline() {
        let f = setup();
        let task = create(&f.db, &f.clock, titled("plan")).await.unwrap();
        f.clock.advance_ms(5);
        let set = update(
            &f.db,
            &f.clock,
            &task.id,
            TaskEdit {
                title: "plan it".into(),
                priority: Some(Priority::High),
                due_date: Some(d(2026, 10, 9)),
                ..Default::default()
            }
            .into(),
        )
        .await
        .unwrap();
        assert_eq!(
            (set.title.as_str(), set.priority, set.due_date),
            ("plan it", Some(Priority::High), Some(d(2026, 10, 9)))
        );
        let cleared = update(
            &f.db,
            &f.clock,
            &task.id,
            TaskEdit {
                title: "plan it".into(),
                ..Default::default()
            }
            .into(),
        )
        .await
        .unwrap();
        assert_eq!((cleared.priority, cleared.due_date), (None, None));
    }

    #[tokio::test]
    async fn work_updates_are_newest_first_and_deletable_for_five_minutes() {
        let f = setup();
        let task = create(&f.db, &f.clock, titled("ship")).await.unwrap();
        let first = add_work_update(&f.db, &f.clock, &task.id, "  started  ")
            .await
            .unwrap();
        assert_eq!(first.body, "started", "trimmed");
        f.clock.advance_ms(60_000);
        let second = add_work_update(&f.db, &f.clock, &task.id, "half way")
            .await
            .unwrap();
        let listed = work_updates(&f.db, &task.id).await.unwrap();
        assert_eq!(
            listed.iter().map(|u| u.body.as_str()).collect::<Vec<_>>(),
            ["half way", "started"]
        );

        // Four minutes after the first one: still deletable.
        f.clock.advance_ms(3 * 60_000);
        delete_work_update(&f.db, &f.clock, &first.id)
            .await
            .unwrap();
        // Six minutes after the second: too late.
        f.clock.advance_ms(5 * 60_000 + 1);
        let late = delete_work_update(&f.db, &f.clock, &second.id)
            .await
            .unwrap_err();
        assert_eq!(late.code, ErrorCode::Conflict);
        assert_eq!(work_updates(&f.db, &task.id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn work_updates_reject_empty_long_and_unknown() {
        let f = setup();
        let task = create(&f.db, &f.clock, titled("x")).await.unwrap();
        assert_eq!(
            add_work_update(&f.db, &f.clock, &task.id, "   ")
                .await
                .unwrap_err()
                .code,
            ErrorCode::Validation
        );
        let long = "a".repeat(WORK_UPDATE_MAX + 1);
        assert_eq!(
            add_work_update(&f.db, &f.clock, &task.id, &long)
                .await
                .unwrap_err()
                .code,
            ErrorCode::Validation
        );
        assert_eq!(
            add_work_update(&f.db, &f.clock, "nope", "hi")
                .await
                .unwrap_err()
                .code,
            ErrorCode::NotFound
        );
        assert_eq!(
            delete_work_update(&f.db, &f.clock, "nope")
                .await
                .unwrap_err()
                .code,
            ErrorCode::NotFound
        );
    }

    #[tokio::test]
    async fn work_updates_go_with_the_task() {
        let f = setup();
        let task = create(&f.db, &f.clock, titled("done soon")).await.unwrap();
        add_work_update(&f.db, &f.clock, &task.id, "note")
            .await
            .unwrap();
        transition(&f.db, &f.clock, &task.id, Completed)
            .await
            .unwrap();
        delete(&f.db, &f.clock, &task.id).await.unwrap();
        assert_eq!(
            work_updates(&f.db, &task.id).await.unwrap_err().code,
            ErrorCode::NotFound
        );
    }

    #[tokio::test]
    async fn the_editor_can_set_description_project_and_clear_them() {
        let f = setup();
        let task = create(&f.db, &f.clock, titled("plan")).await.unwrap();
        let set = update(
            &f.db,
            &f.clock,
            &task.id,
            TaskEdit {
                title: "plan".into(),
                description: "details".into(),
                project: Some("  Pluto ".into()),
                ..Default::default()
            }
            .into(),
        )
        .await
        .unwrap();
        assert_eq!(
            (set.description.as_str(), set.project.as_deref()),
            ("details", Some("Pluto"))
        );
        let cleared = update(
            &f.db,
            &f.clock,
            &task.id,
            TaskEdit {
                title: "plan".into(),
                project: Some("   ".into()),
                ..Default::default()
            }
            .into(),
        )
        .await
        .unwrap();
        assert_eq!((cleared.description.as_str(), cleared.project), ("", None));
    }

    #[tokio::test]
    async fn an_edit_that_changes_nothing_writes_nothing() {
        let f = setup();
        let task = create(
            &f.db,
            &f.clock,
            TaskInput {
                project: Some("Pluto".into()),
                ..titled("same")
            },
        )
        .await
        .unwrap();
        let commits = f.db.committed_transactions();
        f.clock.advance_ms(60_000);
        let same = update(
            &f.db,
            &f.clock,
            &task.id,
            TaskPatch {
                title: Some("same".into()),
                project: Some(Some("Pluto".into())),
                priority: Some(None),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(same, task);
        assert_eq!(f.db.committed_transactions(), commits);
        assert_eq!(history(&f.db, &task.id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn editing_the_planned_date_records_where_it_moved_from_and_to() {
        let f = setup();
        let task = create(
            &f.db,
            &f.clock,
            TaskInput {
                planned_date: Some(d(2026, 10, 3)),
                ..titled("x")
            },
        )
        .await
        .unwrap();
        update(
            &f.db,
            &f.clock,
            &task.id,
            TaskPatch {
                planned_date: Some(Some(d(2026, 10, 1))),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let e = history(&f.db, &task.id).await.unwrap().pop().unwrap();
        assert_eq!(
            (e.from_date.as_deref(), e.to_date.as_deref()),
            (Some("2026-10-03"), Some("2026-10-01"))
        );
    }

    #[tokio::test]
    async fn deferring_moves_the_planned_date_forward_and_keeps_the_status() {
        let f = setup();
        let task = quick_add(&f.db, &f.clock, "later").await.unwrap();
        transition(&f.db, &f.clock, &task.id, InProgress)
            .await
            .unwrap();
        let mut sub = f.bus.subscribe();
        let deferred = defer(&f.db, &f.clock, &task.id, d(2026, 10, 6))
            .await
            .unwrap();
        assert_eq!(
            (deferred.planned_date, deferred.status),
            (Some(d(2026, 10, 6)), InProgress)
        );
        assert_eq!(
            next(&mut sub).await,
            Event::TaskDeferred {
                at: f.clock.now_ms(),
                id: task.id.clone(),
                from_date: Some("2026-10-03".into()),
                to_date: "2026-10-06".into()
            }
        );
        let e = history(&f.db, &task.id).await.unwrap().pop().unwrap();
        assert_eq!(
            (
                e.kind.as_str(),
                e.from_date.as_deref(),
                e.to_date.as_deref()
            ),
            ("DEFERRED", Some("2026-10-03"), Some("2026-10-06"))
        );
    }

    #[tokio::test]
    async fn deferring_must_go_forward_and_not_on_finished_tasks() {
        let f = setup();
        let task = quick_add(&f.db, &f.clock, "x").await.unwrap(); // planned today, 2026-10-03
        for same_or_earlier in [d(2026, 10, 3), d(2026, 10, 1)] {
            assert_eq!(
                defer(&f.db, &f.clock, &task.id, same_or_earlier)
                    .await
                    .unwrap_err()
                    .field
                    .as_deref(),
                Some("to_date")
            );
        }
        let unplanned = create(&f.db, &f.clock, titled("no plan")).await.unwrap();
        assert!(
            defer(&f.db, &f.clock, &unplanned.id, d(2026, 10, 3))
                .await
                .is_err(),
            "with no plan, 'later' means later than today"
        );
        assert!(defer(&f.db, &f.clock, &unplanned.id, d(2026, 10, 4))
            .await
            .is_ok());
        transition(&f.db, &f.clock, &task.id, Completed)
            .await
            .unwrap();
        assert_eq!(
            defer(&f.db, &f.clock, &task.id, d(2026, 10, 9))
                .await
                .unwrap_err()
                .code,
            ErrorCode::InvalidTransition
        );
    }

    #[tokio::test]
    async fn only_finished_tasks_can_be_deleted_and_their_history_goes_with_them() {
        let f = setup();
        let task = quick_add(&f.db, &f.clock, "x").await.unwrap();
        assert_eq!(
            delete(&f.db, &f.clock, &task.id).await.unwrap_err().code,
            ErrorCode::InvalidTransition,
            "planned"
        );
        transition(&f.db, &f.clock, &task.id, InProgress)
            .await
            .unwrap();
        assert!(
            delete(&f.db, &f.clock, &task.id).await.is_err(),
            "in progress"
        );
        transition(&f.db, &f.clock, &task.id, Cancelled)
            .await
            .unwrap();
        let mut sub = f.bus.subscribe();
        delete(&f.db, &f.clock, &task.id).await.unwrap();
        assert_eq!(
            next(&mut sub).await,
            Event::TaskDeleted {
                at: f.clock.now_ms(),
                id: task.id.clone()
            }
        );
        assert_eq!(
            get(&f.db, &task.id).await.unwrap_err().code,
            ErrorCode::NotFound
        );
        assert_eq!(
            history(&f.db, &task.id).await.unwrap_err().code,
            ErrorCode::NotFound
        );
    }

    #[tokio::test]
    async fn operations_on_a_missing_task_are_not_found() {
        let f = setup();
        assert_eq!(
            transition(&f.db, &f.clock, "nope", Completed)
                .await
                .unwrap_err()
                .code,
            ErrorCode::NotFound
        );
        assert_eq!(
            defer(&f.db, &f.clock, "nope", d(2026, 12, 1))
                .await
                .unwrap_err()
                .code,
            ErrorCode::NotFound
        );
        assert_eq!(
            delete(&f.db, &f.clock, "nope").await.unwrap_err().code,
            ErrorCode::NotFound
        );
        assert_eq!(
            update(&f.db, &f.clock, "nope", TaskPatch::default())
                .await
                .unwrap_err()
                .code,
            ErrorCode::NotFound
        );
    }

    #[test]
    fn overdue_means_due_before_today_and_not_finished() {
        let today = d(2026, 10, 3);
        let mut t = Task {
            due_date: Some(d(2026, 10, 2)),
            status: Planned,
            ..Task::sample()
        };
        assert!(t.is_overdue(today));
        for finished in [Completed, Cancelled] {
            t.status = finished;
            assert!(!t.is_overdue(today), "{finished:?} is never overdue");
        }
        t.status = InProgress;
        assert!(t.is_overdue(today));
        t.due_date = Some(today);
        assert!(!t.is_overdue(today), "due today is not overdue yet");
        t.due_date = None;
        assert!(!t.is_overdue(today));
    }

    // ---- views (R1-27) -----------------------------------------------------------------------

    struct Seed<'a> {
        f: &'a Fixture,
    }

    impl Seed<'_> {
        /// Create a task, then move it to `status` if it isn't PLANNED.
        async fn task(
            &self,
            title: &str,
            planned: Option<NaiveDate>,
            due: Option<NaiveDate>,
            priority: Option<Priority>,
            status: TaskStatus,
        ) -> Task {
            self.f.clock.advance_ms(10);
            let t = create(
                &self.f.db,
                &self.f.clock,
                TaskInput {
                    planned_date: planned,
                    due_date: due,
                    priority,
                    ..titled(title)
                },
            )
            .await
            .unwrap();
            match status {
                Planned => t,
                InProgress | Completed | Cancelled => {
                    transition(&self.f.db, &self.f.clock, &t.id, status)
                        .await
                        .unwrap()
                }
                Pending => {
                    transition(&self.f.db, &self.f.clock, &t.id, InProgress)
                        .await
                        .unwrap();
                    transition(&self.f.db, &self.f.clock, &t.id, Pending)
                        .await
                        .unwrap()
                }
            }
        }
    }

    async fn names(f: &Fixture, view: TaskView, filters: TaskFilters) -> Vec<String> {
        query(&f.db, &f.clock, view, filters)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.task.title)
            .collect()
    }

    #[tokio::test]
    async fn today_shows_in_progress_then_overdue_then_planned_today_and_nothing_finished_or_future(
    ) {
        let f = setup(); // today is 2026-10-03
        let s = Seed { f: &f };
        s.task("A planned today", Some(d(2026, 10, 3)), None, None, Planned)
            .await;
        s.task(
            "B planned today HIGH",
            Some(d(2026, 10, 3)),
            None,
            Some(Priority::High),
            Planned,
        )
        .await;
        s.task(
            "C in progress MEDIUM",
            Some(d(2026, 9, 30)),
            None,
            Some(Priority::Medium),
            InProgress,
        )
        .await;
        s.task(
            "D planned next week",
            Some(d(2026, 10, 10)),
            None,
            None,
            Planned,
        )
        .await;
        s.task(
            "E overdue",
            Some(d(2026, 9, 28)),
            Some(d(2026, 10, 1)),
            None,
            Planned,
        )
        .await;
        s.task(
            "F completed today",
            Some(d(2026, 10, 3)),
            None,
            None,
            Completed,
        )
        .await;
        s.task(
            "G cancelled today",
            Some(d(2026, 10, 3)),
            None,
            None,
            Cancelled,
        )
        .await;
        s.task("H pending today", Some(d(2026, 10, 3)), None, None, Pending)
            .await;
        s.task(
            "I overdue and in progress HIGH",
            None,
            Some(d(2026, 10, 2)),
            Some(Priority::High),
            InProgress,
        )
        .await;
        s.task("J no plan, no date", None, None, None, Planned)
            .await;

        let rows = query(&f.db, &f.clock, TaskView::Today, TaskFilters::default())
            .await
            .unwrap();
        let order: Vec<_> = rows
            .iter()
            .map(|r| r.task.title.chars().next().unwrap())
            .collect();
        assert_eq!(
            order,
            ['I', 'C', 'E', 'B', 'A', 'H'],
            "in progress (by priority), overdue, then planned today (by priority, then age)"
        );
        let overdue: Vec<_> = rows
            .iter()
            .filter(|r| r.overdue)
            .map(|r| r.task.title.chars().next().unwrap())
            .collect();
        assert_eq!(overdue, ['I', 'E']);
    }

    #[tokio::test]
    async fn upcoming_is_tomorrow_through_seven_days_ahead_inclusive() {
        let f = setup();
        let s = Seed { f: &f };
        s.task("today", Some(d(2026, 10, 3)), None, None, Planned)
            .await;
        s.task(
            "tomorrow low",
            Some(d(2026, 10, 4)),
            None,
            Some(Priority::Low),
            Planned,
        )
        .await;
        s.task(
            "tomorrow high",
            Some(d(2026, 10, 4)),
            None,
            Some(Priority::High),
            Planned,
        )
        .await;
        s.task("day 7", Some(d(2026, 10, 10)), None, None, Planned)
            .await;
        s.task("day 8", Some(d(2026, 10, 11)), None, None, Planned)
            .await;
        s.task("yesterday", Some(d(2026, 10, 2)), None, None, Planned)
            .await;
        s.task(
            "finished tomorrow",
            Some(d(2026, 10, 5)),
            None,
            None,
            Completed,
        )
        .await;
        s.task("unplanned", None, None, None, Planned).await;
        assert_eq!(
            names(&f, TaskView::Upcoming, TaskFilters::default()).await,
            ["tomorrow high", "tomorrow low", "day 7"]
        );
    }

    #[tokio::test]
    async fn pending_lists_only_paused_tasks_most_recently_paused_first() {
        let f = setup();
        let s = Seed { f: &f };
        s.task("first paused", None, None, None, Pending).await;
        s.task("running", None, None, None, InProgress).await;
        s.task("second paused", None, None, None, Pending).await;
        assert_eq!(
            names(&f, TaskView::Pending, TaskFilters::default()).await,
            ["second paused", "first paused"]
        );
    }

    #[tokio::test]
    async fn completed_is_a_history_newest_first_and_the_date_range_uses_local_days() {
        let f = setup();
        let s = Seed { f: &f };
        for (title, day) in [("done 1st", 1), ("done 2nd", 2), ("done 3rd", 3)] {
            f.clock.set_now_ms(
                chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2026, 10, day, 12, 0, 0)
                    .unwrap()
                    .timestamp_millis(),
            );
            s.task(title, None, None, None, Completed).await;
        }
        s.task("cancelled is not history", None, None, None, Cancelled)
            .await;
        assert_eq!(
            names(&f, TaskView::Completed, TaskFilters::default()).await,
            ["done 3rd", "done 2nd", "done 1st"]
        );
        let only_second = TaskFilters {
            from: Some(d(2026, 10, 2)),
            to: Some(d(2026, 10, 2)),
            ..Default::default()
        };
        assert_eq!(
            names(&f, TaskView::Completed, only_second).await,
            ["done 2nd"],
            "a one-day range is inclusive"
        );
        let from_second = TaskFilters {
            from: Some(d(2026, 10, 2)),
            ..Default::default()
        };
        assert_eq!(
            names(&f, TaskView::Completed, from_second).await,
            ["done 3rd", "done 2nd"]
        );
    }

    #[tokio::test]
    async fn a_completion_just_after_midnight_utc_still_belongs_to_the_previous_local_day() {
        let f = setup();
        f.clock.set_tz(chrono_tz::America::Los_Angeles);
        // 02:00 UTC on Oct 3 is 19:00 on Oct 2 in Los Angeles.
        f.clock.set_now_ms(
            chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2026, 10, 3, 2, 0, 0)
                .unwrap()
                .timestamp_millis(),
        );
        Seed { f: &f }
            .task("evening job", None, None, None, Completed)
            .await;
        let on = |day| TaskFilters {
            from: Some(d(2026, 10, day)),
            to: Some(d(2026, 10, day)),
            ..Default::default()
        };
        assert_eq!(names(&f, TaskView::Completed, on(2)).await, ["evening job"]);
        assert!(names(&f, TaskView::Completed, on(3)).await.is_empty());
    }

    #[tokio::test]
    async fn all_shows_every_status_with_finished_tasks_last_and_filters_combine() {
        let f = setup();
        let s = Seed { f: &f };
        s.task(
            "done",
            Some(d(2026, 10, 1)),
            None,
            Some(Priority::High),
            Completed,
        )
        .await;
        s.task(
            "low planned",
            Some(d(2026, 10, 2)),
            None,
            Some(Priority::Low),
            Planned,
        )
        .await;
        s.task(
            "high planned",
            Some(d(2026, 10, 9)),
            None,
            Some(Priority::High),
            Planned,
        )
        .await;
        s.task("unplanned", None, None, None, Planned).await;
        s.task("cancelled", None, None, None, Cancelled).await;
        assert_eq!(
            names(&f, TaskView::All, TaskFilters::default()).await,
            [
                "high planned",
                "low planned",
                "unplanned",
                "done",
                "cancelled"
            ]
        );
        assert_eq!(
            names(
                &f,
                TaskView::All,
                TaskFilters {
                    status: Some(Cancelled),
                    ..Default::default()
                }
            )
            .await,
            ["cancelled"]
        );
        assert_eq!(
            names(
                &f,
                TaskView::All,
                TaskFilters {
                    priority: Some(Priority::High),
                    ..Default::default()
                }
            )
            .await,
            ["high planned", "done"]
        );
        let range = TaskFilters {
            from: Some(d(2026, 10, 2)),
            to: Some(d(2026, 10, 9)),
            ..Default::default()
        };
        assert_eq!(
            names(&f, TaskView::All, range.clone()).await,
            ["high planned", "low planned"],
            "planned-date range, inclusive"
        );
        let both = TaskFilters {
            priority: Some(Priority::Low),
            ..range
        };
        assert_eq!(
            names(&f, TaskView::All, both).await,
            ["low planned"],
            "filters are ANDed"
        );
    }

    #[tokio::test]
    async fn the_project_filter_ignores_case_and_surrounding_spaces() {
        let f = setup();
        for (title, project) in [("a", "Pluto"), ("b", "pluto"), ("c", "Other")] {
            create(
                &f.db,
                &f.clock,
                TaskInput {
                    project: Some(project.into()),
                    planned_date: Some(f.clock.today()),
                    ..titled(title)
                },
            )
            .await
            .unwrap();
        }
        let pluto = TaskFilters {
            project: Some("  PLUTO ".into()),
            ..Default::default()
        };
        assert_eq!(names(&f, TaskView::Today, pluto.clone()).await.len(), 2);
        assert_eq!(names(&f, TaskView::All, pluto).await.len(), 2);
    }

    #[tokio::test]
    async fn the_note_filter_lists_only_tasks_linked_to_that_note() {
        let f = setup();
        let note = crate::notes::create(&f.db, &f.clock, crate::notes::NoteInput::default())
            .await
            .unwrap();
        for (title, linked) in [("linked", true), ("other", false)] {
            create(
                &f.db,
                &f.clock,
                TaskInput {
                    note_id: linked.then(|| note.id.clone()),
                    ..titled(title)
                },
            )
            .await
            .unwrap();
        }
        let only = TaskFilters {
            note_id: Some(note.id.clone()),
            ..Default::default()
        };
        assert_eq!(names(&f, TaskView::All, only).await, ["linked"]);
    }

    #[tokio::test]
    async fn the_priority_filter_narrows_the_today_view_too() {
        let f = setup();
        let s = Seed { f: &f };
        s.task(
            "high",
            Some(d(2026, 10, 3)),
            None,
            Some(Priority::High),
            Planned,
        )
        .await;
        s.task(
            "low",
            Some(d(2026, 10, 3)),
            None,
            Some(Priority::Low),
            Planned,
        )
        .await;
        assert_eq!(
            names(
                &f,
                TaskView::Today,
                TaskFilters {
                    priority: Some(Priority::High),
                    ..Default::default()
                }
            )
            .await,
            ["high"]
        );
    }

    #[tokio::test]
    async fn every_view_of_an_empty_workspace_is_just_empty() {
        let f = setup();
        for view in [
            TaskView::Today,
            TaskView::Upcoming,
            TaskView::Pending,
            TaskView::All,
            TaskView::Completed,
        ] {
            assert!(
                query(&f.db, &f.clock, view, TaskFilters::default())
                    .await
                    .unwrap()
                    .is_empty(),
                "{view:?}"
            );
        }
    }
}
