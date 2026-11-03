//! Export, import and delete-all (F0-11, ADR-012, R0-07 … R0-09).
//!
//! Your data is never trapped: [`export`] writes everything to one JSON document, [`import`]
//! restores it into an empty Pluto, and [`delete_all`] wipes the lot. The format is a plain dump of
//! every table by column name, so it grows with the schema without a second list to keep in step:
//!
//! ```json
//! { "format": "pluto-export", "format_version": 1, "schema_version": 3, "exported_at": 1760000000000,
//!   "app_version": "0.1.0", "tables": { "notes": [ { "id": "…", "title": "…", … } ], … } }
//! ```
//!
//! * **Export** reads every table inside one read transaction, so the file is one moment in time.
//! * **Import** is all-or-nothing: one write transaction, foreign keys checked, any bad row
//!   undoes the whole thing. It refuses a Pluto that already has notes, tasks and so on. Label
//!   `name_folded` is recomputed from `name`, never trusted from the file.
//! * **Delete all** needs the exact word `DELETE`, empties every table in one transaction (the
//!   append-only triggers are lifted for that transaction and put back), then compacts the file.

use std::collections::BTreeMap;

use rusqlite::types::{Value as Sql, ValueRef};
use rusqlite::{params_from_iter, Connection, Transaction};
use serde_json::{json, Map, Value};

use crate::clock::Clock;
use crate::db::{migrate, Database};
use crate::error::{AppError, Result};
use crate::events::Event;
use crate::labels;

pub const FORMAT: &str = "pluto-export";
pub const FORMAT_VERSION: u32 = 1;
/// The word that must be typed to delete everything.
pub const CONFIRM_WORD: &str = "DELETE";
/// Preferences, not work. An import may replace these even though the workspace is "empty".
const SETTINGS_TABLES: [&str; 2] = ["settings", "user_preferences"];
/// Scratch state that is neither work nor a setting: the crash journal of the stretch in front
/// (`usage_open`). Never exported, never blocks an import; "delete all" still clears it.
const TRANSIENT_TABLES: [&str; 1] = ["usage_open"];

fn user_tables(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    let names = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(names)
}

fn columns(conn: &Connection, table: &str) -> Result<Vec<String>> {
    // `table` always comes from `user_tables`, never from the file, and is quoted anyway.
    let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{table}\")"))?;
    let cols = stmt
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(cols)
}

fn to_json(v: ValueRef<'_>) -> Result<Value> {
    Ok(match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => json!(i),
        ValueRef::Real(f) => json!(f),
        ValueRef::Text(t) => Value::String(
            String::from_utf8(t.to_vec())
                .map_err(|_| AppError::internal("Pluto found text it can't export."))?,
        ),
        ValueRef::Blob(_) => return Err(AppError::internal("Pluto found data it can't export.")),
    })
}

/// Everything, as one document. Reads from one snapshot.
pub async fn export(db: &Database, clock: &dyn Clock, app_version: &str) -> Result<Value> {
    let (now, version) = (clock.now_ms(), app_version.to_owned());
    db.read(move |conn| {
        let snapshot = conn.unchecked_transaction()?; // a read transaction: one moment in time
        let mut tables = Map::new();
        for table in user_tables(&snapshot)?
            .into_iter()
            .filter(|t| !TRANSIENT_TABLES.contains(&t.as_str()))
        {
            let cols = columns(&snapshot, &table)?;
            let mut stmt = snapshot.prepare(&format!("SELECT * FROM \"{table}\""))?;
            let mut rows = Vec::new();
            let mut cursor = stmt.query([])?;
            while let Some(row) = cursor.next()? {
                let mut obj = Map::new();
                for (i, name) in cols.iter().enumerate() {
                    obj.insert(name.clone(), to_json(row.get_ref(i)?)?);
                }
                rows.push(Value::Object(obj));
            }
            tables.insert(table, Value::Array(rows));
        }
        let schema_version = migrate::user_version(&snapshot)?;
        Ok(json!({
            "format": FORMAT,
            "format_version": FORMAT_VERSION,
            "schema_version": schema_version,
            "exported_at": now,
            "app_version": version,
            "tables": tables,
        }))
    })
    .await
}

fn bad_file(message: impl Into<String>) -> AppError {
    AppError::validation("file", message)
}

fn scalar(v: &Value) -> Option<Sql> {
    match v {
        Value::Null => Some(Sql::Null),
        Value::Bool(b) => Some(Sql::Integer(i64::from(*b))),
        Value::Number(n) => n
            .as_i64()
            .map(Sql::Integer)
            .or_else(|| n.as_f64().map(Sql::Real)),
        Value::String(s) => Some(Sql::Text(s.clone())),
        Value::Array(_) | Value::Object(_) => None,
    }
}

/// Restore an export into an empty Pluto. Returns how many rows each table received.
pub async fn import(
    db: &Database,
    clock: &dyn Clock,
    document: Value,
) -> Result<BTreeMap<String, u64>> {
    let now = clock.now_ms();
    db.write(move |tx| {
        let counts = import_in(tx, &document)?;
        Ok((
            counts.clone(),
            vec![Event::DataImported { at: now, counts }],
        ))
    })
    .await
}

fn import_in(tx: &Transaction, doc: &Value) -> Result<BTreeMap<String, u64>> {
    if doc.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err(bad_file("That file isn't a Pluto export."));
    }
    if doc.get("format_version").and_then(Value::as_u64) != Some(u64::from(FORMAT_VERSION)) {
        return Err(bad_file(
            "That export is from a version of Pluto this one can't read.",
        ));
    }
    let current = migrate::user_version(tx)?;
    match doc.get("schema_version").and_then(Value::as_u64) {
        Some(v) if v <= u64::from(current) => {}
        Some(_) => {
            return Err(bad_file(
                "That export is from a newer Pluto. Update Pluto, then try again.",
            ))
        }
        None => return Err(bad_file("That export is missing its version.")),
    }
    let Some(file_tables) = doc.get("tables").and_then(Value::as_object) else {
        return Err(bad_file("That export has no data in it."));
    };

    let known = user_tables(tx)?;
    // Only an empty Pluto can be restored into, so nothing is ever merged or overwritten.
    for table in known.iter().filter(|t| {
        !SETTINGS_TABLES.contains(&t.as_str()) && !TRANSIENT_TABLES.contains(&t.as_str())
    }) {
        let n: i64 = tx.query_row(&format!("SELECT COUNT(*) FROM \"{table}\""), [], |r| {
            r.get(0)
        })?;
        if n > 0 {
            return Err(AppError::conflict(
                "Import only works into an empty Pluto. Delete all data first, then import.",
            ));
        }
    }
    for name in file_tables.keys() {
        if !known.contains(name) {
            return Err(bad_file(format!(
                "That export has a table Pluto doesn't know (“{name}”)."
            )));
        }
    }

    // The file may list rows in any order; foreign keys are checked at commit instead.
    tx.execute_batch("PRAGMA defer_foreign_keys = ON")?;
    let mut counts = BTreeMap::new();
    for table in &known {
        let Some(rows) = file_tables.get(table) else {
            continue;
        };
        let Some(rows) = rows.as_array() else {
            return Err(bad_file(format!(
                "The “{table}” part of that export is damaged."
            )));
        };
        let allowed = columns(tx, table)?;
        let verb = if SETTINGS_TABLES.contains(&table.as_str()) {
            "INSERT OR REPLACE"
        } else {
            "INSERT"
        };
        for (index, row) in rows.iter().enumerate() {
            let damaged = || {
                bad_file(format!(
                    "Row {} of “{table}” is damaged. Nothing was imported.",
                    index + 1
                ))
            };
            let Some(obj) = row.as_object() else {
                return Err(damaged());
            };
            let mut names: Vec<&str> = Vec::new();
            let mut values: Vec<Sql> = Vec::new();
            for (col, value) in obj {
                if !allowed.contains(col) {
                    return Err(damaged());
                }
                let mut v = scalar(value).ok_or_else(damaged)?;
                if table == "labels" && col == "name_folded" {
                    // Never trust the file: the uniqueness key comes from the name.
                    v = Sql::Text(labels::fold(
                        obj.get("name").and_then(Value::as_str).unwrap_or_default(),
                    ));
                }
                names.push(col);
                values.push(v);
            }
            if table == "labels" && !obj.contains_key("name_folded") {
                names.push("name_folded");
                values.push(Sql::Text(labels::fold(
                    obj.get("name").and_then(Value::as_str).unwrap_or_default(),
                )));
            }
            if names.is_empty() {
                return Err(damaged());
            }
            let sql = format!(
                "{verb} INTO \"{table}\" ({}) VALUES ({})",
                names
                    .iter()
                    .map(|n| format!("\"{n}\""))
                    .collect::<Vec<_>>()
                    .join(", "),
                vec!["?"; names.len()].join(", ")
            );
            // Rows of one table nearly always have the same columns, so the statement is reused.
            tx.prepare_cached(&sql)
                .and_then(|mut stmt| stmt.execute(params_from_iter(values)))
                .map_err(|_| damaged())?;
        }
        counts.insert(table.clone(), rows.len() as u64);
    }
    let broken: i64 = tx.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| {
        r.get(0)
    })?;
    if broken > 0 {
        return Err(bad_file(
            "Some items in that export point at things that aren't in it. Nothing was imported.",
        ));
    }
    Ok(counts)
}

/// Wipe everything: every note, task, reminder, log, tracked session and setting. Needs
/// [`CONFIRM_WORD`] exactly. The database is compacted afterwards so deleted text isn't left in the file.
pub async fn delete_all(db: &Database, clock: &dyn Clock, confirm: &str) -> Result<()> {
    if confirm != CONFIRM_WORD {
        return Err(AppError::validation(
            "confirm",
            format!("Type {CONFIRM_WORD} to confirm."),
        ));
    }
    let now = clock.now_ms();
    db.write(move |tx| {
        // Some tables refuse deletes on purpose (history is append-only). Lift those rules for
        // this one transaction, then put them back exactly as they were.
        let triggers: Vec<(String, String)> = {
            let mut stmt = tx.prepare(
                "SELECT name, sql FROM sqlite_master WHERE type = 'trigger' AND sql IS NOT NULL",
            )?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };
        for (name, _) in &triggers {
            tx.execute_batch(&format!("DROP TRIGGER \"{name}\""))?;
        }
        tx.execute_batch("PRAGMA defer_foreign_keys = ON")?;
        for table in user_tables(tx)? {
            tx.execute_batch(&format!("DELETE FROM \"{table}\""))?;
        }
        for (_, sql) in &triggers {
            tx.execute_batch(sql)?;
        }
        Ok(((), vec![Event::AllDataDeleted { at: now }]))
    })
    .await?;
    db.compact().await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::NaiveDate;
    use chrono_tz::Tz;

    use super::*;
    use crate::bus::{EventBus, Recv};
    use crate::clock::FakeClock;
    use crate::error::ErrorCode;
    use crate::notes::{self, NoteInput};
    use crate::reminders::{self, ReminderInput};
    use crate::tasks::{self, TaskInput, TaskStatus};
    use crate::{daily_log, settings_service::SettingsService};

    struct F {
        _dir: tempfile::TempDir,
        bus: EventBus,
        db: Arc<Database>,
        clock: FakeClock,
    }

    fn open(dir: &std::path::Path, bus: &EventBus) -> Arc<Database> {
        Arc::new(Database::open(&dir.join("pluto.db"), bus.clone()).unwrap())
    }

    fn setup() -> F {
        let dir = tempfile::tempdir().unwrap();
        let bus = EventBus::default();
        let db = open(dir.path(), &bus);
        let noon = chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2026, 10, 3, 12, 0, 0)
            .unwrap()
            .timestamp_millis();
        F {
            _dir: dir,
            bus,
            db,
            clock: FakeClock::new(noon, Tz::UTC),
        }
    }

    /// A little of everything, including a frozen daily log.
    async fn populate(f: &F) {
        let label = labels::create(&f.db, &f.clock, "Éclair").await.unwrap();
        let note = notes::create(
            &f.db,
            &f.clock,
            NoteInput {
                title: "Plan".into(),
                body: "# Hi".into(),
                label_ids: vec![label.id],
                ..Default::default()
            },
        )
        .await
        .unwrap();
        notes::set_pinned(&f.db, &f.clock, &note.id, true)
            .await
            .unwrap();
        let gone = notes::create(
            &f.db,
            &f.clock,
            NoteInput {
                title: "Binned".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        notes::delete(&f.db, &f.clock, &gone.id).await.unwrap();
        let task = tasks::create(
            &f.db,
            &f.clock,
            TaskInput {
                title: "Ship".into(),
                note_id: Some(note.id),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        tasks::add_work_update(&f.db, &f.clock, &task.id, "started")
            .await
            .unwrap();
        tasks::transition(&f.db, &f.clock, &task.id, TaskStatus::Completed)
            .await
            .unwrap();
        reminders::create(
            &f.db,
            &f.clock,
            ReminderInput {
                title: "Call".into(),
                remind_at: f.clock.now_ms() + 3_600_000,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        SettingsService::new(f.db.clone(), Arc::new(f.clock.clone()))
            .set_pref("ui.last_view", json!("notes"))
            .await
            .unwrap();
        f.clock.advance_ms(86_400_000);
        daily_log::freeze(
            &f.db,
            &f.clock,
            NaiveDate::from_ymd_opt(2026, 10, 3).unwrap(),
        )
        .await
        .unwrap();
    }

    async fn dump(f: &F) -> Value {
        let mut doc = export(&f.db, &f.clock, "test").await.unwrap();
        doc["exported_at"] = json!(0);
        doc
    }

    #[tokio::test]
    async fn export_delete_import_gives_back_identical_data() {
        let f = setup();
        populate(&f).await;
        let before = dump(&f).await;
        assert_eq!(before["format"], FORMAT);
        assert!(
            before["tables"]["notes"].as_array().unwrap().len() >= 2,
            "includes the binned note"
        );
        assert_eq!(before["tables"]["daily_logs"].as_array().unwrap().len(), 1);

        delete_all(&f.db, &f.clock, "DELETE").await.unwrap();
        let empty = dump(&f).await;
        for (name, rows) in empty["tables"].as_object().unwrap() {
            assert!(
                rows.as_array().unwrap().is_empty(),
                "{name} should be empty"
            );
        }

        let counts = import(&f.db, &f.clock, before.clone()).await.unwrap();
        assert!(counts["notes"] >= 2);
        assert_eq!(dump(&f).await, before, "table by table, row by row");
    }

    #[tokio::test]
    async fn append_only_rules_are_back_after_delete_all() {
        let f = setup();
        populate(&f).await;
        delete_all(&f.db, &f.clock, "DELETE").await.unwrap();
        let triggers: i64 =
            f.db.read(|c| {
                Ok(c.query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .await
            .unwrap();
        assert!(
            triggers >= 3,
            "task_events and daily_logs rules are restored"
        );
        // And the app still works: new data can be written.
        notes::create(
            &f.db,
            &f.clock,
            NoteInput {
                title: "Again".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn delete_all_needs_the_exact_word_and_publishes_after_it_succeeds() {
        let f = setup();
        populate(&f).await;
        let mut events = f.bus.subscribe();
        for wrong in ["", "delete", "DELETE ", "yes"] {
            let err = delete_all(&f.db, &f.clock, wrong).await.unwrap_err();
            assert_eq!(err.code, ErrorCode::Validation, "{wrong:?}");
        }
        assert!(
            !dump(&f).await["tables"]["notes"]
                .as_array()
                .unwrap()
                .is_empty(),
            "nothing was deleted"
        );
        delete_all(&f.db, &f.clock, "DELETE").await.unwrap();
        let mut saw = false;
        while let Ok(Some(Recv::Event(e))) =
            tokio::time::timeout(std::time::Duration::from_millis(200), events.recv())
                .await
                .map(Some)
        {
            saw |= matches!(e, Event::AllDataDeleted { .. });
            if saw {
                break;
            }
        }
        assert!(saw);
    }

    #[tokio::test]
    async fn import_refuses_a_workspace_that_has_data() {
        let f = setup();
        populate(&f).await;
        let doc = dump(&f).await;
        let err = import(&f.db, &f.clock, doc).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::Conflict);
    }

    #[tokio::test]
    async fn import_rejects_the_wrong_format_and_newer_schemas() {
        let f = setup();
        let mut doc = dump(&f).await;
        for (key, value) in [
            ("format", json!("something-else")),
            ("format_version", json!(2)),
            ("schema_version", json!(999)),
            ("tables", json!([])),
        ] {
            let mut bad = doc.clone();
            bad[key] = value;
            let err = import(&f.db, &f.clock, bad).await.unwrap_err();
            assert_eq!(err.code, ErrorCode::Validation, "{key}");
        }
        doc["tables"]["not_a_table"] = json!([]);
        assert_eq!(
            import(&f.db, &f.clock, doc).await.unwrap_err().code,
            ErrorCode::Validation
        );
    }

    #[tokio::test]
    async fn the_crash_journal_is_not_exported_and_does_not_block_an_import() {
        let f = setup();
        populate(&f).await;
        f.db.write(|tx| {
            tx.execute(
                "INSERT INTO usage_open (kind, name, category, is_browser, browser, started_at, last_seen)
                 VALUES ('app', 'VS Code', 'Coding', 0, '', 1000, 5000)",
                [],
            )?;
            Ok(((), vec![]))
        })
        .await
        .unwrap();
        let doc = dump(&f).await;
        assert!(
            doc["tables"].get("usage_open").is_none(),
            "scratch state is not part of a backup"
        );
        // Clearing everything empties it too, so an import into a fresh Pluto is allowed.
        delete_all(&f.db, &f.clock, "DELETE").await.unwrap();
        let n: i64 =
            f.db.read(|c| Ok(c.query_row("SELECT COUNT(*) FROM usage_open", [], |r| r.get(0))?))
                .await
                .unwrap();
        assert_eq!(n, 0);
        // And a journal row in the target does not make it "not empty".
        f.db.write(|tx| {
            tx.execute(
                "INSERT INTO usage_open (kind, name, category, is_browser, browser, started_at, last_seen)
                 VALUES ('app', 'Other', 'Other', 0, '', 1, 2)",
                [],
            )?;
            Ok(((), vec![]))
        })
        .await
        .unwrap();
        import(&f.db, &f.clock, doc).await.unwrap();
    }

    #[tokio::test]
    async fn one_bad_row_in_the_middle_imports_nothing() {
        let f = setup();
        populate(&f).await;
        let mut doc = dump(&f).await;
        delete_all(&f.db, &f.clock, "DELETE").await.unwrap();
        // Break the second task_events row: a column that doesn't exist.
        let notes_rows = doc["tables"]["notes"].as_array().unwrap().len();
        doc["tables"]["notes"].as_array_mut().unwrap()[notes_rows - 1]["no_such_column"] = json!(1);
        let err = import(&f.db, &f.clock, doc).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::Validation);
        let after = dump(&f).await;
        for (name, rows) in after["tables"].as_object().unwrap() {
            if !SETTINGS_TABLES.contains(&name.as_str()) {
                assert!(
                    rows.as_array().unwrap().is_empty(),
                    "{name} must still be empty"
                );
            }
        }
    }

    #[tokio::test]
    async fn rows_pointing_at_missing_things_are_refused() {
        let f = setup();
        populate(&f).await;
        let mut doc = dump(&f).await;
        delete_all(&f.db, &f.clock, "DELETE").await.unwrap();
        doc["tables"]["tasks"] = json!([]); // work updates and history now point at nothing
        assert_eq!(
            import(&f.db, &f.clock, doc).await.unwrap_err().code,
            ErrorCode::Validation
        );
    }

    #[tokio::test]
    async fn a_non_ascii_label_pair_cannot_sneak_in_as_a_duplicate() {
        let f = setup();
        let mut doc = dump(&f).await;
        doc["tables"]["labels"] = json!([
            { "id": "a", "name": "Éclair", "name_folded": "éclair", "created_at": 1 },
            { "id": "b", "name": "ÉCLAIR", "name_folded": "something else entirely", "created_at": 2 },
        ]);
        let err = import(&f.db, &f.clock, doc).await.unwrap_err();
        assert_eq!(
            err.code,
            ErrorCode::Validation,
            "the folded name is recomputed, so these collide"
        );
    }

    #[tokio::test]
    async fn importing_replaces_settings_rather_than_clashing_with_them() {
        let f = setup();
        populate(&f).await;
        let doc = dump(&f).await;
        delete_all(&f.db, &f.clock, "DELETE").await.unwrap();
        SettingsService::new(f.db.clone(), Arc::new(f.clock.clone()))
            .set_pref("ui.last_view", json!("today"))
            .await
            .unwrap();
        import(&f.db, &f.clock, doc).await.unwrap();
        let view = SettingsService::new(f.db.clone(), Arc::new(f.clock.clone()))
            .get_pref("ui.last_view")
            .await
            .unwrap();
        assert_eq!(view, Some(json!("notes")));
    }
}
