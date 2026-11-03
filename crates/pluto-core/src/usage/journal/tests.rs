use chrono_tz::Tz;

use super::*;
use crate::bus::EventBus;
use crate::clock::FakeClock;
use crate::db::Database;
use crate::error::AppError;
use crate::usage::{TimeRange, MAX_SESSION_MS};
use std::sync::Arc;

const MIN: i64 = 60_000;
const HOUR: i64 = 60 * MIN;
/// 2026-10-03 00:00:00 UTC.
const T0: i64 = 1_790_985_600_000;

struct Fixture {
    _dir: tempfile::TempDir,
    clock: FakeClock,
    db: Arc<Database>,
    usage: UsageService,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let bus = EventBus::default();
    let db = Arc::new(Database::open(&dir.path().join("t.db"), bus).unwrap());
    let clock = FakeClock::new(T0 + 24 * HOUR, Tz::UTC);
    let usage = UsageService::new(db.clone(), Arc::new(clock.clone()));
    Fixture {
        _dir: dir,
        clock,
        db,
        usage,
    }
}

fn code(from: i64, to: i64) -> AppSession {
    AppSession {
        app: "VS Code".into(),
        category: "Coding".into(),
        is_browser: false,
        started_at: from,
        ended_at: to,
    }
}

fn site(domain: &str, from: i64, to: i64) -> DomainSession {
    DomainSession {
        browser: "Google Chrome".into(),
        domain: domain.into(),
        started_at: from,
        ended_at: to,
    }
}

impl Fixture {
    async fn rows(&self) -> i64 {
        self.db
            .read(|c| Ok(c.query_row("SELECT COUNT(*) FROM usage_open", [], |r| r.get(0))?))
            .await
            .unwrap()
    }
    async fn apps(&self) -> Vec<TimeRange> {
        self.usage
            .app_sessions("VS Code", 0, T0 + 100 * HOUR)
            .await
            .unwrap()
    }
    async fn sites(&self, domain: &str) -> Vec<TimeRange> {
        self.usage
            .domain_sessions(domain, 0, T0 + 100 * HOUR)
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn a_stretch_left_by_a_crash_comes_back_ending_when_it_was_last_seen() {
    let f = fixture();
    // The app was in front from T0 and last seen 40 minutes later; then the power went.
    f.usage
        .checkpoint_app(Some(&code(T0, T0 + 40 * MIN)))
        .await
        .unwrap();
    assert_eq!(f.rows().await, 1);
    // Pluto starts again the next morning.
    f.clock.set_now_ms(T0 + 30 * HOUR);
    assert_eq!(f.usage.recover_open().await.unwrap(), 1);
    assert_eq!(
        f.apps().await,
        vec![TimeRange {
            started_at: T0,
            ended_at: T0 + 40 * MIN
        }],
        "ends when last seen, not when Pluto came back"
    );
    assert_eq!(f.rows().await, 0);
}

#[tokio::test]
async fn each_checkpoint_replaces_the_last_one_so_there_is_never_more_than_one_row_per_kind() {
    let f = fixture();
    for minute in 1..=5 {
        f.usage
            .checkpoint_app(Some(&code(T0, T0 + minute * MIN)))
            .await
            .unwrap();
    }
    f.usage
        .checkpoint_site(Some(&site("github.com", T0, T0 + 3 * MIN)))
        .await
        .unwrap();
    assert_eq!(f.rows().await, 2);
    f.usage.recover_open().await.unwrap();
    assert_eq!(f.apps().await[0].ended_at, T0 + 5 * MIN, "the newest wins");
    assert_eq!(
        f.sites("github.com").await,
        vec![TimeRange {
            started_at: T0,
            ended_at: T0 + 3 * MIN
        }]
    );
}

#[tokio::test]
async fn recovering_twice_does_nothing_the_second_time() {
    let f = fixture();
    f.usage
        .checkpoint_app(Some(&code(T0, T0 + 10 * MIN)))
        .await
        .unwrap();
    assert_eq!(f.usage.recover_open().await.unwrap(), 1);
    assert_eq!(f.usage.recover_open().await.unwrap(), 0);
    assert_eq!(f.apps().await.len(), 1);
}

#[tokio::test]
async fn a_stretch_that_was_also_saved_normally_is_not_counted_twice() {
    let f = fixture();
    // The collector checkpointed, then the stretch ended and was written, then the crash came
    // before the journal was cleared.
    f.usage
        .checkpoint_app(Some(&code(T0, T0 + 10 * MIN)))
        .await
        .unwrap();
    f.usage
        .record_app_session(code(T0, T0 + 12 * MIN))
        .await
        .unwrap();
    assert_eq!(f.usage.recover_open().await.unwrap(), 0);
    assert_eq!(
        f.apps().await,
        vec![TimeRange {
            started_at: T0,
            ended_at: T0 + 12 * MIN
        }],
        "the normal, longer record stands alone"
    );
}

#[tokio::test]
async fn a_clean_end_clears_the_journal() {
    let f = fixture();
    f.usage
        .checkpoint_app(Some(&code(T0, T0 + MIN)))
        .await
        .unwrap();
    f.usage.checkpoint_app(None).await.unwrap();
    assert_eq!(f.rows().await, 0);
    // Clearing nothing is fine too.
    f.usage.checkpoint_app(None).await.unwrap();
    f.usage.checkpoint_site(None).await.unwrap();
    assert_eq!(f.usage.recover_open().await.unwrap(), 0);
    assert!(f.apps().await.is_empty());
}

#[tokio::test]
async fn recovery_obeys_the_same_rules_as_any_session() {
    let f = fixture();
    // Under two seconds is noise.
    f.usage
        .checkpoint_app(Some(&code(T0, T0 + 1_000)))
        .await
        .unwrap();
    assert_eq!(f.usage.recover_open().await.unwrap(), 0);
    assert!(f.apps().await.is_empty());

    // A stretch can't be longer than the 12 h cap.
    let long = code(T0, T0 + 20 * HOUR);
    f.usage.checkpoint_app(Some(&long)).await.unwrap();
    f.clock.set_now_ms(T0 + 40 * HOUR);
    assert_eq!(f.usage.recover_open().await.unwrap(), 1);
    let got = f.apps().await;
    assert_eq!(got[0].ended_at - got[0].started_at, MAX_SESSION_MS);
}

#[tokio::test]
async fn a_clock_set_back_cannot_make_a_session_end_in_the_future() {
    let f = fixture();
    f.usage
        .checkpoint_app(Some(&code(T0, T0 + 30 * MIN)))
        .await
        .unwrap();
    // Now is earlier than the last sighting (the clock was set back, or a bad value was stored).
    f.clock.set_now_ms(T0 + 20 * MIN);
    assert_eq!(f.usage.recover_open().await.unwrap(), 1);
    assert_eq!(
        f.apps().await,
        vec![TimeRange {
            started_at: T0,
            ended_at: T0 + 20 * MIN
        }]
    );
}

#[tokio::test]
async fn a_site_marked_do_not_track_is_not_brought_back() {
    let f = fixture();
    f.usage
        .set_domain_tracked("bank.example", false)
        .await
        .unwrap();
    f.usage
        .checkpoint_site(Some(&site("bank.example", T0, T0 + 5 * MIN)))
        .await
        .unwrap();
    assert_eq!(f.usage.recover_open().await.unwrap(), 0);
    assert!(f.sites("bank.example").await.is_empty());
    assert_eq!(f.rows().await, 0);
}

#[tokio::test]
async fn forgetting_a_site_or_deleting_history_clears_what_was_waiting() {
    let f = fixture();
    f.usage
        .checkpoint_site(Some(&site("news.example", T0, T0 + 5 * MIN)))
        .await
        .unwrap();
    f.usage
        .set_domain_tracked("news.example", false)
        .await
        .unwrap();
    assert_eq!(f.rows().await, 0, "site marked do not track");

    f.usage
        .checkpoint_app(Some(&code(T0, T0 + 5 * MIN)))
        .await
        .unwrap();
    f.usage
        .delete_range(T0 + 10 * MIN, T0 + 20 * MIN)
        .await
        .unwrap();
    assert_eq!(f.rows().await, 1, "a range that doesn't overlap leaves it");
    f.usage.delete_range(T0, T0 + 3 * MIN).await.unwrap();
    assert_eq!(f.rows().await, 0, "an overlapping range removes it");

    f.usage
        .checkpoint_app(Some(&code(T0, T0 + 5 * MIN)))
        .await
        .unwrap();
    f.usage
        .checkpoint_site(Some(&site("a.example", T0, T0 + 5 * MIN)))
        .await
        .unwrap();
    f.usage.delete_all().await.unwrap();
    assert_eq!(f.rows().await, 0, "delete all");
    assert_eq!(f.usage.recover_open().await.unwrap(), 0);
}

#[tokio::test]
async fn an_invalid_name_is_refused_at_checkpoint_not_stored() {
    let f = fixture();
    let mut bad = code(T0, T0 + MIN);
    bad.app = "   ".into();
    let err: AppError = f.usage.checkpoint_app(Some(&bad)).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::Validation);
    let err = f
        .usage
        .checkpoint_site(Some(&site("https://x.example/a?b=1", T0, T0 + MIN)))
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::Validation, "a link is not a host");
    assert_eq!(f.rows().await, 0);
}

#[tokio::test]
async fn a_damaged_row_is_dropped_without_losing_the_good_one() {
    let f = fixture();
    f.usage
        .checkpoint_site(Some(&site("good.example", T0, T0 + 4 * MIN)))
        .await
        .unwrap();
    // An app row whose category isn't one Pluto knows (a hand-edited or future database).
    f.db.write(|tx| {
        tx.execute(
            "INSERT INTO usage_open (kind, name, category, is_browser, browser, started_at, last_seen)
             VALUES ('app', 'Weird', 'NotACategory', 0, '', 1790985600000, 1790985900000)",
            [],
        )?;
        Ok(((), vec![]))
    })
    .await
    .unwrap();
    assert_eq!(f.usage.recover_open().await.unwrap(), 1);
    assert_eq!(f.sites("good.example").await.len(), 1);
    assert_eq!(f.rows().await, 0);
}
