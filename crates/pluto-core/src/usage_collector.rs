//! The OS-independent half of app tracking: the loop that owns the [`Tracker`], decides when to
//! look at the idle time, batches finished sessions into the database and starts or stops the
//! OS source live as the settings change.
//!
//! The only OS-specific parts are behind [`Platform`] (a foreground-window source and an idle
//! clock), implemented by the shell. Everything here is tested with a fake one.
//!
//! Timers (ADR-019): there are exactly two, and each is armed only when it has work.
//! * **Look**, every [`POLL_INTERVAL_MS`], while tracking is on. It reads the idle time (also while
//!   idle, to see the person come back), whether a film or call is holding the screen on, and
//!   which window is really in front, so a focus event the OS never delivered (after a lock, a
//!   sleep, a full-screen switch) puts the tracker right within a minute instead of never.
//! * **Flush**, one [`FLUSH_INTERVAL`] after the first unsaved session, only while one is waiting.
//!
//! **Crash safety.** Every idle look also *checkpoints* the stretch in progress into the usage
//! journal (see `usage::journal`) after writing any waiting sessions, and the journal is
//! recovered when the collector starts. A kill, crash or power cut therefore loses at most the
//! time since the last look, never a whole stretch. No extra timer is needed: the look already
//! exists exactly when there is a stretch to remember.
//!
//! With tracking off, or nothing in front, the loop parks on the bus and the OS source.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio::sync::mpsc::{self, UnboundedSender};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::bus::{EventBus, Recv, Subscriber};
use crate::clock::Clock;
use crate::error::Result;
use crate::events::Event;
use crate::settings::SettingsKey;
use crate::settings_service::SettingsService;
use crate::usage::{
    effective_idle_ms, AppSession, TrackEvent, Tracker, UsageRecord, UsageService, POLL_INTERVAL_MS,
};

/// Finished sessions are written in one batch this long after the first one is waiting.
pub const FLUSH_INTERVAL: Duration = Duration::from_secs(60);
/// If the database keeps failing, stop remembering sessions beyond this many.
const MAX_PENDING: usize = 1000;

/// What the operating system provides.
pub trait Platform: Send + Sync + 'static {
    /// Start reporting which app is in front: the executable name, or `None` when nothing
    /// trackable is. The current foreground app must be reported right away, then every change.
    /// Must not block for long and must not poll.
    fn start(&self, sink: mpsc::UnboundedSender<Option<String>>) -> Result<()>;
    /// Stop reporting. Safe to call when not started.
    fn stop(&self);
    /// Milliseconds since the last keyboard or mouse input, if the OS can say.
    fn idle_ms(&self) -> Option<u64>;
    /// Whether something is holding the screen on right now (a film, a video call, a
    /// presentation). Such time is use even though nobody touches the keyboard or the mouse.
    fn screen_held(&self) -> bool {
        false
    }
    /// Which app is in front right now, asked directly (`Some(None)` means nothing trackable),
    /// or `None` if this platform cannot be asked. Used to correct a missed focus event.
    fn current_app(&self) -> Option<Option<String>> {
        None
    }
}

#[derive(Clone, Copy)]
pub struct Timing {
    pub poll: Duration,
    pub flush: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            poll: Duration::from_millis(POLL_INTERVAL_MS),
            flush: FLUSH_INTERVAL,
        }
    }
}

pub struct Collector {
    running: Arc<AtomicBool>,
    task: Mutex<Option<JoinHandle<()>>>,
}

impl Collector {
    /// Start the collector task. It reads `tracking.apps` straight away and then follows
    /// `SettingChanged`. Subscribes to the bus before returning, so no change is missed.
    pub fn spawn<P: Platform>(
        platform: Arc<P>,
        usage: Arc<UsageService>,
        settings: Arc<SettingsService>,
        clock: Arc<dyn Clock>,
        bus: &EventBus,
    ) -> Self {
        Self::spawn_with(platform, usage, settings, clock, bus, Timing::default())
    }

    pub fn spawn_with<P: Platform>(
        platform: Arc<P>,
        usage: Arc<UsageService>,
        settings: Arc<SettingsService>,
        clock: Arc<dyn Clock>,
        bus: &EventBus,
        timing: Timing,
    ) -> Self {
        let running = Arc::new(AtomicBool::new(false));
        let worker = Worker {
            platform,
            usage,
            settings,
            clock,
            timing,
            running: running.clone(),
            events: bus.subscribe(),
            tracker: Tracker::default(),
            wanted: false,
            exclude: Vec::new(),
            pending: Vec::new(),
            flush_at: None,
            poll_at: None,
            journaled: false,
        };
        Self {
            running,
            task: Mutex::new(Some(tokio::spawn(worker.run()))),
        }
    }

    /// Whether the OS source is running right now.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Wait (briefly) for the task to finish after `AppShuttingDown`: it closes the open
    /// session and writes everything before it ends. Call before shutting the database down.
    pub async fn finish(&self) {
        let task = self
            .task
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(task) = task {
            let _ = tokio::time::timeout(Duration::from_secs(3), task).await;
        }
    }
}

struct Worker<P: Platform> {
    platform: Arc<P>,
    usage: Arc<UsageService>,
    settings: Arc<SettingsService>,
    clock: Arc<dyn Clock>,
    timing: Timing,
    running: Arc<AtomicBool>,
    events: Subscriber,
    tracker: Tracker,
    /// `tracking.apps`.
    wanted: bool,
    exclude: Vec<String>,
    pending: Vec<UsageRecord>,
    flush_at: Option<Instant>,
    poll_at: Option<Instant>,
    /// Whether the journal currently holds a row for the stretch in progress.
    journaled: bool,
}

async fn sleep_until_opt(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending::<()>().await,
    }
}

impl<P: Platform> Worker<P> {
    async fn run(mut self) {
        let (tx, mut rx) = mpsc::unbounded_channel::<Option<String>>();
        let platform = self.platform.clone();
        self.usage.set_idle_probe(Some(Arc::new(move || {
            let idle = platform.idle_ms()?;
            Some(effective_idle_ms(idle, platform.screen_held()))
        })));
        self.recover().await;
        self.reload_settings(&tx).await;
        loop {
            let (poll_at, flush_at) = (self.poll_at, self.flush_at);
            tokio::select! {
                event = self.events.recv() => match event {
                    Recv::Event(Event::AppShuttingDown { .. }) | Recv::Closed => {
                        self.stop().await;
                        break;
                    }
                    Recv::Event(Event::SettingChanged { key, value, .. }) => {
                        self.setting_changed(&key, &value, &tx).await;
                    }
                    Recv::Resync { .. } => self.reload_settings(&tx).await,
                    Recv::Event(_) => {}
                },
                Some(app) = rx.recv() => {
                    if self.running.load(Ordering::Relaxed) {
                        let at = self.clock.now_ms();
                        self.feed(TrackEvent::Focus { at, app });
                    }
                }
                () = sleep_until_opt(poll_at) => {
                    self.look();
                    self.checkpoint().await;
                }
                () = sleep_until_opt(flush_at) => self.flush().await,
            }
            self.tidy_journal().await;
            self.rearm();
            self.publish_live();
        }
        self.usage.set_idle_probe(None);
    }

    /// Show the stretch in progress in the totals (not stored until it ends).
    fn publish_live(&self) {
        let live = if self.running.load(Ordering::Relaxed) {
            self.tracker.open_session(self.clock.now_ms())
        } else {
            None
        };
        self.usage
            .set_unsaved(live, self.tracker.quiet_since(), self.waiting());
    }

    /// Finished sessions not yet in the database.
    fn waiting(&self) -> Vec<AppSession> {
        self.pending
            .iter()
            .filter_map(|record| match record {
                UsageRecord::App(session) => Some(session.clone()),
                UsageRecord::Domain(_) => None,
            })
            .collect()
    }

    /// Timers exist only while there is something for them to do.
    fn rearm(&mut self) {
        if !self.running.load(Ordering::Relaxed) {
            self.poll_at = None;
        } else if self.poll_at.is_none() {
            self.poll_at = Some(Instant::now() + self.timing.poll);
        }
        if self.pending.is_empty() {
            self.flush_at = None;
        } else if self.flush_at.is_none() {
            self.flush_at = Some(Instant::now() + self.timing.flush);
        }
    }

    fn feed(&mut self, event: TrackEvent) {
        let sessions = self.tracker.handle(event);
        self.keep(sessions);
    }

    fn keep(&mut self, sessions: Vec<crate::usage::AppSession>) {
        self.pending
            .extend(sessions.into_iter().map(UsageRecord::App));
        if self.pending.len() > MAX_PENDING {
            let excess = self.pending.len() - MAX_PENDING;
            self.pending.drain(..excess);
        }
    }

    /// The periodic look: correct the app in front if an event was missed, then read the idle
    /// time (a held screen counts as activity).
    fn look(&mut self) {
        self.poll_at = None; // `rearm` sets the next one
        if let Some(app) = self.platform.current_app() {
            let at = self.clock.now_ms();
            self.feed(TrackEvent::Focus { at, app });
        }
        if let Some(idle_ms) = self.platform.idle_ms() {
            let idle_ms = effective_idle_ms(idle_ms, self.platform.screen_held());
            let sessions = self.tracker.poll(self.clock.now_ms(), idle_ms);
            self.keep(sessions);
        }
    }

    async fn flush(&mut self) {
        self.flush_at = None;
        if self.pending.is_empty() {
            return;
        }
        let batch = std::mem::take(&mut self.pending);
        if let Err(error) = self.usage.save_waiting(batch.clone()).await {
            tracing::warn!(code = ?error.code, "couldn't save app time; it will be retried");
            self.pending = batch;
        }
    }

    /// Turn what a previous run left in the journal (a crash, a kill, a power cut) into sessions.
    async fn recover(&self) {
        match self.usage.recover_open().await {
            Ok(0) => {}
            Ok(n) => tracing::info!(sessions = n, "recovered time from before an unclean stop"),
            Err(error) => {
                tracing::warn!(code = ?error.code, "couldn't recover unsaved app time");
            }
        }
    }

    /// Write waiting sessions, then remember the stretch in progress. Sessions go first so the
    /// journal never describes something that is only in memory.
    async fn checkpoint(&mut self) {
        self.flush().await;
        if !self.pending.is_empty() {
            return; // the database is failing; try again at the next look
        }
        let open = if self.running.load(Ordering::Relaxed) {
            self.tracker.open_session(self.clock.now_ms())
        } else {
            None
        };
        if let Some(open) = open {
            match self.usage.checkpoint_app(Some(&open)).await {
                Ok(()) => self.journaled = true,
                Err(error) => {
                    tracing::warn!(code = ?error.code, "couldn't remember the open app stretch");
                }
            }
        }
    }

    /// Drop the journal row once nothing is in progress and everything finished is saved.
    async fn tidy_journal(&mut self) {
        if !self.journaled || !self.pending.is_empty() {
            return;
        }
        let in_progress = self.running.load(Ordering::Relaxed)
            && self.tracker.open_session(self.clock.now_ms()).is_some();
        if !in_progress {
            self.clear_journal().await;
        }
    }

    async fn clear_journal(&mut self) {
        match self.usage.checkpoint_app(None).await {
            Ok(()) => self.journaled = false,
            Err(error) => {
                tracing::warn!(code = ?error.code, "couldn't clear the open app stretch");
            }
        }
    }

    async fn reload_settings(&mut self, tx: &UnboundedSender<Option<String>>) {
        let Ok(all) = self.settings.get_all().await else {
            return;
        };
        if let Some(list) = all
            .get(SettingsKey::TrackingExcludeApps.as_str())
            .and_then(exclude_list)
        {
            self.set_exclude(list);
        }
        let on = all
            .get(SettingsKey::TrackingApps.as_str())
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        self.set_wanted(on, tx).await;
    }

    async fn setting_changed(
        &mut self,
        key: &str,
        value: &serde_json::Value,
        tx: &UnboundedSender<Option<String>>,
    ) {
        if key == SettingsKey::TrackingApps.as_str() {
            self.set_wanted(value.as_bool().unwrap_or(false), tx).await;
        } else if key == SettingsKey::TrackingExcludeApps.as_str() {
            if let Some(list) = exclude_list(value) {
                self.set_exclude(list);
            }
        }
    }

    fn set_exclude(&mut self, list: Vec<String>) {
        if list == self.exclude {
            return;
        }
        let sessions = self.tracker.set_excluded(self.clock.now_ms(), &list);
        self.exclude = list;
        self.keep(sessions);
    }

    async fn set_wanted(&mut self, on: bool, tx: &UnboundedSender<Option<String>>) {
        self.wanted = on;
        let running = self.running.load(Ordering::Relaxed);
        if on && !running {
            self.tracker = Tracker::new(&self.exclude);
            match self.platform.start(tx.clone()) {
                Ok(()) => self.running.store(true, Ordering::Relaxed),
                Err(error) => {
                    tracing::warn!(code = ?error.code, "couldn't start app tracking");
                }
            }
        } else if !on && running {
            self.stop().await;
        }
    }

    /// Close the open session, stop the OS source and write everything.
    async fn stop(&mut self) {
        if self.running.swap(false, Ordering::Relaxed) {
            self.platform.stop();
        }
        let at = self.clock.now_ms();
        self.feed(TrackEvent::Shutdown { at });
        self.usage.set_unsaved(None, None, self.waiting());
        self.flush().await;
        self.tidy_journal().await;
    }
}

fn exclude_list(value: &serde_json::Value) -> Option<Vec<String>> {
    value
        .as_array()?
        .iter()
        .map(|v| v.as_str().map(str::to_owned))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use chrono_tz::Tz;
    use serde_json::json;

    use super::*;
    use crate::clock::FakeClock;
    use crate::db::Database;
    use crate::error::AppError;
    use crate::usage::TimeRange;

    const T0: i64 = 1_790_985_600_000;
    const S: i64 = 1_000;
    const MIN: i64 = 60 * S;

    #[derive(Default)]
    struct FakePlatform {
        sink: Mutex<Option<UnboundedSender<Option<String>>>>,
        starts: AtomicUsize,
        stops: AtomicUsize,
        idle: Mutex<Option<u64>>,
        fail_start: AtomicBool,
        held: AtomicBool,
        screen: Mutex<Option<Option<String>>>,
        /// When set, the idle time is the fake clock minus this last-input time, so it moves in
        /// step with the clock instead of being set separately (two steps a look could fall between).
        last_input: Mutex<Option<(FakeClock, i64)>>,
    }

    impl FakePlatform {
        fn focus(&self, app: Option<&str>) {
            let sink = self.sink.lock().unwrap();
            sink.as_ref()
                .expect("started")
                .send(app.map(str::to_owned))
                .unwrap();
        }
        fn last_input_at(&self, clock: &FakeClock, at: i64) {
            *self.last_input.lock().unwrap() = Some((clock.clone(), at));
        }
        fn set_idle(&self, ms: u64) {
            *self.idle.lock().unwrap() = Some(ms);
        }
    }

    impl Platform for FakePlatform {
        fn start(&self, sink: UnboundedSender<Option<String>>) -> Result<()> {
            if self.fail_start.load(Ordering::Relaxed) {
                return Err(AppError::internal("no hook"));
            }
            self.starts.fetch_add(1, Ordering::SeqCst);
            *self.sink.lock().unwrap() = Some(sink);
            Ok(())
        }
        fn stop(&self) {
            self.stops.fetch_add(1, Ordering::SeqCst);
            *self.sink.lock().unwrap() = None;
        }
        fn idle_ms(&self) -> Option<u64> {
            if let Some((clock, at)) = &*self.last_input.lock().unwrap() {
                return Some(u64::try_from(clock.now_ms() - at).unwrap_or(0));
            }
            *self.idle.lock().unwrap()
        }
        fn screen_held(&self) -> bool {
            self.held.load(Ordering::Relaxed)
        }
        fn current_app(&self) -> Option<Option<String>> {
            self.screen.lock().unwrap().clone()
        }
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        bus: EventBus,
        clock: FakeClock,
        db: Arc<Database>,
        usage: Arc<UsageService>,
        settings: Arc<SettingsService>,
        platform: Arc<FakePlatform>,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let bus = EventBus::default();
        let db = Arc::new(Database::open(&dir.path().join("t.db"), bus.clone()).unwrap());
        let clock = FakeClock::new(T0, Tz::UTC);
        Fixture {
            usage: Arc::new(UsageService::new(db.clone(), Arc::new(clock.clone()))),
            settings: Arc::new(SettingsService::new(db.clone(), Arc::new(clock.clone()))),
            platform: Arc::new(FakePlatform::default()),
            _dir: dir,
            bus,
            clock,
            db,
        }
    }

    fn quick() -> Timing {
        Timing {
            poll: Duration::from_millis(15),
            flush: Duration::from_millis(30),
        }
    }

    impl Fixture {
        fn collector(&self, timing: Timing) -> Collector {
            Collector::spawn_with(
                self.platform.clone(),
                self.usage.clone(),
                self.settings.clone(),
                Arc::new(self.clock.clone()),
                &self.bus,
                timing,
            )
        }

        async fn spans(&self, app: &str) -> Vec<TimeRange> {
            self.usage
                .app_sessions(app, 0, T0 + 100 * 3_600_000)
                .await
                .unwrap()
        }
    }

    /// Give the collector task time to handle what was just sent (it stamps events on receipt).
    async fn settle() {
        tokio::time::sleep(Duration::from_millis(40)).await;
    }

    /// Waits up to 15 s (CI runners, especially under coverage instrumentation, can be far slower
    /// than a laptop) for `check` to hold.
    async fn eventually<F: std::future::Future<Output = bool>>(mut check: impl FnMut() -> F) {
        for _ in 0..1_500 {
            if check().await {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("condition not reached in time");
    }

    async fn running(c: &Collector, want: bool) {
        eventually(|| async { c.is_running() == want }).await;
    }

    #[tokio::test]
    async fn the_totals_include_the_app_in_front_before_it_is_saved() {
        let f = fixture();
        let c = f.collector(Timing::default());
        f.settings.set("tracking.apps", json!(true)).await.unwrap();
        running(&c, true).await;
        f.platform.focus(Some("Code.exe"));
        settle().await;
        f.clock.advance_ms(10 * MIN);
        let s = f.usage.summary(T0 - MIN, T0 + 60 * MIN).await.unwrap();
        assert_eq!(s.total_seconds, 600, "nothing was saved yet, still counted");
        assert!(f.spans("VS Code").await.is_empty());

        f.settings.set("tracking.apps", json!(false)).await.unwrap();
        running(&c, false).await;
        let s = f.usage.summary(T0 - MIN, T0 + 60 * MIN).await.unwrap();
        assert_eq!(s.total_seconds, 600, "saved now, counted once");
    }

    #[tokio::test]
    async fn tracking_is_off_by_default_and_nothing_starts() {
        let f = fixture();
        let c = f.collector(quick());
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert!(!c.is_running());
        assert_eq!(f.platform.starts.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn turning_tracking_on_starts_the_source_and_off_stops_it_and_saves_the_open_session() {
        let f = fixture();
        let c = f.collector(Timing::default());
        f.settings.set("tracking.apps", json!(true)).await.unwrap();
        running(&c, true).await;
        assert_eq!(f.platform.starts.load(Ordering::SeqCst), 1);

        f.platform.focus(Some("Code.exe"));
        settle().await;
        f.clock.advance_ms(10 * MIN);
        f.platform.focus(Some("chrome.exe"));
        settle().await;
        f.clock.advance_ms(5 * MIN);
        // Nothing is written until the batch flushes or tracking stops.
        f.settings.set("tracking.apps", json!(false)).await.unwrap();
        running(&c, false).await;
        eventually(|| async { f.spans("Google Chrome").await.len() == 1 }).await;

        assert_eq!(f.platform.stops.load(Ordering::SeqCst), 1);
        assert_eq!(
            f.spans("VS Code").await,
            vec![TimeRange {
                started_at: T0,
                ended_at: T0 + 10 * MIN
            }]
        );
        assert_eq!(
            f.spans("Google Chrome").await,
            vec![TimeRange {
                started_at: T0 + 10 * MIN,
                ended_at: T0 + 15 * MIN
            }]
        );
    }

    #[tokio::test]
    async fn focus_reports_after_tracking_is_off_record_nothing() {
        let f = fixture();
        let c = f.collector(quick());
        f.settings.set("tracking.apps", json!(true)).await.unwrap();
        running(&c, true).await;
        let sink = f.platform.sink.lock().unwrap().clone().unwrap();
        f.settings.set("tracking.apps", json!(false)).await.unwrap();
        running(&c, false).await;
        f.clock.advance_ms(MIN);
        let _ = sink.send(Some("Code.exe".into())); // a straggler from the OS thread
        f.clock.advance_ms(MIN);
        tokio::time::sleep(Duration::from_millis(80)).await;
        let n: i64 =
            f.db.read(|c| Ok(c.query_row("SELECT COUNT(*) FROM app_sessions", [], |r| r.get(0))?))
                .await
                .unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn the_setting_is_read_at_startup() {
        let f = fixture();
        f.settings.set("tracking.apps", json!(true)).await.unwrap();
        let c = f.collector(quick());
        running(&c, true).await;
    }

    #[tokio::test]
    async fn quitting_closes_the_open_session_and_writes_it_before_finish_returns() {
        let f = fixture();
        let c = f.collector(Timing::default());
        f.settings.set("tracking.apps", json!(true)).await.unwrap();
        running(&c, true).await;
        f.platform.focus(Some("Slack.exe"));
        settle().await;
        f.clock.advance_ms(3 * MIN);
        f.bus.publish(Event::AppShuttingDown {
            at: f.clock.now_ms(),
        });
        c.finish().await;
        assert!(!c.is_running());
        assert_eq!(
            f.spans("Slack").await,
            vec![TimeRange {
                started_at: T0,
                ended_at: T0 + 3 * MIN
            }]
        );
    }

    #[tokio::test]
    async fn the_idle_look_closes_the_session_at_the_last_input() {
        let f = fixture();
        let c = f.collector(quick());
        f.settings.set("tracking.apps", json!(true)).await.unwrap();
        running(&c, true).await;
        f.platform.focus(Some("Code.exe"));
        f.platform.set_idle(0);
        settle().await;
        // Input stops one minute in; the machine is looked at every minute from then on.
        for minute in 1..=12u64 {
            f.clock.advance_ms(MIN);
            f.platform.set_idle(minute.saturating_sub(1) * MIN as u64);
            settle().await;
        }
        eventually(|| async { f.spans("VS Code").await.len() == 1 }).await;
        assert_eq!(
            f.spans("VS Code").await,
            vec![TimeRange {
                started_at: T0,
                ended_at: T0 + MIN
            }]
        );
    }

    /// The summary total for the whole test day, in seconds.
    async fn total(f: &Fixture) -> i64 {
        f.usage
            .summary(T0 - MIN, T0 + 100 * 3_600_000)
            .await
            .unwrap()
            .total_seconds
    }

    #[tokio::test]
    async fn the_total_never_falls_back_after_the_person_walks_away() {
        let f = fixture();
        let c = f.collector(quick());
        f.settings.set("tracking.apps", json!(true)).await.unwrap();
        running(&c, true).await;
        f.platform.focus(Some("Code.exe"));
        // Input stops after one minute; the idle time follows the clock from then on.
        f.platform.last_input_at(&f.clock, T0 + MIN);
        settle().await;
        let mut seen = vec![total(&f).await];
        for _ in 1..=9 {
            f.clock.advance_ms(MIN);
            settle().await;
            seen.push(total(&f).await);
        }
        assert!(
            seen.windows(2).all(|w| w[0] <= w[1]),
            "the total went down: {seen:?}"
        );
        assert_eq!(*seen.last().unwrap(), 60, "one minute of real input");
    }

    #[tokio::test]
    async fn between_looks_the_total_follows_the_input_and_stops_when_it_stops() {
        let f = fixture();
        // Default timing: no look happens during the test, only the reading of the totals.
        let c = f.collector(Timing::default());
        f.settings.set("tracking.apps", json!(true)).await.unwrap();
        running(&c, true).await;
        f.platform.focus(Some("Code.exe"));
        f.platform.last_input_at(&f.clock, T0 + 60 * S);
        settle().await;
        f.clock.advance_ms(30 * S);
        assert_eq!(total(&f).await, 30, "typing: counted up to now");
        f.clock.advance_ms(30 * S);
        assert_eq!(total(&f).await, 60);
        f.clock.advance_ms(3 * MIN);
        assert_eq!(
            total(&f).await,
            60,
            "walked away: not counted past the last input"
        );
        // Back within the ten minutes: the stretch carries on and the gap counts.
        f.platform.last_input_at(&f.clock, T0 + 4 * MIN);
        assert_eq!(total(&f).await, 240);
    }

    #[tokio::test]
    async fn a_session_that_just_ended_is_still_in_the_totals_before_it_is_saved() {
        let f = fixture();
        let c = f.collector(Timing::default());
        f.settings.set("tracking.apps", json!(true)).await.unwrap();
        running(&c, true).await;
        f.platform.focus(Some("Code.exe"));
        settle().await;
        f.clock.advance_ms(10 * MIN);
        f.platform.focus(Some("chrome.exe"));
        settle().await;
        f.clock.advance_ms(5 * MIN);
        assert!(
            f.spans("VS Code").await.is_empty(),
            "still waiting to be saved"
        );
        assert_eq!(total(&f).await, 15 * 60, "the ten minutes must not vanish");
        let s = f.usage.summary(T0 - MIN, T0 + 60 * MIN).await.unwrap();
        assert_eq!(s.apps.len(), 2);
    }

    #[tokio::test]
    async fn saving_a_waiting_session_does_not_count_it_twice() {
        let f = fixture();
        let c = f.collector(quick());
        f.settings.set("tracking.apps", json!(true)).await.unwrap();
        running(&c, true).await;
        f.platform.focus(Some("Code.exe"));
        settle().await;
        f.clock.advance_ms(10 * MIN);
        f.platform.focus(Some("chrome.exe"));
        eventually(|| async { f.spans("VS Code").await.len() == 1 }).await;
        assert_eq!(total(&f).await, 10 * 60, "saved, and not also waiting");
    }

    #[tokio::test]
    async fn a_film_or_a_call_with_no_input_keeps_counting() {
        let f = fixture();
        let c = f.collector(quick());
        f.settings.set("tracking.apps", json!(true)).await.unwrap();
        running(&c, true).await;
        f.platform.focus(Some("Zoom.exe"));
        f.platform.set_idle(0);
        f.platform.held.store(true, Ordering::Relaxed);
        settle().await;
        // Forty minutes without touching the keyboard or the mouse, looked at every minute. Each
        // minute waits until the collector has counted it: a fixed sleep raced the 15 ms poll on a
        // slow CI runner and dropped minutes (2160 s instead of 2400 s, 4 Nov 2026).
        for minute in 1..=40u64 {
            f.clock.advance_ms(MIN);
            f.platform.set_idle(minute * MIN as u64);
            let want = i64::try_from(minute).unwrap() * 60;
            eventually(|| async { total(&f).await >= want }).await;
        }
        assert_eq!(total(&f).await, 40 * 60);
        f.settings.set("tracking.apps", json!(false)).await.unwrap();
        running(&c, false).await;
        assert_eq!(
            f.spans("Zoom").await,
            vec![TimeRange {
                started_at: T0,
                ended_at: T0 + 40 * MIN
            }]
        );
    }

    #[tokio::test]
    async fn without_a_held_screen_five_quiet_minutes_still_end_the_stretch() {
        let f = fixture();
        let c = f.collector(quick());
        f.settings.set("tracking.apps", json!(true)).await.unwrap();
        running(&c, true).await;
        f.platform.focus(Some("Zoom.exe"));
        f.platform.set_idle(0);
        settle().await;
        // The last input is one minute in.
        for minute in 1..=12u64 {
            f.clock.advance_ms(MIN);
            f.platform.set_idle(minute.saturating_sub(1) * MIN as u64);
            settle().await;
        }
        eventually(|| async { f.spans("Zoom").await.len() == 1 }).await;
        assert_eq!(total(&f).await, 60, "only the minute with input counts");
    }

    #[tokio::test]
    async fn a_missed_focus_event_is_put_right_at_the_next_look() {
        let f = fixture();
        let c = f.collector(quick());
        f.settings.set("tracking.apps", json!(true)).await.unwrap();
        running(&c, true).await;
        f.platform.set_idle(0);
        // The lock screen was in front when Pluto started looking; no event ever arrives for the
        // window that comes back after unlocking, but the look sees it.
        *f.platform.screen.lock().unwrap() = Some(None);
        settle().await;
        f.clock.advance_ms(MIN);
        settle().await;
        *f.platform.screen.lock().unwrap() = Some(Some("Code.exe".to_owned()));
        f.clock.advance_ms(MIN);
        settle().await;
        for _ in 0..10 {
            f.clock.advance_ms(MIN);
            settle().await;
        }
        assert!(total(&f).await >= 9 * 60, "got {}", total(&f).await);
        // And the other way: the app is closed without an event.
        *f.platform.screen.lock().unwrap() = Some(None);
        f.clock.advance_ms(MIN);
        settle().await;
        f.clock.advance_ms(MIN);
        settle().await;
        let before = total(&f).await;
        for _ in 0..5 {
            f.clock.advance_ms(MIN);
            settle().await;
        }
        assert_eq!(
            total(&f).await,
            before,
            "nothing is in front, nothing counts"
        );
    }

    #[tokio::test]
    async fn the_exclude_list_applies_live() {
        let f = fixture();
        let c = f.collector(quick());
        f.settings.set("tracking.apps", json!(true)).await.unwrap();
        running(&c, true).await;
        f.platform.focus(Some("KeePass.exe"));
        settle().await;
        f.clock.advance_ms(MIN);
        f.settings
            .set("tracking.exclude_apps", json!(["keepass"]))
            .await
            .unwrap();
        // The app in front is excluded from now on; what it did before is kept.
        eventually(|| async { f.spans("KeePass").await.len() == 1 }).await;
        f.clock.advance_ms(MIN);
        f.platform.focus(Some("keepass.exe"));
        f.platform.focus(Some("Code.exe"));
        settle().await;
        f.clock.advance_ms(MIN);
        f.platform.focus(None);
        eventually(|| async { f.spans("VS Code").await.len() == 1 }).await;
        assert_eq!(f.spans("KeePass").await.len(), 1);
    }

    #[tokio::test]
    async fn a_source_that_cannot_start_leaves_the_collector_not_running() {
        let f = fixture();
        f.platform.fail_start.store(true, Ordering::Relaxed);
        let c = f.collector(quick());
        f.settings.set("tracking.apps", json!(true)).await.unwrap();
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert!(!c.is_running());
        // Turning it off and on again retries.
        f.platform.fail_start.store(false, Ordering::Relaxed);
        f.settings.set("tracking.apps", json!(false)).await.unwrap();
        f.settings.set("tracking.apps", json!(true)).await.unwrap();
        running(&c, true).await;
    }

    async fn journal_rows(f: &Fixture) -> Vec<(String, i64, i64)> {
        f.db.read(|c| {
            let mut stmt =
                c.prepare("SELECT name, started_at, last_seen FROM usage_open ORDER BY kind")?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
        .unwrap()
    }

    /// Kill the collector the way a crash would: no shutdown event, no final write.
    fn crash(c: &Collector) {
        c.task.lock().unwrap().take().expect("running").abort();
    }

    #[tokio::test]
    async fn a_crash_loses_at_most_the_last_minute_not_the_whole_stretch() {
        let f = fixture();
        let c = f.collector(quick());
        f.settings.set("tracking.apps", json!(true)).await.unwrap();
        running(&c, true).await;
        f.platform.set_idle(0);
        f.platform.focus(Some("Code.exe"));
        settle().await;
        // Twenty minutes of work, looked at every minute (a longer gap would read as sleep).
        for _ in 0..20 {
            f.clock.advance_ms(MIN);
            settle().await;
        }
        eventually(|| async {
            journal_rows(&f).await == vec![("VS Code".to_owned(), T0, T0 + 20 * MIN)]
        })
        .await;
        assert!(f.spans("VS Code").await.is_empty(), "nothing is final yet");

        // The power goes. Pluto starts again later and recovers the twenty minutes.
        crash(&c);
        f.clock.advance_ms(2 * 3_600_000);
        let _again = f.collector(quick());
        eventually(|| async { f.spans("VS Code").await.len() == 1 }).await;
        assert_eq!(
            f.spans("VS Code").await,
            vec![TimeRange {
                started_at: T0,
                ended_at: T0 + 20 * MIN
            }],
            "ends when last seen, not when Pluto came back"
        );
        assert!(journal_rows(&f).await.is_empty());
    }

    #[tokio::test]
    async fn a_clean_quit_leaves_nothing_in_the_journal_and_a_restart_adds_nothing() {
        let f = fixture();
        let c = f.collector(quick());
        f.settings.set("tracking.apps", json!(true)).await.unwrap();
        running(&c, true).await;
        f.platform.set_idle(0);
        f.platform.focus(Some("Slack.exe"));
        settle().await;
        for _ in 0..5 {
            f.clock.advance_ms(MIN);
            settle().await;
        }
        f.bus.publish(Event::AppShuttingDown {
            at: f.clock.now_ms(),
        });
        c.finish().await;
        assert!(journal_rows(&f).await.is_empty());
        assert_eq!(f.spans("Slack").await.len(), 1);

        let _again = f.collector(quick());
        settle().await;
        assert_eq!(f.spans("Slack").await.len(), 1, "counted once");
    }

    #[tokio::test]
    async fn the_journal_follows_the_stretch_and_is_cleared_when_nothing_is_in_front() {
        let f = fixture();
        let c = f.collector(quick());
        f.settings.set("tracking.apps", json!(true)).await.unwrap();
        running(&c, true).await;
        f.platform.set_idle(0);
        f.platform.focus(Some("Code.exe"));
        settle().await;
        f.clock.advance_ms(2 * MIN);
        eventually(|| async { journal_rows(&f).await.len() == 1 }).await;

        // Another window comes to the front: the first stretch is saved, the new one journaled.
        f.platform.focus(Some("chrome.exe"));
        settle().await;
        f.clock.advance_ms(2 * MIN);
        eventually(|| async {
            journal_rows(&f).await == vec![("Google Chrome".to_owned(), T0 + 2 * MIN, T0 + 4 * MIN)]
        })
        .await;
        eventually(|| async { f.spans("VS Code").await.len() == 1 }).await;

        // Nothing trackable in front: nothing left waiting.
        f.platform.focus(None);
        eventually(|| async {
            journal_rows(&f).await.is_empty() && f.spans("Google Chrome").await.len() == 1
        })
        .await;
    }

    #[tokio::test]
    async fn a_session_waiting_to_be_saved_is_never_dropped_from_the_journal_first() {
        let f = fixture();
        // Slow flush so the closed session is still in memory when the journal is checked.
        let c = f.collector(Timing {
            poll: Duration::from_millis(15),
            flush: Duration::from_secs(3_600),
        });
        f.settings.set("tracking.apps", json!(true)).await.unwrap();
        running(&c, true).await;
        f.platform.set_idle(0);
        f.platform.focus(Some("Code.exe"));
        settle().await;
        for _ in 0..3 {
            f.clock.advance_ms(MIN);
            settle().await;
        }
        // Each look writes waiting sessions first, so a crash right now still has the stretch.
        crash(&c);
        let _again = f.collector(quick());
        eventually(|| async { f.spans("VS Code").await.len() == 1 }).await;
    }

    #[test]
    fn this_module_arms_timers_only_through_the_two_documented_deadlines() {
        let src = include_str!("usage_collector.rs");
        let production = src.split("#[cfg(test)]").next().unwrap();
        for banned in [
            "interval(",
            "thread::sleep",
            "loop {\n        tokio::time::sleep",
        ] {
            assert!(!production.contains(banned), "{banned}");
        }
    }
}
