//! The database handle: one writer thread, a small read pool (ADR-005, TRD §1).
//!
//! * **All writes** go through one OS thread that owns the only write connection. A write is a
//!   closure run inside a transaction; the events it returns are published on the bus only
//!   **after** `COMMIT`, so no subscriber ever hears about data that is not durable.
//! * **Reads** use two separate read-only connections via `spawn_blocking`; under WAL they never
//!   block the writer.
//! * Nothing here polls: the writer thread blocks on its channel and costs nothing when idle.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;

use rusqlite::{Connection, Transaction, TransactionBehavior};
use tokio::sync::{oneshot, OwnedSemaphorePermit, Semaphore};

use super::{open_and_migrate, open_connection};
use crate::bus::{EventBus, Recv};
use crate::error::{AppError, Result};
use crate::events::Event;

const READERS: usize = 2;

type Job = Box<dyn FnOnce(&mut Connection, &EventBus, &AtomicU64) + Send>;

enum Message {
    Run(Job),
    /// Queued behind every pending write, so a shutdown drains the queue first.
    Stop,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct Database {
    writer: Mutex<Option<Sender<Message>>>,
    thread: Mutex<Option<JoinHandle<()>>>,
    readers: Arc<Mutex<Vec<Connection>>>,
    permits: Arc<Semaphore>,
    commits: Arc<AtomicU64>,
}

impl Database {
    /// Open (creating and migrating if needed) the database at `path`.
    pub fn open(path: &Path, bus: EventBus) -> Result<Self> {
        let conn = open_and_migrate(path)?;
        let readers = (0..READERS)
            .map(|_| {
                let reader = open_connection(path)?;
                reader.pragma_update(None, "query_only", true)?;
                Ok(reader)
            })
            .collect::<Result<Vec<_>>>()?;

        let (tx, rx) = mpsc::channel::<Message>();
        let commits = Arc::new(AtomicU64::new(0));
        let thread_commits = Arc::clone(&commits);
        let thread = std::thread::Builder::new()
            .name("pluto-db-writer".into())
            .spawn(move || {
                let mut conn = conn;
                // `recv` blocks: an idle writer is parked, not polling.
                while let Ok(Message::Run(job)) = rx.recv() {
                    job(&mut conn, &bus, &thread_commits);
                }
            })
            .map_err(|_| AppError::internal("Pluto couldn't start its database thread."))?;

        Ok(Self {
            writer: Mutex::new(Some(tx)),
            thread: Mutex::new(Some(thread)),
            readers: Arc::new(Mutex::new(readers)),
            permits: Arc::new(Semaphore::new(READERS)),
            commits,
        })
    }

    /// Run `f` in one transaction on the writer thread. `f` returns the value for the caller and
    /// the events to publish; they are published only if the transaction commits. If `f` or the
    /// commit fails, everything rolls back, nothing is published, and the error comes back.
    pub async fn write<T, F>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Transaction) -> Result<(T, Vec<Event>)> + Send + 'static,
        T: Send + 'static,
    {
        let (reply, answer) = oneshot::channel();
        let job: Job = Box::new(move |conn, bus, commits| {
            let outcome = (|| {
                let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
                let produced = f(&tx)?; // an early return drops `tx`, which rolls back
                tx.commit()?;
                Ok(produced)
            })();
            let result = outcome.map(|(value, events): (T, Vec<Event>)| {
                commits.fetch_add(1, Ordering::Relaxed);
                for event in events {
                    bus.publish(event); // after COMMIT, never before
                }
                value
            });
            let _ = reply.send(result);
        });

        let sender = lock(&self.writer).clone();
        match sender {
            Some(tx) if tx.send(Message::Run(job)).is_ok() => {}
            _ => return Err(shutting_down()),
        }
        answer
            .await
            .map_err(|_| AppError::internal("Pluto stopped before that change finished."))?
    }

    /// Reclaim the space of deleted rows and fold the write-ahead log into the main file, on the
    /// writer thread (so it runs after every queued write). Used after "Delete all data", so the
    /// deleted text is not left behind in the file.
    pub async fn compact(&self) -> Result<()> {
        let (reply, answer) = oneshot::channel();
        let job: Job = Box::new(move |conn, _bus, _commits| {
            let result = conn
                .execute_batch("VACUUM; PRAGMA wal_checkpoint(TRUNCATE);")
                .map_err(AppError::from);
            let _ = reply.send(result);
        });
        let sender = lock(&self.writer).clone();
        match sender {
            Some(tx) if tx.send(Message::Run(job)).is_ok() => {}
            _ => return Err(shutting_down()),
        }
        answer
            .await
            .map_err(|_| AppError::internal("Pluto stopped before that finished."))?
    }

    /// Run `f` on a read-only connection. At most two reads run at once.
    pub async fn read<T, F>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let permit: OwnedSemaphorePermit = Arc::clone(&self.permits)
            .acquire_owned()
            .await
            .map_err(|_| AppError::internal("Pluto is shutting down."))?;
        let readers = Arc::clone(&self.readers);
        tokio::task::spawn_blocking(move || {
            let conn = lock(&readers)
                .pop()
                .expect("a permit guarantees a free reader");
            let result = f(&conn);
            lock(&readers).push(conn);
            drop(permit);
            result
        })
        .await
        .map_err(|_| AppError::internal("A read was interrupted."))?
    }

    /// Transactions committed so far. A debug counter for the idle-write budget (09).
    pub fn committed_transactions(&self) -> u64 {
        self.commits.load(Ordering::Relaxed)
    }

    /// Finish every queued write, then stop the writer. Later writes fail with `INTERNAL`.
    pub async fn shutdown(&self) {
        if let Some(tx) = lock(&self.writer).take() {
            let _ = tx.send(Message::Stop);
        }
        let thread = lock(&self.thread).take();
        if let Some(handle) = thread {
            let _ = tokio::task::spawn_blocking(move || handle.join()).await;
        }
    }

    /// Shut down when `AppShuttingDown` is published. The task ends once the writer has stopped.
    pub fn stop_on_shutdown(self: Arc<Self>, bus: &EventBus) -> tokio::task::JoinHandle<()> {
        let mut events = bus.subscribe();
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Recv::Event(Event::AppShuttingDown { .. }) | Recv::Closed => break,
                    Recv::Event(_) | Recv::Resync { .. } => {}
                }
            }
            self.shutdown().await;
        })
    }
}

fn shutting_down() -> AppError {
    AppError::internal("Pluto is shutting down, so that change wasn't saved.")
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc as std_mpsc;

    use serde_json::json;
    use tempfile::TempDir;
    use tokio::task::JoinSet;

    use super::*;
    use crate::error::ErrorCode;

    fn setting_changed(key: &str) -> Event {
        Event::SettingChanged {
            at: 0,
            key: key.into(),
            value: json!(true),
        }
    }

    fn insert_setting(
        key: &'static str,
    ) -> impl FnOnce(&Transaction) -> Result<((), Vec<Event>)> + Send + 'static {
        move |tx| {
            tx.execute(
                "INSERT INTO settings (key, value, updated_at) VALUES (?1, 'true', 0)",
                [key],
            )?;
            Ok(((), vec![setting_changed(key)]))
        }
    }

    async fn count(db: &Database, key: &'static str) -> i64 {
        db.read(move |c| {
            Ok(
                c.query_row("SELECT COUNT(*) FROM settings WHERE key = ?1", [key], |r| {
                    r.get(0)
                })?,
            )
        })
        .await
        .unwrap()
    }

    fn open() -> (TempDir, EventBus, Arc<Database>) {
        let dir = tempfile::tempdir().unwrap();
        let bus = EventBus::default();
        let db = Arc::new(Database::open(&dir.path().join("pluto.db"), bus.clone()).unwrap());
        (dir, bus, db)
    }

    #[tokio::test]
    async fn a_subscriber_that_reacts_to_an_event_always_finds_the_committed_row() {
        let (_dir, bus, db) = open();
        let mut sub = bus.subscribe();
        let observer = Arc::clone(&db);
        let watcher = tokio::spawn(async move {
            let mut seen = 0;
            while seen < 100 {
                if let Recv::Event(Event::SettingChanged { key, .. }) = sub.recv().await {
                    let k = key.clone();
                    let found: i64 = observer
                        .read(move |c| {
                            Ok(c.query_row(
                                "SELECT COUNT(*) FROM settings WHERE key = ?1",
                                [k],
                                |r| r.get(0),
                            )?)
                        })
                        .await
                        .unwrap();
                    assert_eq!(
                        found, 1,
                        "event for {key} arrived before its commit was visible"
                    );
                    seen += 1;
                }
            }
        });
        for i in 0..100 {
            let key: &'static str = Box::leak(format!("k{i}").into_boxed_str());
            db.write(insert_setting(key)).await.unwrap();
        }
        watcher.await.unwrap();
    }

    #[tokio::test]
    async fn a_failed_write_rolls_back_and_publishes_nothing() {
        let (_dir, bus, db) = open();
        let mut sub = bus.subscribe();
        let err = db
            .write(|tx| {
                tx.execute(
                    "INSERT INTO settings (key, value, updated_at) VALUES ('half', 'true', 0)",
                    [],
                )?;
                Err::<((), Vec<Event>), _>(AppError::validation("key", "nope"))
            })
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::Validation);
        assert_eq!(count(&db, "half").await, 0, "partial row must roll back");
        bus.publish(setting_changed("sentinel"));
        assert_eq!(
            sub.recv().await,
            Recv::Event(setting_changed("sentinel")),
            "no event may precede the sentinel"
        );
    }

    #[tokio::test]
    async fn a_commit_failure_is_reported_and_publishes_nothing() {
        let (_dir, bus, db) = open();
        let mut sub = bus.subscribe();
        db.write(insert_setting("dup")).await.unwrap();
        assert_eq!(sub.recv().await, Recv::Event(setting_changed("dup")));
        // Violates the primary key, so the statement fails inside the closure.
        let err = db.write(insert_setting("dup")).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::Db);
        bus.publish(setting_changed("sentinel"));
        assert_eq!(sub.recv().await, Recv::Event(setting_changed("sentinel")));
    }

    #[tokio::test]
    async fn the_transaction_counter_counts_commits_only() {
        let (_dir, _bus, db) = open();
        let before = db.committed_transactions(); // opening/migrating is not counted
        db.write(insert_setting("a")).await.unwrap();
        db.write(insert_setting("b")).await.unwrap();
        let _ = db.write(insert_setting("a")).await; // fails: duplicate key
        assert_eq!(db.committed_transactions() - before, 2);
    }

    #[tokio::test]
    async fn shutdown_finishes_every_queued_write_before_stopping() {
        let (_dir, _bus, db) = open();
        let mut writes = JoinSet::new();
        for i in 0..50 {
            let db = Arc::clone(&db);
            let key: &'static str = Box::leak(format!("q{i}").into_boxed_str());
            writes.spawn(async move { db.write(insert_setting(key)).await });
        }
        tokio::task::yield_now().await; // let every write enqueue itself
        db.shutdown().await;
        while let Some(done) = writes.join_next().await {
            done.unwrap()
                .expect("a write queued before shutdown must complete");
        }
        assert_eq!(db.committed_transactions(), 50);
    }

    #[tokio::test]
    async fn writes_after_shutdown_fail_cleanly() {
        let (_dir, _bus, db) = open();
        db.shutdown().await;
        let err = db.write(insert_setting("late")).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::Internal);
        assert!(err.message.contains("shutting down"));
    }

    #[tokio::test]
    async fn the_app_shutting_down_event_stops_the_writer() {
        let (_dir, bus, db) = open();
        let stopped = Arc::clone(&db).stop_on_shutdown(&bus);
        db.write(insert_setting("before")).await.unwrap();
        bus.publish(Event::AppShuttingDown { at: 0 });
        stopped.await.unwrap();
        assert!(db.write(insert_setting("after")).await.is_err());
        assert_eq!(db.committed_transactions(), 1);
    }

    #[tokio::test]
    async fn reads_are_not_blocked_while_a_write_is_in_flight() {
        let (_dir, _bus, db) = open();
        db.write(insert_setting("old")).await.unwrap();
        let (started_tx, started_rx) = std_mpsc::channel::<()>();
        let (go_tx, go_rx) = std_mpsc::channel::<()>();

        let writer = Arc::clone(&db);
        let slow_write = tokio::spawn(async move {
            writer
                .write(move |tx| {
                    tx.execute(
                        "INSERT INTO settings (key, value, updated_at) VALUES ('new', 'true', 0)",
                        [],
                    )?;
                    started_tx.send(()).unwrap();
                    go_rx.recv().unwrap(); // hold the write transaction open
                    Ok(((), vec![]))
                })
                .await
        });
        tokio::task::spawn_blocking(move || started_rx.recv())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            count(&db, "old").await,
            1,
            "committed data stays readable mid-write"
        );
        assert_eq!(
            count(&db, "new").await,
            0,
            "uncommitted data is invisible to readers"
        );

        go_tx.send(()).unwrap();
        slow_write.await.unwrap().unwrap();
        assert_eq!(count(&db, "new").await, 1);
    }

    #[tokio::test]
    async fn read_connections_cannot_write() {
        let (_dir, _bus, db) = open();
        let err = db
            .read(|c| {
                Ok(c.execute(
                    "INSERT INTO settings (key, value, updated_at) VALUES ('x', 'true', 0)",
                    [],
                )?)
            })
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::Db);
        assert_eq!(count(&db, "x").await, 0);
    }

    #[tokio::test]
    async fn more_reads_than_connections_all_complete() {
        let (_dir, _bus, db) = open();
        db.write(insert_setting("r")).await.unwrap();
        let mut reads = JoinSet::new();
        for _ in 0..20 {
            let db = Arc::clone(&db);
            reads.spawn(async move { count(&db, "r").await });
        }
        while let Some(n) = reads.join_next().await {
            assert_eq!(n.unwrap(), 1);
        }
    }
}
