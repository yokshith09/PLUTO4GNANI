//! Notes (PRD §6.1, R1-01 … R1-11). Pure data rules plus the operations that persist them.
//!
//! Every mutation runs through [`Database::write`], so its event is published only after the
//! commit. Timestamps come from the injected [`Clock`].

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::bus::{EventBus, Recv};
use crate::clock::Clock;
use crate::db::Database;
use crate::error::{AppError, Result};
use crate::events::Event;
use crate::ids::new_id;
use crate::labels::Label;

pub const TITLE_MAX: usize = 200;
pub const BODY_MAX: usize = 100_000;
pub const EXCERPT_CHARS: usize = 200;
/// How long a note stays in the Bin before it is removed for good.
pub const BIN_RETENTION_DAYS: i64 = 30;
/// The most rows a search returns.
pub const SEARCH_LIMIT: usize = 200;

const DAY_MS: i64 = 86_400_000;

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoteColor {
    #[default]
    Default,
    Red,
    Orange,
    Yellow,
    Green,
    Teal,
    Blue,
    Purple,
    Gray,
}

impl NoteColor {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Red => "red",
            Self::Orange => "orange",
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::Teal => "teal",
            Self::Blue => "blue",
            Self::Purple => "purple",
            Self::Gray => "gray",
        }
    }

    fn from_db(s: &str) -> Result<Self> {
        Ok(match s {
            "default" => Self::Default,
            "red" => Self::Red,
            "orange" => Self::Orange,
            "yellow" => Self::Yellow,
            "green" => Self::Green,
            "teal" => Self::Teal,
            "blue" => Self::Blue,
            "purple" => Self::Purple,
            "gray" => Self::Gray,
            _ => {
                return Err(AppError::internal(
                    "A note has a colour Pluto doesn't know.",
                ))
            }
        })
    }
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub body: String,
    pub color: NoteColor,
    pub pinned: bool,
    pub archived: bool,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub edited_at: i64,
    /// Sorted by folded name.
    pub labels: Vec<Label>,
    /// The project this note belongs to, like `Task::project` (owner 2026-10-25).
    pub project: Option<String>,
}

#[cfg(test)]
impl Note {
    pub(crate) fn sample() -> Self {
        Self {
            id: "n1".into(),
            title: "Hackathon ideas".into(),
            body: "- GCP setup".into(),
            color: NoteColor::Yellow,
            pinned: false,
            archived: false,
            created_at: 1,
            edited_at: 2,
            labels: vec![Label {
                id: "l1".into(),
                name: "work".into(),
            }],
            project: None,
        }
    }
}

/// What the list shows: a card, not the whole note.
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteSummary {
    pub id: String,
    pub title: String,
    pub excerpt: String,
    pub color: NoteColor,
    pub pinned: bool,
    pub archived: bool,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub edited_at: i64,
    pub labels: Vec<Label>,
    pub project: Option<String>,
}

/// A card in the Bin: what the list shows, plus when the note was binned. It is permanently
/// removed [`BIN_RETENTION_DAYS`] days after `deleted_at`.
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BinNote {
    pub id: String,
    pub title: String,
    pub excerpt: String,
    pub color: NoteColor,
    pub pinned: bool,
    pub archived: bool,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub edited_at: i64,
    pub labels: Vec<Label>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub deleted_at: i64,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteSort {
    #[default]
    LastEdited,
    Created,
    Color,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, optional_fields = nullable))]
#[serde(default)]
pub struct NoteInput {
    pub title: String,
    pub body: String,
    pub color: NoteColor,
    pub label_ids: Vec<String>,
    pub project: Option<String>,
}

/// Fields left as `None` are not touched.
#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, optional_fields = nullable))]
#[serde(default)]
pub struct NotePatch {
    pub title: Option<String>,
    pub body: Option<String>,
    pub color: Option<NoteColor>,
    pub label_ids: Option<Vec<String>>,
    /// `Some("")` takes the note out of its project.
    pub project: Option<String>,
}

fn check_title(title: &str) -> Result<()> {
    if title.chars().count() > TITLE_MAX {
        return Err(AppError::validation(
            "title",
            format!("Titles can be up to {TITLE_MAX} characters."),
        ));
    }
    Ok(())
}

pub const PROJECT_MAX: usize = 100;

/// Trimmed; empty means no project.
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

fn check_body(body: &str) -> Result<()> {
    if body.chars().count() > BODY_MAX {
        return Err(AppError::validation(
            "body",
            format!("Notes can be up to {BODY_MAX} characters."),
        ));
    }
    Ok(())
}

fn not_found() -> AppError {
    AppError::not_found("That note no longer exists.")
}

fn note_labels(conn: &Connection, note_id: &str) -> Result<Vec<Label>> {
    let mut stmt = conn.prepare(
        "SELECT l.id, l.name FROM note_labels nl JOIN labels l ON l.id = nl.label_id
          WHERE nl.note_id = ?1 ORDER BY l.name_folded",
    )?;
    let labels = stmt
        .query_map([note_id], |r| {
            Ok(Label {
                id: r.get(0)?,
                name: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(labels)
}

fn load(conn: &Connection, id: &str) -> Result<Note> {
    let row = conn
        .query_row(
            "SELECT title, body, color, pinned, archived, created_at, edited_at, project FROM notes WHERE id = ?1",
            [id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, bool>(3)?, r.get::<_, bool>(4)?, r.get(5)?, r.get(6)?, r.get::<_, Option<String>>(7)?)),
        )
        .optional()?;
    let Some((title, body, color, pinned, archived, created_at, edited_at, project)) = row else {
        return Err(not_found());
    };
    Ok(Note {
        id: id.to_owned(),
        title,
        body,
        color: NoteColor::from_db(&color)?,
        pinned,
        archived,
        created_at,
        edited_at,
        labels: note_labels(conn, id)?,
        project,
    })
}

/// Replace a note's labels. Every id must exist; duplicates are ignored.
fn set_labels(conn: &Connection, note_id: &str, label_ids: &[String]) -> Result<()> {
    let mut seen = HashSet::new();
    let unique: Vec<&String> = label_ids
        .iter()
        .filter(|id| seen.insert(id.as_str()))
        .collect();
    for id in &unique {
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM labels WHERE id = ?1)",
            [id],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(AppError::not_found("One of those labels no longer exists.")
                .with_field("label_ids"));
        }
    }
    conn.execute("DELETE FROM note_labels WHERE note_id = ?1", [note_id])?;
    for id in unique {
        conn.execute(
            "INSERT INTO note_labels (note_id, label_id) VALUES (?1, ?2)",
            params![note_id, id],
        )?;
    }
    Ok(())
}

/// Refuse to change a note that is in the Bin (it has to be restored first). A missing note is
/// `NOT_FOUND`.
fn ensure_live(conn: &Connection, id: &str) -> Result<()> {
    let deleted_at: Option<Option<i64>> = conn
        .query_row("SELECT deleted_at FROM notes WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .optional()?;
    match deleted_at {
        None => Err(not_found()),
        Some(Some(_)) => Err(AppError::not_found(
            "That note is in the Bin. Restore it to change it.",
        )),
        Some(None) => Ok(()),
    }
}

fn same_labels(a: &[Label], ids: &[String]) -> bool {
    let current: HashSet<&str> = a.iter().map(|l| l.id.as_str()).collect();
    let wanted: HashSet<&str> = ids.iter().map(String::as_str).collect();
    current == wanted
}

pub async fn get(db: &Database, id: &str) -> Result<Note> {
    let id = id.to_owned();
    db.read(move |conn| load(conn, &id)).await
}

pub async fn create(db: &Database, clock: &dyn Clock, input: NoteInput) -> Result<Note> {
    check_title(&input.title)?;
    check_body(&input.body)?;
    let project = clean_project(input.project.as_deref())?;
    let (id, now) = (new_id(), clock.now_ms());
    db.write(move |tx| {
        tx.execute(
            "INSERT INTO notes (id, title, body, color, pinned, archived, created_at, edited_at, project)
             VALUES (?1, ?2, ?3, ?4, 0, 0, ?5, ?5, ?6)",
            params![id, input.title, input.body, input.color.as_str(), now, project],
        )?;
        set_labels(tx, &id, &input.label_ids)?;
        let note = load(tx, &id)?;
        Ok((note.clone(), vec![Event::NoteCreated { at: now, note }]))
    })
    .await
}

/// Apply a patch. `edited_at` moves only when the title, body, colour or labels really change
/// (R1-04): an autosave that changes nothing writes nothing and publishes nothing.
pub async fn update(db: &Database, clock: &dyn Clock, id: &str, patch: NotePatch) -> Result<Note> {
    if let Some(t) = &patch.title {
        check_title(t)?;
    }
    if let Some(b) = &patch.body {
        check_body(b)?;
    }
    let mut patch = patch;
    if let Some(p) = &patch.project {
        patch.project = Some(clean_project(Some(p))?.unwrap_or_default());
    }
    let current = get(db, id).await?;
    if !differs(&current, &patch) {
        return Ok(current);
    }
    let (id, now) = (id.to_owned(), clock.now_ms());
    db.write(move |tx| {
        ensure_live(tx, &id)?;
        let current = load(tx, &id)?;
        if !differs(&current, &patch) {
            return Ok((current, vec![]));
        }
        tx.execute(
            "UPDATE notes SET title = ?1, body = ?2, color = ?3, edited_at = ?4, project = ?5 WHERE id = ?6",
            params![
                patch.title.as_deref().unwrap_or(&current.title),
                patch.body.as_deref().unwrap_or(&current.body),
                patch.color.unwrap_or(current.color).as_str(),
                now,
                match &patch.project {
                    Some(p) if p.is_empty() => None,
                    Some(p) => Some(p.as_str()),
                    None => current.project.as_deref(),
                },
                id
            ],
        )?;
        if let Some(ids) = &patch.label_ids {
            set_labels(tx, &id, ids)?;
        }
        let note = load(tx, &id)?;
        Ok((note.clone(), vec![Event::NoteUpdated { at: now, note }]))
    })
    .await
}

fn differs(current: &Note, patch: &NotePatch) -> bool {
    patch.title.as_ref().is_some_and(|t| *t != current.title)
        || patch.body.as_ref().is_some_and(|b| *b != current.body)
        || patch.color.is_some_and(|c| c != current.color)
        || patch
            .label_ids
            .as_ref()
            .is_some_and(|ids| !same_labels(&current.labels, ids))
        || patch
            .project
            .as_ref()
            .is_some_and(|p| current.project.as_deref().unwrap_or("") != p)
}

/// Pin or unpin. Does not touch `edited_at`. Pinning an archived note is allowed but has no
/// effect until it is restored (Schema §3.2).
pub async fn set_pinned(db: &Database, clock: &dyn Clock, id: &str, pinned: bool) -> Result<Note> {
    set_flag(db, clock, id, Flag::Pinned, pinned).await
}

/// Archive or restore. Does not touch `edited_at`.
pub async fn set_archived(
    db: &Database,
    clock: &dyn Clock,
    id: &str,
    archived: bool,
) -> Result<Note> {
    set_flag(db, clock, id, Flag::Archived, archived).await
}

#[derive(Clone, Copy)]
enum Flag {
    Pinned,
    Archived,
}

async fn set_flag(
    db: &Database,
    clock: &dyn Clock,
    id: &str,
    flag: Flag,
    value: bool,
) -> Result<Note> {
    let current = get(db, id).await?;
    let already = match flag {
        Flag::Pinned => current.pinned == value,
        Flag::Archived => current.archived == value,
    };
    if already {
        return Ok(current);
    }
    let (id, now) = (id.to_owned(), clock.now_ms());
    db.write(move |tx| {
        ensure_live(tx, &id)?;
        let column = match flag {
            Flag::Pinned => "pinned",
            Flag::Archived => "archived",
        };
        // `column` is one of two constants above, never user input.
        if tx.execute(
            &format!("UPDATE notes SET {column} = ?1 WHERE id = ?2"),
            params![value, id],
        )? == 0
        {
            return Err(not_found());
        }
        let event = match flag {
            Flag::Pinned => Event::NotePinnedChanged {
                at: now,
                id: id.clone(),
                pinned: value,
            },
            Flag::Archived => Event::NoteArchivedChanged {
                at: now,
                id: id.clone(),
                archived: value,
            },
        };
        Ok((load(tx, &id)?, vec![event]))
    })
    .await
}

/// Move a note to the Bin (R1-05). It keeps its labels and every other field, disappears from
/// the list, search and label counts, and can be restored until it is purged. Returns the note.
/// Deleting a note that is already in the Bin changes nothing.
pub async fn delete(db: &Database, clock: &dyn Clock, id: &str) -> Result<Note> {
    let (id, now) = (id.to_owned(), clock.now_ms());
    db.write(move |tx| {
        let deleted_at: Option<Option<i64>> = tx
            .query_row("SELECT deleted_at FROM notes WHERE id = ?1", [&id], |r| {
                r.get(0)
            })
            .optional()?;
        match deleted_at {
            None => return Err(not_found()),
            Some(Some(_)) => return Ok((load(tx, &id)?, vec![])),
            Some(None) => {}
        }
        tx.execute(
            "UPDATE notes SET deleted_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;
        Ok((load(tx, &id)?, vec![Event::NoteDeleted { at: now, id }]))
    })
    .await
}

/// Take a note out of the Bin. It comes back exactly as it was. Restoring a note that is not in
/// the Bin changes nothing.
pub async fn restore_from_bin(db: &Database, clock: &dyn Clock, id: &str) -> Result<Note> {
    let (id, now) = (id.to_owned(), clock.now_ms());
    db.write(move |tx| {
        let restored = tx.execute(
            "UPDATE notes SET deleted_at = NULL WHERE id = ?1 AND deleted_at IS NOT NULL",
            [&id],
        )?;
        let note = load(tx, &id)?;
        let events = if restored > 0 {
            vec![Event::NoteRestored {
                at: now,
                note: note.clone(),
            }]
        } else {
            vec![]
        };
        Ok((note, events))
    })
    .await
}

/// What is in the Bin, most recently deleted first.
pub async fn list_bin(db: &Database) -> Result<Vec<BinNote>> {
    db.read(|conn| {
        let rows = summaries(
            conn,
            "WHERE n.deleted_at IS NOT NULL ORDER BY n.deleted_at DESC, n.id DESC",
            &[],
        )?;
        Ok(rows
            .into_iter()
            .map(|(s, deleted_at)| BinNote {
                id: s.id,
                title: s.title,
                excerpt: s.excerpt,
                color: s.color,
                pinned: s.pinned,
                archived: s.archived,
                created_at: s.created_at,
                edited_at: s.edited_at,
                labels: s.labels,
                deleted_at: deleted_at.unwrap_or_default(),
            })
            .collect())
    })
    .await
}

/// Remove binned notes by id, in the caller's transaction. Reminders that pointed at them are
/// unlinked by the database.
fn purge_ids(tx: &rusqlite::Transaction, ids: Vec<String>, now: i64) -> Result<(u64, Vec<Event>)> {
    let mut events = Vec::with_capacity(ids.len());
    for id in ids {
        tx.execute(
            "DELETE FROM notes WHERE id = ?1 AND deleted_at IS NOT NULL",
            [&id],
        )?;
        events.push(Event::NotePurged { at: now, id });
    }
    Ok((events.len() as u64, events))
}

/// Delete one binned note for good. A note that is not in the Bin is left alone (`CONFLICT`).
pub async fn purge(db: &Database, clock: &dyn Clock, id: &str) -> Result<()> {
    let (id, now) = (id.to_owned(), clock.now_ms());
    db.write(move |tx| {
        ensure_binned(tx, &id)?;
        let (_, events) = purge_ids(tx, vec![id], now)?;
        Ok(((), events))
    })
    .await
}

fn ensure_binned(conn: &Connection, id: &str) -> Result<()> {
    let deleted_at: Option<Option<i64>> = conn
        .query_row("SELECT deleted_at FROM notes WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .optional()?;
    match deleted_at {
        None => Err(not_found()),
        Some(None) => Err(AppError::conflict(
            "Only notes in the Bin can be deleted forever.",
        )),
        Some(Some(_)) => Ok(()),
    }
}

/// Empty the Bin. Returns how many notes were removed.
pub async fn empty_bin(db: &Database, clock: &dyn Clock) -> Result<u64> {
    let now = clock.now_ms();
    db.write(move |tx| {
        let ids = binned_ids(tx, None)?;
        purge_ids(tx, ids, now)
    })
    .await
}

/// Remove notes that have been in the Bin for more than [`BIN_RETENTION_DAYS`] days. Returns how
/// many were removed. Runs at startup and on every day rollover, never on a timer of its own.
pub async fn purge_expired(db: &Database, clock: &dyn Clock) -> Result<u64> {
    let now = clock.now_ms();
    let cutoff = now - BIN_RETENTION_DAYS * DAY_MS;
    db.write(move |tx| {
        let ids = binned_ids(tx, Some(cutoff))?;
        purge_ids(tx, ids, now)
    })
    .await
}

fn binned_ids(conn: &Connection, older_than: Option<i64>) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM notes WHERE deleted_at IS NOT NULL AND (?1 IS NULL OR deleted_at < ?1)",
    )?;
    let ids = stmt
        .query_map([older_than], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    Ok(ids)
}

/// Run [`purge_expired`] whenever the day rolls over (and after falling behind on the bus).
/// Event-driven: it parks on the bus between days.
pub fn spawn_bin_janitor(
    db: Arc<Database>,
    clock: Arc<dyn Clock>,
    bus: &EventBus,
) -> tokio::task::JoinHandle<()> {
    let mut events = bus.subscribe();
    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Recv::Event(Event::DayRolledOver { .. }) | Recv::Resync { .. } => {
                    if let Err(error) = purge_expired(&db, &*clock).await {
                        tracing::warn!(code = ?error.code, "couldn't clear old notes out of the Bin; it will be retried tomorrow");
                    }
                }
                Recv::Closed => break,
                Recv::Event(_) => {}
            }
        }
    })
}

/// A note with no title and no body is discarded when its editor closes (R1-01). Returns whether
/// anything was deleted. Labels and colour do not count as content. This is a hard delete: an
/// empty note is not worth a place in the Bin.
pub async fn discard_if_empty(db: &Database, clock: &dyn Clock, id: &str) -> Result<bool> {
    let (id, now) = (id.to_owned(), clock.now_ms());
    db.write(move |tx| {
        let removed = tx.execute(
            "DELETE FROM notes WHERE id = ?1 AND title = '' AND body = '' AND deleted_at IS NULL",
            [&id],
        )?;
        let events = if removed > 0 {
            vec![Event::NoteDeleted { at: now, id }]
        } else {
            vec![]
        };
        Ok((removed > 0, events))
    })
    .await
}

type SummaryRow = (NoteSummary, Option<i64>);

/// The shared card query: `tail` is everything after `FROM notes n` (a WHERE and an ORDER BY, and
/// optionally a LIMIT), written by this module and never from user input; user values arrive
/// only through `args`. Labels for all the cards come from one more query. The second element of
/// each row is `deleted_at`.
fn summaries(
    conn: &Connection,
    tail: &str,
    args: &[&dyn rusqlite::ToSql],
) -> Result<Vec<SummaryRow>> {
    let sql = format!(
        "SELECT n.id, n.title, substr(n.body, 1, {EXCERPT_CHARS}), n.color, n.pinned, n.archived, n.created_at, n.edited_at, n.deleted_at, n.project
           FROM notes n {tail}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt
        .query_map(args, |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, bool>(4)?,
                r.get::<_, bool>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, i64>(7)?,
                r.get::<_, Option<i64>>(8)?,
                r.get::<_, Option<String>>(9)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(
            |(
                id,
                title,
                excerpt,
                color,
                pinned,
                archived,
                created_at,
                edited_at,
                deleted_at,
                project,
            )| {
                Ok((
                    NoteSummary {
                        id,
                        title,
                        excerpt,
                        color: NoteColor::from_db(&color)?,
                        pinned,
                        archived,
                        created_at,
                        edited_at,
                        labels: Vec::new(),
                        project,
                    },
                    deleted_at,
                ))
            },
        )
        .collect::<Result<Vec<_>>>()?;

    if rows.is_empty() {
        return Ok(rows);
    }
    // One query for every card's labels instead of one per note.
    let ids = serde_json::to_string(&rows.iter().map(|(s, _)| s.id.as_str()).collect::<Vec<_>>())
        .map_err(|_| AppError::internal("Pluto couldn't prepare the notes list."))?;
    let mut by_note: HashMap<String, Vec<Label>> = HashMap::new();
    let mut links = conn.prepare(
        "SELECT nl.note_id, l.id, l.name FROM note_labels nl
           JOIN labels l ON l.id = nl.label_id
          WHERE nl.note_id IN (SELECT value FROM json_each(?1)) ORDER BY l.name_folded",
    )?;
    for row in links.query_map([ids], |r| {
        Ok((
            r.get::<_, String>(0)?,
            Label {
                id: r.get(1)?,
                name: r.get(2)?,
            },
        ))
    })? {
        let (note_id, label) = row?;
        by_note.entry(note_id).or_default().push(label);
    }
    for (note, _) in &mut rows {
        note.labels = by_note.remove(&note.id).unwrap_or_default();
    }
    Ok(rows)
}

/// The Notes view (`archived = false`) or the Archive view. Pinned notes come first in the Notes
/// view only; in the Archive view pinning is ignored (Schema §3.2). Notes in the Bin never appear.
pub async fn list(
    db: &Database,
    archived: bool,
    label_id: Option<&str>,
    sort: NoteSort,
) -> Result<Vec<NoteSummary>> {
    let label_id = label_id.map(str::to_owned);
    db.read(move |conn| {
        let order = match sort {
            NoteSort::LastEdited => "n.edited_at DESC, n.id DESC",
            NoteSort::Created => "n.created_at DESC, n.id DESC",
            NoteSort::Color => "n.color ASC, n.edited_at DESC, n.id DESC",
        };
        // `order` is one of three constants above, never user input.
        let tail = format!(
            "WHERE n.archived = ?1 AND n.deleted_at IS NULL
                AND (?2 IS NULL OR EXISTS (SELECT 1 FROM note_labels nl WHERE nl.note_id = n.id AND nl.label_id = ?2))
              ORDER BY (CASE WHEN ?1 = 0 THEN n.pinned ELSE 0 END) DESC, {order}"
        );
        let rows = summaries(conn, &tail, &[&archived, &label_id])?;
        Ok(rows.into_iter().map(|(s, _)| s).collect())
    })
    .await
}

/// Escape `%`, `_` and the escape character itself so the query is matched literally.
fn like_pattern(query: &str) -> String {
    let mut out = String::with_capacity(query.len() + 2);
    out.push('%');
    for c in query.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

/// Plain search (ADR-017: no search tables). A note matches if its title or body contains the
/// query, ignoring case; SQLite folds only ASCII letters, so accented capitals match themselves
/// exactly. The Bin is excluded, `archived` picks the view, newest-edited first, at most
/// [`SEARCH_LIMIT`] results. A blank query finds nothing.
pub async fn search(db: &Database, query: &str, archived: bool) -> Result<Vec<NoteSummary>> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let pattern = like_pattern(query);
    db.read(move |conn| {
        let tail = format!(
            "WHERE n.archived = ?1 AND n.deleted_at IS NULL
                AND (n.title LIKE ?2 ESCAPE '\\' OR n.body LIKE ?2 ESCAPE '\\')
              ORDER BY n.edited_at DESC, n.id DESC LIMIT {SEARCH_LIMIT}"
        );
        let rows = summaries(conn, &tail, &[&archived, &pattern])?;
        Ok(rows.into_iter().map(|(s, _)| s).collect())
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
    use crate::labels;

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
            clock: FakeClock::new(1_000, Tz::UTC),
        }
    }

    impl Fixture {
        async fn db_deleted_at(&self, id: &str) -> i64 {
            let id = id.to_owned();
            self.db
                .read(move |c| {
                    Ok(
                        c.query_row("SELECT deleted_at FROM notes WHERE id = ?1", [id], |r| {
                            r.get(0)
                        })?,
                    )
                })
                .await
                .unwrap()
        }
    }

    async fn event(sub: &mut Subscriber) -> Event {
        match sub.recv().await {
            Recv::Event(e) => e,
            other => panic!("expected an event, got {other:?}"),
        }
    }

    fn input(title: &str, body: &str) -> NoteInput {
        NoteInput {
            title: title.into(),
            body: body.into(),
            ..Default::default()
        }
    }

    async fn make(f: &Fixture, title: &str, body: &str) -> Note {
        f.clock.advance_ms(10);
        create(&f.db, &f.clock, input(title, body)).await.unwrap()
    }

    // ---- create (R1-01) ------------------------------------------------------------------------

    #[tokio::test]
    async fn a_new_note_has_the_documented_defaults_and_is_announced() {
        let f = setup();
        let mut sub = f.bus.subscribe();
        let note = create(&f.db, &f.clock, input("Hackathon ideas", "- GCP setup"))
            .await
            .unwrap();
        assert_eq!(
            (note.title.as_str(), note.body.as_str()),
            ("Hackathon ideas", "- GCP setup")
        );
        assert_eq!(
            (note.color, note.pinned, note.archived),
            (NoteColor::Default, false, false)
        );
        assert_eq!((note.created_at, note.edited_at), (1_000, 1_000));
        assert!(note.labels.is_empty());
        assert_eq!(
            event(&mut sub).await,
            Event::NoteCreated {
                at: 1_000,
                note: note.clone()
            }
        );
        assert_eq!(get(&f.db, &note.id).await.unwrap(), note);
    }

    #[tokio::test]
    async fn an_empty_note_can_exist_until_its_editor_closes() {
        let f = setup();
        let note = create(&f.db, &f.clock, NoteInput::default()).await.unwrap();
        assert_eq!((note.title.as_str(), note.body.as_str()), ("", ""));
    }

    #[tokio::test]
    async fn title_and_body_limits_count_characters_and_name_the_field() {
        let f = setup();
        assert!(create(
            &f.db,
            &f.clock,
            input(&"é".repeat(200), &"x".repeat(100_000))
        )
        .await
        .is_ok());
        let err = create(&f.db, &f.clock, input(&"é".repeat(201), ""))
            .await
            .unwrap_err();
        assert_eq!(
            (err.code, err.field.as_deref()),
            (ErrorCode::Validation, Some("title"))
        );
        let err = create(&f.db, &f.clock, input("", &"x".repeat(100_001)))
            .await
            .unwrap_err();
        assert_eq!(
            (err.code, err.field.as_deref()),
            (ErrorCode::Validation, Some("body"))
        );
        assert_eq!(
            list(&f.db, false, None, NoteSort::default())
                .await
                .unwrap()
                .len(),
            1,
            "rejected notes were not saved"
        );
    }

    #[tokio::test]
    async fn a_missing_label_rejects_the_whole_create() {
        let f = setup();
        let err = create(
            &f.db,
            &f.clock,
            NoteInput {
                label_ids: vec!["nope".into()],
                ..input("x", "y")
            },
        )
        .await
        .unwrap_err();
        assert_eq!(
            (err.code, err.field.as_deref()),
            (ErrorCode::NotFound, Some("label_ids"))
        );
        assert!(
            list(&f.db, false, None, NoteSort::default())
                .await
                .unwrap()
                .is_empty(),
            "no half-created note"
        );
    }

    #[tokio::test]
    async fn labels_are_attached_sorted_by_name_with_duplicates_ignored() {
        let f = setup();
        let work = labels::create(&f.db, &f.clock, "work").await.unwrap();
        let alpha = labels::create(&f.db, &f.clock, "Alpha").await.unwrap();
        let note = create(
            &f.db,
            &f.clock,
            NoteInput {
                label_ids: vec![work.id.clone(), alpha.id.clone(), work.id.clone()],
                ..input("x", "")
            },
        )
        .await
        .unwrap();
        assert_eq!(note.labels, vec![alpha, work]);
    }

    // ---- edit (R1-02 rules, R1-04) -------------------------------------------------------------

    #[tokio::test]
    async fn editing_changes_only_what_was_given_and_moves_edited_at() {
        let f = setup();
        let note = make(&f, "Title", "Body").await;
        let mut sub = f.bus.subscribe();
        f.clock.advance_ms(500);
        let updated = update(
            &f.db,
            &f.clock,
            &note.id,
            NotePatch {
                title: Some("New title".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(
            (updated.title.as_str(), updated.body.as_str()),
            ("New title", "Body")
        );
        assert_eq!(updated.created_at, note.created_at);
        assert_eq!(updated.edited_at, f.clock.now_ms());
        assert_eq!(
            event(&mut sub).await,
            Event::NoteUpdated {
                at: f.clock.now_ms(),
                note: updated
            }
        );
    }

    #[tokio::test]
    async fn a_note_joins_and_leaves_a_project() {
        let f = setup();
        let note = make(&f, "Outline", "Body").await;
        assert_eq!(note.project, None);
        let set = |p: &str| NotePatch {
            project: Some(p.into()),
            ..Default::default()
        };
        let n = update(&f.db, &f.clock, &note.id, set("  Thesis  "))
            .await
            .unwrap();
        assert_eq!(n.project.as_deref(), Some("Thesis"));
        let cards = list(&f.db, false, None, NoteSort::LastEdited)
            .await
            .unwrap();
        assert_eq!(cards[0].project.as_deref(), Some("Thesis"));
        let n = update(&f.db, &f.clock, &note.id, set("")).await.unwrap();
        assert_eq!(n.project, None);
        let err = update(&f.db, &f.clock, &note.id, set(&"x".repeat(101)))
            .await
            .unwrap_err();
        assert_eq!(err.field.as_deref(), Some("project"));
    }

    #[tokio::test]
    async fn an_autosave_that_changes_nothing_writes_nothing_and_says_nothing() {
        let f = setup();
        let note = make(&f, "Title", "Body").await;
        let commits = f.db.committed_transactions();
        let mut sub = f.bus.subscribe();
        f.clock.advance_ms(60_000);
        let same = update(
            &f.db,
            &f.clock,
            &note.id,
            NotePatch {
                title: Some("Title".into()),
                body: Some("Body".into()),
                color: Some(NoteColor::Default),
                label_ids: Some(vec![]),
                project: Some(String::new()),
            },
        )
        .await
        .unwrap();
        assert_eq!(same, note, "edited_at must not move when nothing changed");
        assert_eq!(
            f.db.committed_transactions(),
            commits,
            "no transaction for a no-op"
        );
        f.bus.publish(Event::AppShuttingDown { at: 0 });
        assert!(
            matches!(event(&mut sub).await, Event::AppShuttingDown { .. }),
            "no NoteUpdated for a no-op"
        );
    }

    #[tokio::test]
    async fn the_same_labels_in_a_different_order_is_not_a_change() {
        let f = setup();
        let (a, b) = (
            labels::create(&f.db, &f.clock, "a").await.unwrap(),
            labels::create(&f.db, &f.clock, "b").await.unwrap(),
        );
        let note = create(
            &f.db,
            &f.clock,
            NoteInput {
                label_ids: vec![a.id.clone(), b.id.clone()],
                ..input("x", "")
            },
        )
        .await
        .unwrap();
        f.clock.advance_ms(99);
        let same = update(
            &f.db,
            &f.clock,
            &note.id,
            NotePatch {
                label_ids: Some(vec![b.id, a.id]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(same.edited_at, note.edited_at);
    }

    #[tokio::test]
    async fn changing_colour_or_labels_counts_as_an_edit() {
        let f = setup();
        let work = labels::create(&f.db, &f.clock, "work").await.unwrap();
        let note = make(&f, "t", "b").await;
        f.clock.advance_ms(5);
        let coloured = update(
            &f.db,
            &f.clock,
            &note.id,
            NotePatch {
                color: Some(NoteColor::Teal),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(
            (coloured.color, coloured.edited_at),
            (NoteColor::Teal, f.clock.now_ms())
        );
        f.clock.advance_ms(5);
        let labelled = update(
            &f.db,
            &f.clock,
            &note.id,
            NotePatch {
                label_ids: Some(vec![work.id.clone()]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(
            (labelled.labels, labelled.edited_at),
            (vec![work], f.clock.now_ms())
        );
    }

    #[tokio::test]
    async fn editing_a_missing_note_or_past_a_limit_is_rejected_cleanly() {
        let f = setup();
        let err = update(
            &f.db,
            &f.clock,
            "nope",
            NotePatch {
                title: Some("x".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::NotFound);
        let note = make(&f, "t", "b").await;
        let err = update(
            &f.db,
            &f.clock,
            &note.id,
            NotePatch {
                body: Some("x".repeat(100_001)),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
        assert_eq!(err.field.as_deref(), Some("body"));
        assert_eq!(get(&f.db, &note.id).await.unwrap().body, "b");
    }

    // ---- pin and archive (R1-06, R1-07) --------------------------------------------------------

    #[tokio::test]
    async fn pinning_does_not_count_as_an_edit_and_is_idempotent() {
        let f = setup();
        let note = make(&f, "t", "b").await;
        let mut sub = f.bus.subscribe();
        f.clock.advance_ms(777);
        let pinned = set_pinned(&f.db, &f.clock, &note.id, true).await.unwrap();
        assert!(pinned.pinned);
        assert_eq!(pinned.edited_at, note.edited_at);
        assert_eq!(
            event(&mut sub).await,
            Event::NotePinnedChanged {
                at: f.clock.now_ms(),
                id: note.id.clone(),
                pinned: true
            }
        );
        let commits = f.db.committed_transactions();
        set_pinned(&f.db, &f.clock, &note.id, true).await.unwrap();
        assert_eq!(
            f.db.committed_transactions(),
            commits,
            "pinning an already-pinned note writes nothing"
        );
    }

    #[tokio::test]
    async fn archiving_moves_a_note_between_views_without_editing_it() {
        let f = setup();
        let note = make(&f, "t", "b").await;
        let mut sub = f.bus.subscribe();
        f.clock.advance_ms(1);
        let archived = set_archived(&f.db, &f.clock, &note.id, true).await.unwrap();
        assert_eq!(archived.edited_at, note.edited_at);
        assert_eq!(
            event(&mut sub).await,
            Event::NoteArchivedChanged {
                at: f.clock.now_ms(),
                id: note.id.clone(),
                archived: true
            }
        );
        assert!(list(&f.db, false, None, NoteSort::default())
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            list(&f.db, true, None, NoteSort::default())
                .await
                .unwrap()
                .len(),
            1
        );
        set_archived(&f.db, &f.clock, &note.id, false)
            .await
            .unwrap();
        assert_eq!(
            list(&f.db, false, None, NoteSort::default())
                .await
                .unwrap()
                .len(),
            1,
            "restored"
        );
    }

    #[tokio::test]
    async fn a_missing_note_cannot_be_pinned_or_archived() {
        let f = setup();
        assert_eq!(
            set_pinned(&f.db, &f.clock, "nope", true)
                .await
                .unwrap_err()
                .code,
            ErrorCode::NotFound
        );
        assert_eq!(
            set_archived(&f.db, &f.clock, "nope", true)
                .await
                .unwrap_err()
                .code,
            ErrorCode::NotFound
        );
    }

    // ---- the Bin (R1-05) -----------------------------------------------------------------------

    const DAY: i64 = 86_400_000;

    async fn binned(f: &Fixture) -> Vec<String> {
        list_bin(&f.db)
            .await
            .unwrap()
            .into_iter()
            .map(|n| n.id)
            .collect()
    }

    #[tokio::test]
    async fn delete_moves_a_note_to_the_bin_and_restore_brings_back_the_identical_note() {
        let f = setup();
        let work = labels::create(&f.db, &f.clock, "work").await.unwrap();
        let note = create(
            &f.db,
            &f.clock,
            NoteInput {
                color: NoteColor::Blue,
                label_ids: vec![work.id.clone()],
                ..input("Plan", "details")
            },
        )
        .await
        .unwrap();
        set_pinned(&f.db, &f.clock, &note.id, true).await.unwrap();
        let before = get(&f.db, &note.id).await.unwrap();
        let mut sub = f.bus.subscribe();

        f.clock.advance_ms(2_000);
        let deleted = delete(&f.db, &f.clock, &note.id).await.unwrap();
        assert_eq!(
            deleted, before,
            "the note comes back as it was, labels included"
        );
        assert_eq!(
            event(&mut sub).await,
            Event::NoteDeleted {
                at: f.clock.now_ms(),
                id: note.id.clone()
            }
        );
        assert_eq!(
            get(&f.db, &note.id).await.unwrap(),
            before,
            "get still works by id"
        );
        assert!(list(&f.db, false, None, NoteSort::default())
            .await
            .unwrap()
            .is_empty());
        let bin = list_bin(&f.db).await.unwrap();
        assert_eq!(bin.len(), 1);
        assert_eq!(
            (bin[0].id.as_str(), bin[0].deleted_at),
            (note.id.as_str(), f.clock.now_ms())
        );
        assert_eq!(
            bin[0].labels,
            vec![work.clone()],
            "label links are kept in the Bin"
        );

        f.clock.advance_ms(1_000);
        let restored = restore_from_bin(&f.db, &f.clock, &note.id).await.unwrap();
        assert_eq!(
            restored, before,
            "same id, timestamps, pin, colour and labels"
        );
        assert_eq!(
            event(&mut sub).await,
            Event::NoteRestored {
                at: f.clock.now_ms(),
                note: before
            }
        );
        assert!(binned(&f).await.is_empty());
        assert_eq!(
            list(&f.db, false, None, NoteSort::default())
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn binning_or_restoring_twice_changes_nothing() {
        let f = setup();
        let note = make(&f, "t", "b").await;
        restore_from_bin(&f.db, &f.clock, &note.id).await.unwrap();
        delete(&f.db, &f.clock, &note.id).await.unwrap();
        let commits = f.db.committed_transactions();
        let mut sub = f.bus.subscribe();
        delete(&f.db, &f.clock, &note.id).await.unwrap();
        assert_eq!(f.db.committed_transactions(), commits + 1);
        restore_from_bin(&f.db, &f.clock, &note.id).await.unwrap();
        restore_from_bin(&f.db, &f.clock, &note.id).await.unwrap();
        assert!(matches!(event(&mut sub).await, Event::NoteRestored { .. }));
        f.bus.publish(Event::AppReady {
            at: 0,
            startup_ms: 0,
        });
        assert!(
            matches!(event(&mut sub).await, Event::AppReady { .. }),
            "no extra events"
        );
    }

    #[tokio::test]
    async fn binned_notes_are_hidden_from_the_list_search_and_label_counts() {
        let f = setup();
        let work = labels::create(&f.db, &f.clock, "work").await.unwrap();
        let note = create(
            &f.db,
            &f.clock,
            NoteInput {
                label_ids: vec![work.id.clone()],
                ..input("findable", "")
            },
        )
        .await
        .unwrap();
        assert_eq!(labels::list(&f.db).await.unwrap()[0].count, 1);
        delete(&f.db, &f.clock, &note.id).await.unwrap();
        assert_eq!(labels::list(&f.db).await.unwrap()[0].count, 0);
        assert!(list(&f.db, false, Some(&work.id), NoteSort::default())
            .await
            .unwrap()
            .is_empty());
        assert!(search(&f.db, "findable", false).await.unwrap().is_empty());
        restore_from_bin(&f.db, &f.clock, &note.id).await.unwrap();
        assert_eq!(labels::list(&f.db,).await.unwrap()[0].count, 1);
        assert_eq!(search(&f.db, "findable", false).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_binned_note_cannot_be_edited_pinned_or_archived_until_restored() {
        let f = setup();
        let note = make(&f, "t", "b").await;
        delete(&f.db, &f.clock, &note.id).await.unwrap();
        let edit = update(
            &f.db,
            &f.clock,
            &note.id,
            NotePatch {
                title: Some("x".into()),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(edit.unwrap_err().code, ErrorCode::NotFound);
        assert_eq!(
            set_pinned(&f.db, &f.clock, &note.id, true)
                .await
                .unwrap_err()
                .code,
            ErrorCode::NotFound
        );
        assert_eq!(
            set_archived(&f.db, &f.clock, &note.id, true)
                .await
                .unwrap_err()
                .code,
            ErrorCode::NotFound
        );
    }

    #[tokio::test]
    async fn the_bin_lists_the_most_recently_deleted_first() {
        let f = setup();
        let (a, b, c) = (
            make(&f, "a", "").await,
            make(&f, "b", "").await,
            make(&f, "c", "").await,
        );
        for n in [&b, &c, &a] {
            f.clock.advance_ms(100);
            delete(&f.db, &f.clock, &n.id).await.unwrap();
        }
        assert_eq!(binned(&f).await, [a.id, c.id, b.id]);
    }

    #[tokio::test]
    async fn purge_deletes_only_binned_notes_for_good_and_says_so() {
        let f = setup();
        let note = make(&f, "t", "b").await;
        assert_eq!(
            purge(&f.db, &f.clock, &note.id).await.unwrap_err().code,
            ErrorCode::Conflict
        );
        assert_eq!(
            purge(&f.db, &f.clock, "nope").await.unwrap_err().code,
            ErrorCode::NotFound
        );
        delete(&f.db, &f.clock, &note.id).await.unwrap();
        let mut sub = f.bus.subscribe();
        purge(&f.db, &f.clock, &note.id).await.unwrap();
        assert_eq!(
            event(&mut sub).await,
            Event::NotePurged {
                at: f.clock.now_ms(),
                id: note.id.clone()
            }
        );
        assert_eq!(
            get(&f.db, &note.id).await.unwrap_err().code,
            ErrorCode::NotFound
        );
        assert_eq!(
            restore_from_bin(&f.db, &f.clock, &note.id)
                .await
                .unwrap_err()
                .code,
            ErrorCode::NotFound
        );
    }

    #[tokio::test]
    async fn emptying_the_bin_removes_every_binned_note_and_nothing_else() {
        let f = setup();
        let keep = make(&f, "keep", "").await;
        for t in ["a", "b", "c"] {
            let n = make(&f, t, "").await;
            delete(&f.db, &f.clock, &n.id).await.unwrap();
        }
        let mut sub = f.bus.subscribe();
        assert_eq!(empty_bin(&f.db, &f.clock).await.unwrap(), 3);
        for _ in 0..3 {
            assert!(matches!(event(&mut sub).await, Event::NotePurged { .. }));
        }
        assert_eq!(empty_bin(&f.db, &f.clock).await.unwrap(), 0);
        assert!(binned(&f).await.is_empty());
        assert!(get(&f.db, &keep.id).await.is_ok());
    }

    #[tokio::test]
    async fn notes_binned_more_than_thirty_days_ago_are_purged_and_newer_ones_stay() {
        let f = setup();
        let old = make(&f, "old", "").await;
        let edge = make(&f, "edge", "").await;
        let recent = make(&f, "recent", "").await;
        delete(&f.db, &f.clock, &old.id).await.unwrap(); // binned at t0
        f.clock.advance_ms(1);
        delete(&f.db, &f.clock, &edge.id).await.unwrap(); // binned at t0 + 1
        f.clock.advance_ms(10 * DAY);
        delete(&f.db, &f.clock, &recent.id).await.unwrap();
        // 30 days after `old` was binned exactly: not yet "more than" 30 days.
        f.clock
            .set_now_ms(f.db_deleted_at(&old.id).await + BIN_RETENTION_DAYS * DAY);
        assert_eq!(purge_expired(&f.db, &f.clock).await.unwrap(), 0);
        f.clock.advance_ms(1);
        let mut sub = f.bus.subscribe();
        assert_eq!(purge_expired(&f.db, &f.clock).await.unwrap(), 1);
        assert!(matches!(event(&mut sub).await, Event::NotePurged { id, .. } if id == old.id));
        assert_eq!(binned(&f).await.len(), 2);
        f.clock.advance_ms(DAY);
        assert_eq!(
            purge_expired(&f.db, &f.clock).await.unwrap(),
            1,
            "edge goes next"
        );
        assert_eq!(
            binned(&f).await,
            [recent.id],
            "the recently binned one is kept"
        );
    }

    #[tokio::test]
    async fn a_day_rollover_clears_the_bin_of_expired_notes() {
        let f = setup();
        let note = make(&f, "old", "").await;
        delete(&f.db, &f.clock, &note.id).await.unwrap();
        f.clock.advance_ms((BIN_RETENTION_DAYS + 1) * DAY);
        let mut sub = f.bus.subscribe();
        let janitor = spawn_bin_janitor(f.db.clone(), Arc::new(f.clock.clone()), &f.bus);
        f.bus.publish(Event::DayRolledOver {
            at: f.clock.now_ms(),
            ended_date: "2026-10-03".into(),
            new_date: "2026-10-04".into(),
        });
        loop {
            if matches!(event(&mut sub).await, Event::NotePurged { .. }) {
                break;
            }
        }
        assert!(binned(&f).await.is_empty());
        janitor.abort();
    }

    #[tokio::test]
    async fn deleting_a_missing_note_is_not_found() {
        let f = setup();
        assert_eq!(
            delete(&f.db, &f.clock, "nope").await.unwrap_err().code,
            ErrorCode::NotFound
        );
        assert_eq!(
            restore_from_bin(&f.db, &f.clock, "nope")
                .await
                .unwrap_err()
                .code,
            ErrorCode::NotFound
        );
    }

    // ---- search ---------------------------------------------------------------------------------

    fn names(v: &[NoteSummary]) -> Vec<&str> {
        v.iter().map(|n| n.title.as_str()).collect()
    }

    #[tokio::test]
    async fn search_matches_title_or_body_ignoring_case_newest_edited_first() {
        let f = setup();
        make(&f, "Groceries", "milk and eggs").await;
        make(&f, "MILKSHAKE plan", "").await;
        make(&f, "Unrelated", "nothing").await;
        make(&f, "Third", "Whole Milk").await;
        let found = search(&f.db, "  milk ", false).await.unwrap();
        assert_eq!(names(&found), ["Third", "MILKSHAKE plan", "Groceries"]);
    }

    #[tokio::test]
    async fn blank_queries_find_nothing() {
        let f = setup();
        make(&f, "a", "b").await;
        for q in ["", "   ", "\t\n"] {
            assert!(search(&f.db, q, false).await.unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn like_wildcards_in_the_query_are_matched_literally() {
        let f = setup();
        make(&f, "100% done", "").await;
        make(&f, "snake_case", "").await;
        make(&f, "back\\slash", "").await;
        make(&f, "plain", "xyz").await;
        assert_eq!(
            names(&search(&f.db, "%", false).await.unwrap()),
            ["100% done"]
        );
        assert_eq!(
            names(&search(&f.db, "_", false).await.unwrap()),
            ["snake_case"]
        );
        assert_eq!(
            names(&search(&f.db, "\\", false).await.unwrap()),
            ["back\\slash"]
        );
        assert!(search(&f.db, "x_z", false).await.unwrap().is_empty());
        assert!(search(&f.db, "p%n", false).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn search_honours_archived_and_carries_labels() {
        let f = setup();
        let work = labels::create(&f.db, &f.clock, "work").await.unwrap();
        let live = create(
            &f.db,
            &f.clock,
            NoteInput {
                label_ids: vec![work.id.clone()],
                ..input("alpha live", "")
            },
        )
        .await
        .unwrap();
        let old = make(&f, "alpha old", "").await;
        set_archived(&f.db, &f.clock, &old.id, true).await.unwrap();
        let in_notes = search(&f.db, "alpha", false).await.unwrap();
        assert_eq!(names(&in_notes), ["alpha live"]);
        assert_eq!(in_notes[0].id, live.id);
        assert_eq!(in_notes[0].labels, vec![work]);
        assert_eq!(
            names(&search(&f.db, "alpha", true).await.unwrap()),
            ["alpha old"]
        );
    }

    #[tokio::test]
    async fn search_returns_at_most_two_hundred_rows() {
        let f = setup();
        for i in 0..205 {
            f.clock.advance_ms(1);
            create(&f.db, &f.clock, input(&format!("match {i}"), ""))
                .await
                .unwrap();
        }
        let found = search(&f.db, "match", false).await.unwrap();
        assert_eq!(found.len(), SEARCH_LIMIT);
        assert_eq!(found[0].title, "match 204", "newest first");
    }

    #[tokio::test]
    async fn only_a_note_with_no_title_and_no_body_is_discarded_on_close() {
        let f = setup();
        let empty = create(&f.db, &f.clock, NoteInput::default()).await.unwrap();
        let titled = make(&f, "kept", "").await;
        let bodied = make(&f, "", "kept").await;
        assert!(discard_if_empty(&f.db, &f.clock, &empty.id).await.unwrap());
        assert!(!discard_if_empty(&f.db, &f.clock, &titled.id).await.unwrap());
        assert!(!discard_if_empty(&f.db, &f.clock, &bodied.id).await.unwrap());
        assert_eq!(
            list(&f.db, false, None, NoteSort::default())
                .await
                .unwrap()
                .len(),
            2
        );
    }

    // ---- the list (R1-03, R1-09) ---------------------------------------------------------------

    async fn titles(f: &Fixture, archived: bool, sort: NoteSort) -> Vec<String> {
        list(&f.db, archived, None, sort)
            .await
            .unwrap()
            .into_iter()
            .map(|n| n.title)
            .collect()
    }

    #[tokio::test]
    async fn pinned_notes_come_first_then_by_last_edited() {
        let f = setup();
        let a = make(&f, "A", "").await;
        let b = make(&f, "B", "").await;
        make(&f, "C", "").await;
        set_pinned(&f.db, &f.clock, &b.id, true).await.unwrap();
        assert_eq!(
            titles(&f, false, NoteSort::LastEdited).await,
            ["B", "C", "A"]
        );
        f.clock.advance_ms(100);
        update(
            &f.db,
            &f.clock,
            &a.id,
            NotePatch {
                body: Some("edited".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(
            titles(&f, false, NoteSort::LastEdited).await,
            ["B", "A", "C"],
            "editing A lifts it above C"
        );
    }

    #[tokio::test]
    async fn the_other_sort_orders_still_put_pinned_first() {
        let f = setup();
        let a = make(&f, "A", "").await;
        let b = make(&f, "B", "").await;
        let c = make(&f, "C", "").await;
        update(
            &f.db,
            &f.clock,
            &a.id,
            NotePatch {
                color: Some(NoteColor::Red),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        update(
            &f.db,
            &f.clock,
            &b.id,
            NotePatch {
                color: Some(NoteColor::Blue),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        update(
            &f.db,
            &f.clock,
            &c.id,
            NotePatch {
                color: Some(NoteColor::Blue),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        set_pinned(&f.db, &f.clock, &a.id, true).await.unwrap();
        assert_eq!(titles(&f, false, NoteSort::Created).await, ["A", "C", "B"]);
        assert_eq!(
            titles(&f, false, NoteSort::Color).await,
            ["A", "C", "B"],
            "A pinned; then blue (C edited after B) before nothing else"
        );
    }

    #[tokio::test]
    async fn the_archive_view_ignores_pinning() {
        let f = setup();
        let a = make(&f, "A", "").await;
        let b = make(&f, "B", "").await;
        set_pinned(&f.db, &f.clock, &a.id, true).await.unwrap();
        set_archived(&f.db, &f.clock, &a.id, true).await.unwrap();
        set_archived(&f.db, &f.clock, &b.id, true).await.unwrap();
        assert_eq!(
            titles(&f, true, NoteSort::LastEdited).await,
            ["B", "A"],
            "newest edit first; the pinned flag does nothing here"
        );
    }

    #[tokio::test]
    async fn a_card_shows_at_most_200_characters_of_the_body() {
        let f = setup();
        make(&f, "long", &"x".repeat(500)).await;
        make(&f, "wide", &"é".repeat(300)).await;
        let cards = list(&f.db, false, None, NoteSort::default()).await.unwrap();
        for card in cards {
            assert_eq!(
                card.excerpt.chars().count(),
                EXCERPT_CHARS,
                "{}",
                card.title
            );
        }
    }

    #[tokio::test]
    async fn filtering_by_label_shows_only_notes_with_it_and_cards_carry_their_labels() {
        let f = setup();
        let work = labels::create(&f.db, &f.clock, "work").await.unwrap();
        let home = labels::create(&f.db, &f.clock, "home").await.unwrap();
        f.clock.advance_ms(1);
        create(
            &f.db,
            &f.clock,
            NoteInput {
                label_ids: vec![work.id.clone(), home.id.clone()],
                ..input("both", "")
            },
        )
        .await
        .unwrap();
        f.clock.advance_ms(1);
        create(
            &f.db,
            &f.clock,
            NoteInput {
                label_ids: vec![home.id.clone()],
                ..input("home only", "")
            },
        )
        .await
        .unwrap();
        f.clock.advance_ms(1);
        make(&f, "none", "").await;

        let only_work = list(&f.db, false, Some(&work.id), NoteSort::default())
            .await
            .unwrap();
        assert_eq!(
            only_work
                .iter()
                .map(|n| n.title.as_str())
                .collect::<Vec<_>>(),
            ["both"]
        );
        assert_eq!(
            only_work[0].labels,
            vec![home.clone(), work.clone()],
            "cards list every label, sorted by name"
        );
        assert_eq!(
            list(&f.db, false, Some(&home.id), NoteSort::default())
                .await
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            list(&f.db, false, None, NoteSort::default())
                .await
                .unwrap()
                .len(),
            3
        );
    }

    #[tokio::test]
    async fn sidebar_label_counts_exclude_archived_notes() {
        let f = setup();
        let work = labels::create(&f.db, &f.clock, "work").await.unwrap();
        let live = create(
            &f.db,
            &f.clock,
            NoteInput {
                label_ids: vec![work.id.clone()],
                ..input("live", "")
            },
        )
        .await
        .unwrap();
        let old = create(
            &f.db,
            &f.clock,
            NoteInput {
                label_ids: vec![work.id.clone()],
                ..input("old", "")
            },
        )
        .await
        .unwrap();
        assert_eq!(labels::list(&f.db).await.unwrap()[0].count, 2);
        set_archived(&f.db, &f.clock, &old.id, true).await.unwrap();
        assert_eq!(labels::list(&f.db).await.unwrap()[0].count, 1);
        let _ = live;
    }

    #[tokio::test]
    async fn deleting_a_label_keeps_the_notes_that_had_it() {
        let f = setup();
        let work = labels::create(&f.db, &f.clock, "work").await.unwrap();
        let note = create(
            &f.db,
            &f.clock,
            NoteInput {
                label_ids: vec![work.id.clone()],
                ..input("keep me", "")
            },
        )
        .await
        .unwrap();
        labels::delete(&f.db, &f.clock, &work.id).await.unwrap();
        let after = get(&f.db, &note.id).await.unwrap();
        assert_eq!(after.title, "keep me");
        assert!(after.labels.is_empty());
        assert_eq!(
            after.edited_at, note.edited_at,
            "losing a label is not an edit of the note"
        );
    }
}
