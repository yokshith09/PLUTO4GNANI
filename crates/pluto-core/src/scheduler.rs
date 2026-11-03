//! Day rollover (TRD §6.2): one sleep until the next local midnight, re-armed when the machine
//! wakes or the timezone changes. There is no polling loop; between wake-ups it costs nothing.
//!
//! On each wake it publishes one `DayRolledOver` per local day that has ended since
//! `last_seen_date`, in order, then records today. Building the daily-log snapshot is the
//! subscriber's job (F1-15); it must treat a repeated event as a no-op (`INSERT OR IGNORE`).

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use chrono::NaiveDate;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::bus::EventBus;
use crate::clock::{next_local_midnight_ms, Clock};
use crate::db::Database;
use crate::error::Result;
use crate::events::Event;

/// Where the last day we rolled over to is remembered between runs.
pub trait LastSeenStore: Send + Sync + 'static {
    fn get(&self) -> impl Future<Output = Result<Option<NaiveDate>>> + Send;
    fn set(&self, date: NaiveDate, at_ms: i64) -> impl Future<Output = Result<()>> + Send;
}

/// `user_preferences.last_seen_date`, a JSON string `"YYYY-MM-DD"`.
pub struct DbLastSeen(pub Arc<Database>);

const KEY: &str = "last_seen_date";

impl LastSeenStore for DbLastSeen {
    async fn get(&self) -> Result<Option<NaiveDate>> {
        self.0
            .read(|conn| {
                let mut stmt = conn.prepare("SELECT value FROM user_preferences WHERE key = ?1")?;
                let mut rows = stmt.query([KEY])?;
                let Some(row) = rows.next()? else {
                    return Ok(None);
                };
                let raw: String = row.get(0)?;
                let parsed: Option<String> = serde_json::from_str(&raw).ok();
                Ok(parsed.and_then(|s| s.parse().ok()))
            })
            .await
    }

    async fn set(&self, date: NaiveDate, at_ms: i64) -> Result<()> {
        let value = serde_json::to_string(&date.to_string()).unwrap_or_default();
        self.0
            .write(move |tx| {
                tx.execute(
                    "INSERT INTO user_preferences (key, value, updated_at) VALUES (?1, ?2, ?3)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                    rusqlite::params![KEY, value, at_ms],
                )?;
                Ok(((), vec![]))
            })
            .await
    }
}

/// `(ended, new)` for every local day that has ended since `last_seen`, oldest first.
/// Nothing on first launch (`None`), and nothing if the clock went backwards (a timezone change
/// to the west can make "today" earlier than the last day seen) — a day is never rolled twice.
pub fn pending_rollovers(
    last_seen: Option<NaiveDate>,
    today: NaiveDate,
) -> Vec<(NaiveDate, NaiveDate)> {
    let Some(last) = last_seen else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut day = last;
    while day < today {
        let next = day.succ_opt().unwrap_or(today);
        out.push((day, next));
        day = next;
    }
    out
}

/// One rollover check: publish what is owed, then remember today. Safe to call at any time and
/// as often as you like — a second call the same day publishes nothing.
pub async fn roll_over<S: LastSeenStore>(
    clock: &dyn Clock,
    store: &S,
    bus: &EventBus,
) -> Result<Vec<(NaiveDate, NaiveDate)>> {
    let today = clock.today();
    let last = store.get().await?;
    let owed = pending_rollovers(last, today);
    for (ended, new) in &owed {
        bus.publish(Event::DayRolledOver {
            at: clock.now_ms(),
            ended_date: ended.to_string(),
            new_date: new.to_string(),
        });
    }
    // Publish first, record second: if we die in between, the next start repeats the events,
    // which subscribers must tolerate, instead of silently losing a day.
    if last.is_none_or(|l| l < today) {
        store.set(today, clock.now_ms()).await?;
    }
    Ok(owed)
}

pub struct Scheduler {
    task: JoinHandle<()>,
    rearm: Arc<Notify>,
}

impl Scheduler {
    /// Check once now (catching up on any days missed while Pluto was closed), then sleep to each
    /// midnight in turn.
    pub fn spawn<S: LastSeenStore>(clock: Arc<dyn Clock>, store: S, bus: EventBus) -> Self {
        let rearm = Arc::new(Notify::new());
        let wake = Arc::clone(&rearm);
        let task = tokio::spawn(async move {
            loop {
                if let Err(error) = roll_over(&*clock, &store, &bus).await {
                    tracing::warn!(code = ?error.code, "day rollover check failed; will retry at the next wake");
                }
                let wait_ms = (next_local_midnight_ms(&*clock) - clock.now_ms()).max(0) as u64;
                tokio::select! {
                    () = tokio::time::sleep(Duration::from_millis(wait_ms)) => {}
                    () = wake.notified() => {}
                }
            }
        });
        Self { task, rearm }
    }

    /// Call when the OS resumes from sleep or reports a timezone change: the sleeping timer was
    /// set against a clock that has since jumped, so work out where we are now.
    pub fn rearm(&self) {
        self.rearm.notify_one();
    }

    pub fn stop(&self) {
        self.task.abort();
    }
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use chrono_tz::{America, Asia, Tz};

    use super::*;
    use crate::bus::{Recv, Subscriber};
    use crate::clock::FakeClock;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[derive(Clone, Default)]
    struct MemoryStore(Arc<Mutex<Option<NaiveDate>>>);

    impl MemoryStore {
        fn with(date: NaiveDate) -> Self {
            Self(Arc::new(Mutex::new(Some(date))))
        }
        fn value(&self) -> Option<NaiveDate> {
            *self.0.lock().unwrap()
        }
    }

    impl LastSeenStore for MemoryStore {
        async fn get(&self) -> Result<Option<NaiveDate>> {
            Ok(self.value())
        }
        async fn set(&self, date: NaiveDate, _at: i64) -> Result<()> {
            *self.0.lock().unwrap() = Some(date);
            Ok(())
        }
    }

    /// Everything currently queued for this subscriber, as `(ended, new)` pairs.
    async fn rollovers(sub: &mut Subscriber) -> Vec<(String, String)> {
        let mut out = Vec::new();
        // Publishing is synchronous, so by now everything owed is already queued; a sentinel
        // marks the end without any sleeping.
        loop {
            match sub.recv().await {
                Recv::Event(Event::DayRolledOver {
                    ended_date,
                    new_date,
                    ..
                }) => out.push((ended_date, new_date)),
                Recv::Event(Event::AppReady { .. }) => return out,
                other => panic!("unexpected {other:?}"),
            }
        }
    }

    fn end_marker(bus: &EventBus) {
        bus.publish(Event::AppReady {
            at: 0,
            startup_ms: 0,
        });
    }

    // ---- pure rule ---------------------------------------------------------------------------

    #[test]
    fn first_launch_owes_nothing() {
        assert!(pending_rollovers(None, d(2026, 10, 3)).is_empty());
    }

    #[test]
    fn the_same_day_owes_nothing() {
        assert!(pending_rollovers(Some(d(2026, 10, 3)), d(2026, 10, 3)).is_empty());
    }

    #[test]
    fn a_gap_of_several_days_owes_one_rollover_per_day_in_order() {
        let owed = pending_rollovers(Some(d(2026, 10, 1)), d(2026, 10, 4));
        assert_eq!(
            owed,
            vec![
                (d(2026, 10, 1), d(2026, 10, 2)),
                (d(2026, 10, 2), d(2026, 10, 3)),
                (d(2026, 10, 3), d(2026, 10, 4))
            ]
        );
    }

    #[test]
    fn a_clock_that_moved_backwards_owes_nothing() {
        assert!(pending_rollovers(Some(d(2026, 10, 4)), d(2026, 10, 3)).is_empty());
    }

    // ---- roll_over ---------------------------------------------------------------------------

    #[tokio::test]
    async fn closed_for_three_days_publishes_three_ordered_events_then_remembers_today() {
        let bus = EventBus::default();
        let mut sub = bus.subscribe();
        let clock = FakeClock::at_local(Tz::UTC, d(2026, 10, 4), 9, 0, 0);
        let store = MemoryStore::with(d(2026, 10, 1));
        roll_over(&clock, &store, &bus).await.unwrap();
        end_marker(&bus);
        assert_eq!(
            rollovers(&mut sub).await,
            [
                ("2026-10-01", "2026-10-02"),
                ("2026-10-02", "2026-10-03"),
                ("2026-10-03", "2026-10-04")
            ]
            .map(|(a, b)| (a.to_owned(), b.to_owned()))
        );
        assert_eq!(store.value(), Some(d(2026, 10, 4)));
    }

    #[tokio::test]
    async fn checking_twice_on_the_same_day_publishes_once() {
        let bus = EventBus::default();
        let mut sub = bus.subscribe();
        let clock = FakeClock::at_local(Tz::UTC, d(2026, 10, 4), 9, 0, 0);
        let store = MemoryStore::with(d(2026, 10, 3));
        roll_over(&clock, &store, &bus).await.unwrap();
        roll_over(&clock, &store, &bus).await.unwrap();
        end_marker(&bus);
        assert_eq!(rollovers(&mut sub).await.len(), 1);
    }

    #[tokio::test]
    async fn first_launch_publishes_nothing_and_records_today() {
        let bus = EventBus::default();
        let mut sub = bus.subscribe();
        let clock = FakeClock::at_local(Tz::UTC, d(2026, 10, 4), 9, 0, 0);
        let store = MemoryStore::default();
        roll_over(&clock, &store, &bus).await.unwrap();
        end_marker(&bus);
        assert!(rollovers(&mut sub).await.is_empty());
        assert_eq!(store.value(), Some(d(2026, 10, 4)));
    }

    #[tokio::test]
    async fn a_timezone_change_to_the_east_rolls_the_day_over_at_once() {
        // 06:00 UTC on Oct 4 is 23:00 on Oct 3 in Los Angeles, but 15:00 on Oct 4 in Tokyo.
        let bus = EventBus::default();
        let mut sub = bus.subscribe();
        let clock = FakeClock::new(
            chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2026, 10, 4, 6, 0, 0)
                .unwrap()
                .timestamp_millis(),
            America::Los_Angeles,
        );
        let store = MemoryStore::with(d(2026, 10, 3));
        roll_over(&clock, &store, &bus).await.unwrap(); // still the 3rd: nothing owed
        clock.set_tz(Asia::Tokyo);
        roll_over(&clock, &store, &bus).await.unwrap();
        end_marker(&bus);
        assert_eq!(
            rollovers(&mut sub).await,
            vec![("2026-10-03".to_owned(), "2026-10-04".to_owned())]
        );
    }

    #[tokio::test]
    async fn a_timezone_change_to_the_west_never_rolls_a_day_twice() {
        let bus = EventBus::default();
        let mut sub = bus.subscribe();
        let clock = FakeClock::new(
            chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2026, 10, 4, 6, 0, 0)
                .unwrap()
                .timestamp_millis(),
            Asia::Tokyo,
        );
        let store = MemoryStore::with(d(2026, 10, 4));
        clock.set_tz(America::Los_Angeles); // "today" is now the 3rd, earlier than the last day seen
        roll_over(&clock, &store, &bus).await.unwrap();
        end_marker(&bus);
        assert!(rollovers(&mut sub).await.is_empty());
        assert_eq!(
            store.value(),
            Some(d(2026, 10, 4)),
            "last_seen must never move backwards"
        );
    }

    // ---- the scheduler task (virtual time: nothing here sleeps for real) -------------------

    #[tokio::test(start_paused = true)]
    async fn the_scheduler_rolls_over_when_midnight_arrives() {
        let bus = EventBus::default();
        let mut sub = bus.subscribe();
        let clock = FakeClock::at_local(Tz::UTC, d(2026, 10, 3), 23, 59, 0);
        let store = MemoryStore::with(d(2026, 10, 3));
        let _scheduler = Scheduler::spawn(Arc::new(clock.clone()), store.clone(), bus.clone());

        tokio::task::yield_now().await; // first check runs: still the 3rd, nothing owed
        clock.advance_ms(61_000); // wall clock passes midnight
        tokio::time::advance(Duration::from_secs(61)).await; // and so does the timer
        tokio::task::yield_now().await;

        end_marker(&bus);
        assert_eq!(
            rollovers(&mut sub).await,
            vec![("2026-10-03".to_owned(), "2026-10-04".to_owned())]
        );
        assert_eq!(store.value(), Some(d(2026, 10, 4)));
    }

    #[tokio::test(start_paused = true)]
    async fn sleeping_through_midnight_is_caught_by_the_resume_signal() {
        // While the laptop sleeps the timer does not advance, but the wall clock does.
        let bus = EventBus::default();
        let mut sub = bus.subscribe();
        let clock = FakeClock::at_local(Tz::UTC, d(2026, 10, 3), 22, 0, 0);
        let store = MemoryStore::with(d(2026, 10, 3));
        let scheduler = Scheduler::spawn(Arc::new(clock.clone()), store.clone(), bus.clone());
        tokio::task::yield_now().await;

        clock.advance_ms(9 * 3_600_000); // wakes at 07:00 on the 4th; the timer saw no time pass
        scheduler.rearm();
        tokio::task::yield_now().await;

        end_marker(&bus);
        assert_eq!(
            rollovers(&mut sub).await,
            vec![("2026-10-03".to_owned(), "2026-10-04".to_owned())],
            "exactly once"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn rearming_with_nothing_owed_does_nothing() {
        let bus = EventBus::default();
        let mut sub = bus.subscribe();
        let clock = FakeClock::at_local(Tz::UTC, d(2026, 10, 3), 12, 0, 0);
        let store = MemoryStore::with(d(2026, 10, 3));
        let scheduler = Scheduler::spawn(Arc::new(clock), store, bus.clone());
        tokio::task::yield_now().await;
        for _ in 0..5 {
            scheduler.rearm();
            tokio::task::yield_now().await;
        }
        end_marker(&bus);
        assert!(rollovers(&mut sub).await.is_empty());
    }

    // ---- the real store ----------------------------------------------------------------------

    #[tokio::test]
    async fn the_database_store_round_trips_and_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let db =
            Arc::new(Database::open(&dir.path().join("pluto.db"), EventBus::default()).unwrap());
        let store = DbLastSeen(db);
        assert_eq!(store.get().await.unwrap(), None);
        store.set(d(2026, 10, 3), 1).await.unwrap();
        assert_eq!(store.get().await.unwrap(), Some(d(2026, 10, 3)));
        store.set(d(2026, 10, 4), 2).await.unwrap();
        assert_eq!(store.get().await.unwrap(), Some(d(2026, 10, 4)));
    }

    #[tokio::test]
    async fn the_last_seen_day_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pluto.db");
        {
            let db = Arc::new(Database::open(&path, EventBus::default()).unwrap());
            DbLastSeen(Arc::clone(&db))
                .set(d(2026, 10, 3), 1)
                .await
                .unwrap();
            db.shutdown().await;
        }
        let db = Arc::new(Database::open(&path, EventBus::default()).unwrap());
        assert_eq!(DbLastSeen(db).get().await.unwrap(), Some(d(2026, 10, 3)));
    }

    #[test]
    fn the_scheduler_module_has_no_polling_loop() {
        let src = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/scheduler.rs"),
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
