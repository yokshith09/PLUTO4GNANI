//! Labels (PRD R1-08, R1-09).
//!
//! Names are unique ignoring case **and** Unicode form: the folded name (trimmed, NFC-normalised,
//! lower-cased) is the uniqueness key in the database, so `Éclair`, `ÉCLAIR` and `E` + combining
//! accent are one label. `name` keeps the casing the user typed, for display.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::clock::Clock;
use crate::db::Database;
use crate::error::{AppError, Result};
use crate::events::Event;
use crate::ids::new_id;

pub const NAME_MAX: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Label {
    pub id: String,
    pub name: String,
}

/// A label with how many of the non-archived, non-Bin notes carry it (the sidebar count).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct LabelCount {
    pub label: Label,
    pub count: u32,
}

/// The uniqueness key for a label name.
pub fn fold(name: &str) -> String {
    name.trim().nfc().collect::<String>().to_lowercase()
}

/// Trim and validate a typed name: 1–50 characters.
pub fn clean_name(name: &str) -> Result<String> {
    let name = name.trim();
    match name.chars().count() {
        0 => Err(AppError::validation("name", "Give the label a name.")),
        n if n > NAME_MAX => Err(AppError::validation(
            "name",
            format!("Label names can be up to {NAME_MAX} characters."),
        )),
        _ => Ok(name.to_owned()),
    }
}

pub(crate) fn all(conn: &Connection) -> Result<Vec<Label>> {
    let mut stmt = conn.prepare("SELECT id, name FROM labels ORDER BY name_folded")?;
    let labels = stmt
        .query_map([], |r| {
            Ok(Label {
                id: r.get(0)?,
                name: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(labels)
}

fn find_by_folded(conn: &Connection, folded: &str) -> Result<Option<Label>> {
    Ok(conn
        .query_row(
            "SELECT id, name FROM labels WHERE name_folded = ?1",
            [folded],
            |r| {
                Ok(Label {
                    id: r.get(0)?,
                    name: r.get(1)?,
                })
            },
        )
        .optional()?)
}

pub async fn list(db: &Database) -> Result<Vec<LabelCount>> {
    db.read(|conn| {
        let mut stmt = conn.prepare(
            "SELECT l.id, l.name,
                    (SELECT COUNT(*) FROM note_labels nl JOIN notes n ON n.id = nl.note_id
                      WHERE nl.label_id = l.id AND n.archived = 0 AND n.deleted_at IS NULL)
               FROM labels l ORDER BY l.name_folded",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(LabelCount {
                    label: Label {
                        id: r.get(0)?,
                        name: r.get(1)?,
                    },
                    count: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    })
    .await
}

/// Create a label, or return the existing one if the name is already taken in any casing.
pub async fn create(db: &Database, clock: &dyn Clock, name: &str) -> Result<Label> {
    let name = clean_name(name)?;
    let folded = fold(&name);
    let now = clock.now_ms();
    db.write(move |tx| {
        if let Some(existing) = find_by_folded(tx, &folded)? {
            return Ok((existing, vec![]));
        }
        let label = Label { id: new_id(), name };
        tx.execute(
            "INSERT INTO labels (id, name, name_folded, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![label.id, label.name, folded, now],
        )?;
        let event = Event::LabelsChanged {
            at: now,
            labels: all(tx)?,
        };
        Ok((label, vec![event]))
    })
    .await
}

/// Rename a label. Changing only the casing of its own name is fine; taking another label's name is a conflict.
pub async fn rename(db: &Database, clock: &dyn Clock, id: &str, name: &str) -> Result<Label> {
    let name = clean_name(name)?;
    let folded = fold(&name);
    let (id, now) = (id.to_owned(), clock.now_ms());
    db.write(move |tx| {
        if let Some(other) = find_by_folded(tx, &folded)? {
            if other.id != id {
                return Err(
                    AppError::conflict("A label with that name already exists.").with_field("name")
                );
            }
        }
        let changed = tx.execute(
            "UPDATE labels SET name = ?1, name_folded = ?2 WHERE id = ?3",
            params![name, folded, id],
        )?;
        if changed == 0 {
            return Err(AppError::not_found("That label no longer exists."));
        }
        let event = Event::LabelsChanged {
            at: now,
            labels: all(tx)?,
        };
        Ok((Label { id, name }, vec![event]))
    })
    .await
}

/// Delete a label. Notes keep existing; only the link to the label goes (R1-08).
pub async fn delete(db: &Database, clock: &dyn Clock, id: &str) -> Result<()> {
    let (id, now) = (id.to_owned(), clock.now_ms());
    db.write(move |tx| {
        if tx.execute("DELETE FROM labels WHERE id = ?1", [&id])? == 0 {
            return Err(AppError::not_found("That label no longer exists."));
        }
        Ok((
            (),
            vec![Event::LabelsChanged {
                at: now,
                labels: all(tx)?,
            }],
        ))
    })
    .await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono_tz::Tz;

    use super::*;
    use crate::bus::{EventBus, Recv};
    use crate::clock::FakeClock;
    use crate::error::ErrorCode;

    fn setup() -> (tempfile::TempDir, EventBus, Arc<Database>, FakeClock) {
        let dir = tempfile::tempdir().unwrap();
        let bus = EventBus::default();
        let db = Arc::new(Database::open(&dir.path().join("pluto.db"), bus.clone()).unwrap());
        (dir, bus, db, FakeClock::new(1_000, Tz::UTC))
    }

    #[test]
    fn folding_ignores_case_whitespace_and_unicode_form() {
        assert_eq!(fold("  Work "), "work");
        assert_eq!(fold("ÉCLAIR"), fold("éclair"));
        // 'É' as one code point vs 'E' followed by a combining acute accent.
        assert_eq!(fold("\u{00C9}clair"), fold("E\u{0301}clair"));
        assert_ne!(fold("work"), fold("works"));
    }

    #[test]
    fn names_are_trimmed_and_length_checked_in_characters_not_bytes() {
        assert_eq!(clean_name("  work ").unwrap(), "work");
        assert_eq!(
            clean_name("   ").unwrap_err().field.as_deref(),
            Some("name")
        );
        assert!(
            clean_name(&"é".repeat(50)).is_ok(),
            "50 characters is fine even though it is 100 bytes"
        );
        assert_eq!(
            clean_name(&"é".repeat(51)).unwrap_err().code,
            ErrorCode::Validation
        );
    }

    #[tokio::test]
    async fn creating_a_label_publishes_the_full_list() {
        let (_d, bus, db, clock) = setup();
        let mut sub = bus.subscribe();
        let label = create(&db, &clock, "Work").await.unwrap();
        match sub.recv().await {
            Recv::Event(Event::LabelsChanged { labels, .. }) => assert_eq!(labels, vec![label]),
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn creating_a_name_that_exists_in_any_casing_or_form_returns_the_existing_label() {
        let (_d, bus, db, clock) = setup();
        let first = create(&db, &clock, "Éclair").await.unwrap();
        let mut sub = bus.subscribe();
        for variant in ["éclair", "ÉCLAIR", "E\u{0301}clair", "  Éclair  "] {
            assert_eq!(
                create(&db, &clock, variant).await.unwrap(),
                first,
                "{variant:?}"
            );
        }
        assert_eq!(list(&db).await.unwrap().len(), 1);
        bus.publish(Event::AppShuttingDown { at: 0 });
        assert!(
            matches!(sub.recv().await, Recv::Event(Event::AppShuttingDown { .. })),
            "no event for a no-op create"
        );
    }

    #[tokio::test]
    async fn the_list_is_sorted_by_folded_name_not_by_typing_order() {
        let (_d, _b, db, clock) = setup();
        for name in ["work", "Alpha", "zebra", "Beta"] {
            create(&db, &clock, name).await.unwrap();
        }
        let names: Vec<_> = list(&db)
            .await
            .unwrap()
            .into_iter()
            .map(|c| c.label.name)
            .collect();
        assert_eq!(names, ["Alpha", "Beta", "work", "zebra"]);
    }

    #[tokio::test]
    async fn renaming_to_another_labels_name_is_a_conflict_but_recasing_your_own_is_fine() {
        let (_d, _b, db, clock) = setup();
        let work = create(&db, &clock, "work").await.unwrap();
        let home = create(&db, &clock, "home").await.unwrap();
        let err = rename(&db, &clock, &home.id, "WORK").await.unwrap_err();
        assert_eq!(err.code, ErrorCode::Conflict);
        assert_eq!(err.field.as_deref(), Some("name"));
        let recased = rename(&db, &clock, &work.id, "Work").await.unwrap();
        assert_eq!(recased.name, "Work");
        let names: Vec<_> = list(&db)
            .await
            .unwrap()
            .into_iter()
            .map(|c| c.label.name)
            .collect();
        assert_eq!(names, ["home", "Work"]);
    }

    #[tokio::test]
    async fn renaming_or_deleting_a_missing_label_is_not_found() {
        let (_d, _b, db, clock) = setup();
        assert_eq!(
            rename(&db, &clock, "nope", "x").await.unwrap_err().code,
            ErrorCode::NotFound
        );
        assert_eq!(
            delete(&db, &clock, "nope").await.unwrap_err().code,
            ErrorCode::NotFound
        );
    }

    #[tokio::test]
    async fn deleting_a_label_publishes_the_remaining_list() {
        let (_d, bus, db, clock) = setup();
        let work = create(&db, &clock, "work").await.unwrap();
        let home = create(&db, &clock, "home").await.unwrap();
        let mut sub = bus.subscribe();
        delete(&db, &clock, &work.id).await.unwrap();
        match sub.recv().await {
            Recv::Event(Event::LabelsChanged { labels, .. }) => assert_eq!(labels, vec![home]),
            other => panic!("{other:?}"),
        }
    }
}
