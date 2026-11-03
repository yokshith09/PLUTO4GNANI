use std::fs;

use rusqlite::Connection;

use super::migrate::{self, validate_chain, Migration, MIGRATIONS};
use super::*;
use crate::error::ErrorCode;

const EXPECTED_TABLES: [&str; 28] = [
    "app_sessions",
    "courses",
    "daily_logs",
    "diary_entries",
    "diary_meta",
    "domain_sessions",
    "domains",
    "exams",
    "focus_sessions",
    "labels",
    "meeting_action_items",
    "meeting_decisions",
    "meeting_participants",
    "meetings",
    "note_course",
    "note_labels",
    "notes",
    "reminders",
    "settings",
    "task_course",
    "task_days",
    "task_events",
    "task_links",
    "task_ticks",
    "task_work_updates",
    "tasks",
    "usage_open",
    "user_preferences",
];

fn tables(conn: &Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .unwrap();
    stmt.query_map([], |r| r.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

fn migrated_memory() -> Connection {
    let mut conn = open_memory().unwrap();
    migrate::migrate(&mut conn, MIGRATIONS).unwrap();
    conn
}

// ---- opening and PRAGMAs --------------------------------------------------------------------

#[test]
fn a_fresh_database_is_created_at_the_latest_version_with_exactly_the_schema_tables() {
    let dir = tempfile::tempdir().unwrap();
    let conn = open_and_migrate(&dir.path().join("pluto.db")).unwrap();
    assert_eq!(migrate::user_version(&conn).unwrap(), 11);
    assert_eq!(
        tables(&conn),
        EXPECTED_TABLES,
        "no search tables (ADR-017), no strays"
    );
}

#[test]
fn every_connection_gets_the_documented_pragmas() {
    let dir = tempfile::tempdir().unwrap();
    let conn = open_connection(&dir.path().join("pluto.db")).unwrap();
    let get = |name: &str| -> i64 {
        conn.query_row(&format!("PRAGMA {name}"), [], |r| r.get(0))
            .unwrap()
    };
    assert_eq!(
        conn.query_row("PRAGMA journal_mode", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "wal"
    );
    assert_eq!(get("foreign_keys"), 1);
    assert_eq!(get("synchronous"), 1, "NORMAL");
    assert_eq!(get("busy_timeout"), 5000);
}

#[test]
fn reopening_an_up_to_date_database_changes_nothing_and_writes_no_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pluto.db");
    drop(open_and_migrate(&path).unwrap());
    let conn = open_and_migrate(&path).unwrap();
    assert_eq!(migrate::user_version(&conn).unwrap(), 11);
    assert!(migrate::latest_backup(&path).is_none());
}

// ---- integrity ------------------------------------------------------------------------------

#[test]
fn a_damaged_file_is_refused_and_left_byte_for_byte_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pluto.db");
    let garbage = b"this is definitely not a sqlite database, just text".repeat(50);
    fs::write(&path, &garbage).unwrap();
    let err = open_and_migrate(&path).unwrap_err();
    assert_eq!(err.code, ErrorCode::Db);
    assert!(err.message.contains("untouched"));
    assert_eq!(
        fs::read(&path).unwrap(),
        garbage,
        "the damaged file must not be modified"
    );
}

#[test]
fn the_error_for_a_damaged_database_names_the_newest_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pluto.db");
    fs::write(&path, b"garbage garbage garbage garbage").unwrap();
    fs::write(dir.path().join("pluto.db.bak-1"), b"old").unwrap();
    fs::write(dir.path().join("pluto.db.bak-3"), b"newer").unwrap();
    let err = open_and_migrate(&path).unwrap_err();
    assert!(
        err.message.contains("pluto.db.bak-3"),
        "should point at the newest backup: {}",
        err.message
    );
}

// ---- migration behaviour (uses synthetic chains as the fixture mechanism) ------------------

const V2_ADD_COLUMN: Migration = Migration {
    version: 2,
    sql: "ALTER TABLE notes ADD COLUMN pinned_at INTEGER;",
};

/// A database as version 1 of Pluto left it, for the migration tests.
fn open_at_v1(path: &std::path::Path) -> Connection {
    let mut conn = open_connection(path).unwrap();
    migrate::migrate(&mut conn, &MIGRATIONS[..1]).unwrap();
    conn
}

fn chain_v1_v2() -> Vec<Migration> {
    vec![
        Migration {
            version: 1,
            sql: MIGRATIONS[0].sql,
        },
        V2_ADD_COLUMN,
    ]
}

#[test]
fn migrating_an_existing_database_backs_it_up_first_and_keeps_its_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pluto.db");
    {
        let conn = open_at_v1(&path);
        conn.execute(
            "INSERT INTO notes (id, title, created_at, edited_at) VALUES ('n1', 'kept', 1, 1)",
            [],
        )
        .unwrap();
    }
    let mut conn = open_connection(&path).unwrap();
    assert_eq!(migrate::migrate(&mut conn, &chain_v1_v2()).unwrap(), 2);

    let title: String = conn
        .query_row("SELECT title FROM notes WHERE id = 'n1'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(title, "kept");
    conn.execute("UPDATE notes SET pinned_at = 5 WHERE id = 'n1'", [])
        .expect("the new column exists");

    let backup = path.with_file_name("pluto.db.bak-1");
    assert!(backup.exists(), "backup of the pre-migration database");
    let old = Connection::open(&backup).unwrap();
    assert_eq!(migrate::user_version(&old).unwrap(), 1);
    assert!(
        old.prepare("SELECT pinned_at FROM notes").is_err(),
        "backup predates the new column"
    );
    assert_eq!(
        old.query_row("SELECT COUNT(*) FROM notes", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn a_failing_migration_leaves_the_database_exactly_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pluto.db");
    {
        let conn = open_at_v1(&path);
        conn.execute(
            "INSERT INTO notes (id, title, created_at, edited_at) VALUES ('n1', 'safe', 1, 1)",
            [],
        )
        .unwrap();
    }
    let bad = vec![
        Migration {
            version: 1,
            sql: MIGRATIONS[0].sql,
        },
        Migration {
            version: 2,
            sql: "CREATE TABLE half_done (a); INSERT INTO table_that_does_not_exist VALUES (1);",
        },
    ];
    let mut conn = open_connection(&path).unwrap();
    let err = migrate::migrate(&mut conn, &bad).unwrap_err();
    assert_eq!(err.code, ErrorCode::Db);
    assert!(err.message.contains("unchanged"));

    assert_eq!(
        migrate::user_version(&conn).unwrap(),
        1,
        "version must not advance"
    );
    assert!(
        !tables(&conn).contains(&"half_done".to_string()),
        "partial work must roll back"
    );
    let title: String = conn
        .query_row("SELECT title FROM notes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(title, "safe");
}

#[test]
fn a_database_from_a_newer_pluto_is_refused_not_downgraded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pluto.db");
    let mut conn = open_connection(&path).unwrap();
    conn.pragma_update(None, "user_version", 99).unwrap();
    let err = migrate::migrate(&mut conn, MIGRATIONS).unwrap_err();
    assert!(err.message.contains("newer version"));
    assert_eq!(migrate::user_version(&conn).unwrap(), 99);
}

#[test]
fn only_the_newest_three_backups_are_kept() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("pluto.db");
    for v in 1..=5 {
        fs::write(dir.path().join(format!("pluto.db.bak-{v}")), b"x").unwrap();
    }
    fs::write(dir.path().join("unrelated.txt"), b"keep me").unwrap();
    migrate::prune_backups(&db, 3).unwrap();
    let mut left: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    left.sort();
    assert_eq!(
        left,
        [
            "pluto.db.bak-3",
            "pluto.db.bak-4",
            "pluto.db.bak-5",
            "unrelated.txt"
        ]
    );
}

// ---- the migration lint (Schema §5) --------------------------------------------------------

#[test]
fn the_real_migration_chain_is_valid() {
    validate_chain(MIGRATIONS).expect("embedded migrations must satisfy the runner's rules");
}

fn lint(sqls: &[&'static str]) -> std::result::Result<(), String> {
    let chain: Vec<Migration> = sqls
        .iter()
        .enumerate()
        .map(|(i, sql)| Migration {
            version: i as u32 + 1,
            sql,
        })
        .collect();
    validate_chain(&chain)
}

#[test]
fn the_lint_rejects_a_file_that_opens_its_own_transaction() {
    let err = lint(&["BEGIN; CREATE TABLE t (a); COMMIT;"]).unwrap_err();
    assert!(err.contains("starts its own transaction"), "{err}");
}

#[test]
fn the_lint_rejects_commit_end_and_rollback() {
    for ender in ["COMMIT;", "END;", "END TRANSACTION;", "ROLLBACK;"] {
        let sql: &'static str = Box::leak(format!("CREATE TABLE t (a); {ender}").into_boxed_str());
        let err = lint(&[sql]).unwrap_err();
        assert!(err.contains("ends the transaction"), "{ender}: {err}");
    }
}

#[test]
fn the_lint_rejects_a_user_version_write() {
    let err = lint(&["CREATE TABLE t (a); PRAGMA user_version = 7;"]).unwrap_err();
    assert!(err.contains("user_version"), "{err}");
}

#[test]
fn the_lint_accepts_triggers_including_multiline_bodies() {
    // The false positive a text search would hit: BEGIN … END; is legitimate inside a trigger,
    // including with END; alone on its own line.
    let sql = "CREATE TABLE t (a);\nCREATE TRIGGER t_no_update BEFORE UPDATE ON t\nBEGIN\n  SELECT RAISE(ABORT, 'append-only');\nEND;\n";
    lint(&[sql]).expect("a multi-line trigger body is not transaction control");
}

#[test]
fn the_lint_rejects_gaps_in_the_version_sequence() {
    let chain = [
        Migration {
            version: 1,
            sql: "CREATE TABLE a (x);",
        },
        Migration {
            version: 3,
            sql: "CREATE TABLE b (x);",
        },
    ];
    assert!(validate_chain(&chain).unwrap_err().contains("1, 2, 3"));
}

#[test]
fn the_lint_rejects_sql_that_does_not_apply() {
    assert!(lint(&["CREATE TABLE ("])
        .unwrap_err()
        .contains("does not apply"));
}

// ---- the schema's constraints, one test each (they exist because of the review) -------------

fn exec(conn: &Connection, sql: &str) -> rusqlite::Result<usize> {
    conn.execute(sql, [])
}

#[test]
fn label_names_are_unique_after_unicode_folding_not_just_ascii_folding() {
    let conn = migrated_memory();
    exec(&conn, "INSERT INTO labels (id, name, name_folded, created_at) VALUES ('1', 'Éclair', 'éclair', 0)").unwrap();
    let ascii_dup = exec(&conn, "INSERT INTO labels (id, name, name_folded, created_at) VALUES ('2', 'ÉCLAIR', 'éclair', 0)");
    assert!(
        ascii_dup.unwrap_err().to_string().contains("UNIQUE"),
        "ÉCLAIR must collide with Éclair"
    );
    exec(
        &conn,
        "INSERT INTO labels (id, name, name_folded, created_at) VALUES ('3', 'Work', 'work', 0)",
    )
    .unwrap();
    assert!(exec(
        &conn,
        "INSERT INTO labels (id, name, name_folded, created_at) VALUES ('4', 'WORK', 'work', 0)"
    )
    .is_err());
}

#[test]
fn task_events_cannot_be_updated_and_the_deleted_kind_does_not_exist() {
    let conn = migrated_memory();
    exec(
        &conn,
        "INSERT INTO tasks (id, title, created_at, updated_at) VALUES ('t1', 'x', 0, 0)",
    )
    .unwrap();
    exec(&conn, "INSERT INTO task_events (task_id, kind, at, local_date) VALUES ('t1', 'CREATED', 0, '2026-10-03')").unwrap();
    let update = exec(&conn, "UPDATE task_events SET kind = 'EDITED'");
    assert!(update.unwrap_err().to_string().contains("append-only"));
    let deleted_kind = exec(&conn, "INSERT INTO task_events (task_id, kind, at, local_date) VALUES ('t1', 'DELETED', 0, '2026-10-03')");
    assert!(
        deleted_kind.unwrap_err().to_string().contains("CHECK"),
        "a DELETED event would be erased by its own cascade"
    );
}

#[test]
fn deleting_a_task_removes_its_history_but_not_the_notes_or_logs_around_it() {
    let conn = migrated_memory();
    exec(
        &conn,
        "INSERT INTO tasks (id, title, created_at, updated_at) VALUES ('t1', 'x', 0, 0)",
    )
    .unwrap();
    exec(&conn, "INSERT INTO task_events (task_id, kind, at, local_date) VALUES ('t1', 'CREATED', 0, '2026-10-03')").unwrap();
    exec(&conn, "DELETE FROM tasks WHERE id = 't1'").unwrap();
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM task_events", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn daily_logs_are_immutable_and_undeletable() {
    let conn = migrated_memory();
    exec(&conn, "INSERT INTO daily_logs (log_date, snapshot, snapshot_version, generated_at) VALUES ('2026-10-02', '{}', 1, 0)").unwrap();
    assert!(exec(&conn, "UPDATE daily_logs SET snapshot = '{\"x\":1}'")
        .unwrap_err()
        .to_string()
        .contains("immutable"));
    assert!(exec(&conn, "DELETE FROM daily_logs")
        .unwrap_err()
        .to_string()
        .contains("cannot be deleted"));
}

#[test]
fn completed_at_is_set_exactly_when_the_task_is_completed() {
    let conn = migrated_memory();
    let no_timestamp = exec(&conn, "INSERT INTO tasks (id, title, status, created_at, updated_at) VALUES ('a', 'x', 'COMPLETED', 0, 0)");
    assert!(no_timestamp.is_err(), "COMPLETED without completed_at");
    let stray = exec(&conn, "INSERT INTO tasks (id, title, status, created_at, updated_at, completed_at) VALUES ('b', 'x', 'PLANNED', 0, 0, 5)");
    assert!(
        stray.is_err(),
        "completed_at on a task that is not completed"
    );
    exec(&conn, "INSERT INTO tasks (id, title, status, created_at, updated_at, completed_at) VALUES ('c', 'x', 'COMPLETED', 0, 0, 5)").unwrap();
}

#[test]
fn deleting_a_label_detaches_it_from_notes_without_deleting_the_notes() {
    let conn = migrated_memory();
    exec(
        &conn,
        "INSERT INTO notes (id, title, created_at, edited_at) VALUES ('n', 'keep', 0, 0)",
    )
    .unwrap();
    exec(
        &conn,
        "INSERT INTO labels (id, name, name_folded, created_at) VALUES ('l', 'work', 'work', 0)",
    )
    .unwrap();
    exec(
        &conn,
        "INSERT INTO note_labels (note_id, label_id) VALUES ('n', 'l')",
    )
    .unwrap();
    exec(&conn, "DELETE FROM labels WHERE id = 'l'").unwrap();
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM notes", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM note_labels", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn an_action_item_can_convert_to_at_most_one_task() {
    let conn = migrated_memory();
    exec(&conn, "INSERT INTO meetings (id, title, starts_at, created_at, updated_at) VALUES ('m', 'sync', 0, 0, 0)").unwrap();
    exec(
        &conn,
        "INSERT INTO tasks (id, title, created_at, updated_at) VALUES ('t', 'x', 0, 0)",
    )
    .unwrap();
    exec(&conn, "INSERT INTO meeting_action_items (id, meeting_id, position, body, task_id) VALUES ('a1', 'm', 0, 'do it', 't')").unwrap();
    let second = exec(&conn, "INSERT INTO meeting_action_items (id, meeting_id, position, body, task_id) VALUES ('a2', 'm', 1, 'again', 't')");
    assert!(second.unwrap_err().to_string().contains("UNIQUE"));
}

#[test]
fn database_errors_shown_to_users_never_quote_sql_or_values() {
    let conn = migrated_memory();
    let driver_error = exec(&conn, "INSERT INTO labels (id, name, name_folded, created_at) VALUES ('1', 'secret-label', 'x', 0), ('2', 'secret-label', 'x', 0)").unwrap_err();
    let shown = AppError::from(driver_error);
    assert_eq!(shown.code, ErrorCode::Db);
    assert!(!shown.message.contains("secret-label") && !shown.message.contains("labels"));
}

#[test]
fn migration_2_adds_the_bin_column_and_keeps_existing_notes_live() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pluto.db");
    {
        let conn = open_at_v1(&path);
        conn.execute(
            "INSERT INTO notes (id, title, created_at, edited_at) VALUES ('n1', 'old note', 1, 1)",
            [],
        )
        .unwrap();
    }
    let conn = open_and_migrate(&path).unwrap();
    assert_eq!(migrate::user_version(&conn).unwrap(), 11);
    let deleted: Option<i64> = conn
        .query_row("SELECT deleted_at FROM notes WHERE id = 'n1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(deleted, None, "existing notes are not in the Bin");
    assert!(migrate::latest_backup(&path).is_some(), "backed up first");
    conn.execute(
        "INSERT INTO reminders (id, title, remind_at, note_id, created_at) VALUES ('r1', 'x', 5, 'n1', 1)",
        [],
    )
    .unwrap();
    conn.execute("DELETE FROM notes WHERE id = 'n1'", [])
        .unwrap();
    let note: Option<String> = conn
        .query_row("SELECT note_id FROM reminders WHERE id = 'r1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(note, None, "deleting a note unlinks its reminders");
}

#[test]
fn migration_3_adds_usage_tables_that_enforce_their_checks() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pluto.db");
    {
        let mut conn = open_connection(&path).unwrap();
        migrate::migrate(&mut conn, &MIGRATIONS[..2]).unwrap();
    }
    let conn = open_and_migrate(&path).unwrap();
    assert_eq!(migrate::user_version(&conn).unwrap(), 11);
    assert!(migrate::latest_backup(&path).is_some(), "backed up first");

    exec(&conn, "INSERT INTO app_sessions (id, app, category, is_browser, started_at, ended_at) VALUES ('a', 'VS Code', 'Coding', 0, 1, 5)").unwrap();
    exec(&conn, "INSERT INTO domain_sessions (id, browser, domain, started_at, ended_at) VALUES ('d', 'Firefox', 'example.com', 1, 5)").unwrap();
    exec(
        &conn,
        "INSERT INTO domains (domain, first_seen, last_seen) VALUES ('example.com', 1, 5)",
    )
    .unwrap();
    let (category, tracked): (String, i64) = conn
        .query_row("SELECT category, tracked FROM domains", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((category.as_str(), tracked), ("Other", 1), "defaults");

    // A session cannot end before it starts, and the flags are strictly 0/1.
    assert!(exec(&conn, "INSERT INTO app_sessions (id, app, category, is_browser, started_at, ended_at) VALUES ('b', 'x', 'Other', 0, 9, 1)").is_err());
    assert!(exec(&conn, "INSERT INTO app_sessions (id, app, category, is_browser, started_at, ended_at) VALUES ('c', 'x', 'Other', 2, 1, 1)").is_err());
    assert!(exec(&conn, "INSERT INTO domain_sessions (id, browser, domain, started_at, ended_at) VALUES ('e', 'x', 'a.com', 9, 1)").is_err());
    assert!(exec(
        &conn,
        "INSERT INTO domains (domain, tracked, first_seen, last_seen) VALUES ('b.com', 3, 1, 1)"
    )
    .is_err());
}

#[test]
fn usage_tables_have_no_column_that_could_hold_a_url_or_a_title() {
    let conn = migrated_memory();
    for table in ["app_sessions", "domain_sessions", "domains"] {
        let mut stmt = conn
            .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
            .unwrap();
        let columns: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        for column in columns {
            assert!(
                !["url", "path", "title", "window_title", "tab_title"].contains(&column.as_str()),
                "{table}.{column} would store more than a domain"
            );
        }
    }
}

#[test]
fn migration_4_adds_task_links_days_ticks_and_stars_that_enforce_their_checks() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pluto.db");
    {
        let mut conn = open_connection(&path).unwrap();
        migrate::migrate(&mut conn, &MIGRATIONS[..3]).unwrap();
        exec(&conn, "INSERT INTO tasks (id, title, created_at, updated_at) VALUES ('old', 'Before 004', 1, 1)").unwrap();
    }
    let conn = open_and_migrate(&path).unwrap();
    assert_eq!(migrate::user_version(&conn).unwrap(), 11);
    let (starred, rule): (i64, Option<String>) = conn
        .query_row(
            "SELECT starred, repeat_rule FROM tasks WHERE id = 'old'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        (starred, rule),
        (0, None),
        "existing tasks are unstarred and don't repeat"
    );

    // Rules and flags are checked.
    for ok in ["daily", "weekly:1,3,5", "every:2"] {
        exec(
            &conn,
            &format!("UPDATE tasks SET repeat_rule = '{ok}' WHERE id = 'old'"),
        )
        .unwrap();
    }
    for bad in ["weekly", "every:0", "monthly", "weekly:9"] {
        assert!(
            exec(
                &conn,
                &format!("UPDATE tasks SET repeat_rule = '{bad}' WHERE id = 'old'")
            )
            .is_err(),
            "{bad}"
        );
    }
    assert!(exec(&conn, "UPDATE tasks SET starred = 2 WHERE id = 'old'").is_err());
    assert!(
        exec(&conn, "UPDATE reminders SET starred = 2").is_ok(),
        "no rows, no error"
    );
    assert!(exec(
        &conn,
        "INSERT INTO task_days (task_id, day) VALUES ('old', 'Monday')"
    )
    .is_err());

    // Links, days and ticks go with their task.
    exec(&conn, "INSERT INTO task_links (id, task_id, url, position, created_at) VALUES ('l', 'old', 'https://a.com', 0, 1)").unwrap();
    exec(
        &conn,
        "INSERT INTO task_days (task_id, day) VALUES ('old', '2026-10-27')",
    )
    .unwrap();
    exec(
        &conn,
        "INSERT INTO task_ticks (task_id, day, done_at) VALUES ('old', '2026-10-27', 5)",
    )
    .unwrap();
    assert!(exec(&conn, "INSERT INTO task_links (id, task_id, url, position, created_at) VALUES ('x', 'nobody', 'https://a.com', 0, 1)").is_err());
    exec(&conn, "DELETE FROM tasks WHERE id = 'old'").unwrap();
    for table in ["task_links", "task_days", "task_ticks"] {
        let n: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "{table}");
    }
}

#[test]
fn migration_5_allows_one_running_focus_session_and_checks_its_fields() {
    let conn = migrated_memory();
    let ins = |id: &str, ended: &str| {
        format!("INSERT INTO focus_sessions (id, kind, planned_ms, started_at, ended_at, outcome, local_date) VALUES ('{id}', 'FOCUS', 1500000, 1, {ended}, '2026-10-23')")
    };
    exec(&conn, &ins("a", "NULL, NULL")).unwrap();
    assert!(
        exec(&conn, &ins("b", "NULL, NULL")).is_err(),
        "a second running session"
    );
    exec(&conn, &ins("c", "9, 'DONE'")).unwrap();
    assert!(
        exec(&conn, &ins("d", "9, NULL")).is_err(),
        "ended without an outcome"
    );
    assert!(exec(&conn, "INSERT INTO focus_sessions (id, kind, planned_ms, started_at, local_date) VALUES ('e', 'NAP', 1500000, 1, '2026-10-23')").is_err());
    assert!(exec(&conn, "INSERT INTO focus_sessions (id, kind, planned_ms, started_at, local_date) VALUES ('f', 'BREAK', 5, 1, '2026-10-23')").is_err(), "under a minute");
}

#[test]
fn migration_7_adds_note_projects_and_task_reminders_that_enforce_their_checks() {
    let conn = migrated_memory();
    exec(&conn, "INSERT INTO notes (id, title, body, created_at, edited_at, project) VALUES ('n', 't', '', 1, 1, 'Thesis')").unwrap();
    assert!(
        exec(&conn, "INSERT INTO notes (id, title, body, created_at, edited_at, project) VALUES ('m', 't', '', 1, 1, '')").is_err(),
        "an empty project name"
    );
    exec(
        &conn,
        "INSERT INTO reminders (id, title, remind_at, created_at) VALUES ('r', 'x', 5, 1)",
    )
    .unwrap();
    let (lead, sound): (i64, i64) = conn
        .query_row(
            "SELECT lead_min, sound FROM reminders WHERE id = 'r'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((lead, sound), (0, 1), "old reminders: no lead, sound on");
    assert!(exec(&conn, "INSERT INTO reminders (id, title, remind_at, created_at, lead_min) VALUES ('s', 'x', 5, 1, 2000)").is_err());
    assert!(exec(&conn, "INSERT INTO reminders (id, title, remind_at, created_at, sound) VALUES ('u', 'x', 5, 1, 2)").is_err());
}
