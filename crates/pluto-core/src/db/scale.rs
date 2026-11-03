//! Big-database checks (audit 2026-10-28). Size comes from `PLUTO_SCALE` (default 2,000 notes and
//! tasks, so the ordinary test run stays fast); the audit ran it at 50,000:
//! `PLUTO_SCALE=50000 cargo test -p pluto-core scale -- --nocapture`.

use std::sync::Arc;
use std::time::Instant;

use chrono_tz::Tz;
use rusqlite::Connection;

use super::migrate::{self, MIGRATIONS};
use super::*;
use crate::bus::EventBus;
use crate::clock::FakeClock;

fn scale() -> i64 {
    std::env::var("PLUTO_SCALE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2_000)
}

/// `n` notes (2 KB bodies), `n` tasks, `5n` app sessions and `n` site sessions. Call it inside a transaction (it is much faster).
fn fill(conn: &Connection, n: i64) {
    conn.execute(
        "WITH RECURSIVE s(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM s WHERE i < ?1)
         INSERT INTO notes (id, title, body, pinned, archived, created_at, edited_at)
         SELECT 'n' || i, 'Note ' || i, substr(replace(hex(zeroblob(1000)), '00', 'lorem ipsum '), 1, 2000),
                i % 50 = 0, i % 20 = 0, 1760000000000 + i, 1760000000000 + i FROM s",
        [n],
    )
    .unwrap();
    conn.execute(
        "WITH RECURSIVE s(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM s WHERE i < ?1)
         INSERT INTO tasks (id, title, status, planned_date, created_at, updated_at, completed_at)
         SELECT 't' || i, 'Task ' || i, CASE WHEN i % 3 = 0 THEN 'COMPLETED' ELSE 'PLANNED' END,
                date('2026-01-01', '+' || (i % 300) || ' days'), 1760000000000 + i, 1760000000000 + i,
                CASE WHEN i % 3 = 0 THEN 1760000000000 + i END FROM s",
        [n],
    )
    .unwrap();
    conn.execute(
        "WITH RECURSIVE s(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM s WHERE i < ?1)
         INSERT INTO app_sessions (id, app, category, is_browser, started_at, ended_at)
         SELECT 'a' || i, 'App ' || (i % 40), 'Coding', i % 7 = 0, 1760000000000 + i * 60000, 1760000000000 + i * 60000 + 30000 FROM s",
        [n * 5],
    )
    .unwrap();
    conn.execute(
        "WITH RECURSIVE s(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM s WHERE i < ?1)
         INSERT INTO domain_sessions (id, browser, domain, started_at, ended_at)
         SELECT 'd' || i, 'Chrome', 'site' || (i % 200) || '.com', 1760000000000 + i * 60000, 1760000000000 + i * 60000 + 30000 FROM s",
        [n],
    )
    .unwrap();
}

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

#[test]
fn a_big_database_migrates_to_the_latest_version_with_nothing_lost_and_a_backup_made() {
    let n = scale();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pluto.db");
    {
        let mut conn = open_connection(&path).unwrap();
        migrate::migrate(&mut conn, &MIGRATIONS[..6]).unwrap();
        let tx = conn.transaction().unwrap();
        fill(&tx, n);
        tx.commit().unwrap();
    }
    let t = Instant::now();
    let mut conn = open_and_migrate(&path).unwrap();
    let took = t.elapsed();
    eprintln!(
        "scale {n}: open + migrate 6 -> {} took {took:?}",
        MIGRATIONS.len()
    );
    assert_eq!(
        migrate::user_version(&conn).unwrap(),
        MIGRATIONS.len() as u32
    );
    assert_eq!((count(&conn, "notes"), count(&conn, "tasks")), (n, n));
    assert_eq!(count(&conn, "app_sessions"), n * 5);
    assert!(
        path.with_file_name("pluto.db.bak-6").exists(),
        "the pre-migration backup"
    );
    quick_check(&conn).unwrap();
    // Opening it again is instant and writes no new backup.
    let again = Instant::now();
    migrate::migrate(&mut conn, MIGRATIONS).unwrap();
    assert!(again.elapsed().as_millis() < 500);
}

#[test]
fn the_screens_main_queries_stay_on_their_indexes_at_scale() {
    let n = scale();
    let mut conn = open_memory().unwrap();
    migrate::migrate(&mut conn, MIGRATIONS).unwrap();
    let tx = conn.transaction().unwrap();
    fill(&tx, n);
    tx.commit().unwrap();
    let plan = |sql: &str| -> String {
        let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
        stmt.query_map([], |r| r.get::<_, String>(3))
            .unwrap()
            .map(|r| r.unwrap())
            .collect::<Vec<_>>()
            .join(" | ")
    };
    for (what, sql, must) in [
        ("notes list", "SELECT id FROM notes WHERE archived = 0 ORDER BY pinned DESC, edited_at DESC", "idx_notes_active"),
        ("tasks by day", "SELECT id FROM tasks WHERE status = 'PLANNED' AND planned_date = '2026-03-01'", "idx_tasks_status_planned"),
        ("time by range", "SELECT app, SUM(ended_at - started_at) FROM app_sessions WHERE started_at BETWEEN 1 AND 2 GROUP BY app", "idx_app_sessions_started"),
        ("site time", "SELECT domain FROM domain_sessions WHERE domain = 'site1.com' AND started_at > 1", "idx_domain_sessions_domain"),
    ] {
        let p = plan(sql);
        assert!(p.contains(must), "{what} should use {must}, but the plan is: {p}");
    }
}

#[tokio::test]
async fn export_then_import_round_trips_a_big_database_exactly() {
    let n = scale();
    let dir = tempfile::tempdir().unwrap();
    let bus = EventBus::default();
    let clock = FakeClock::new(1_760_000_000_000, Tz::UTC);
    let db = Arc::new(Database::open(&dir.path().join("a.db"), bus.clone()).unwrap());
    db.write(move |tx| {
        fill(tx, n);
        Ok(((), Vec::new()))
    })
    .await
    .unwrap();
    let t = Instant::now();
    let doc = crate::data::export(&db, &clock, "test").await.unwrap();
    eprintln!("scale {n}: export took {:?}", t.elapsed());

    let other = Arc::new(Database::open(&dir.path().join("b.db"), bus).unwrap());
    let t = Instant::now();
    let rows = crate::data::import(&other, &clock, doc).await.unwrap();
    eprintln!("scale {n}: import took {:?}", t.elapsed());
    assert_eq!(rows["notes"], n as u64);
    assert_eq!(rows["tasks"], n as u64);
    assert_eq!(rows["app_sessions"], n as u64 * 5);
    let notes = other.read(|c| Ok(count(c, "notes"))).await.unwrap();
    assert_eq!(notes, n);
}
