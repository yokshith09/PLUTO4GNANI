//! Time, injectable (TRD §6.1). Every date-dependent function takes a [`Clock`], so tests never
//! sleep and never depend on the machine's timezone.
//!
//! Calendar days are *local* days (`YYYY-MM-DD`); instants are milliseconds since the Unix epoch.

use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Duration, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;

pub trait Clock: Send + Sync {
    /// Now, ms since the Unix epoch (UTC).
    fn now_ms(&self) -> i64;
    /// The timezone "local" means right now. It can change while the app runs.
    fn tz(&self) -> Tz;

    /// The local calendar day containing the instant `ms`.
    fn local_date(&self, ms: i64) -> NaiveDate {
        let utc = DateTime::<Utc>::from_timestamp_millis(ms).unwrap_or(DateTime::UNIX_EPOCH);
        utc.with_timezone(&self.tz()).date_naive()
    }

    fn today(&self) -> NaiveDate {
        self.local_date(self.now_ms())
    }
}

/// The real clock. The timezone is detected once and re-read only when told to
/// ([`SystemClock::refresh_tz`], called when the OS reports a timezone change), so ordinary
/// reads cost nothing.
pub struct SystemClock {
    tz: RwLock<Tz>,
}

impl SystemClock {
    pub fn new() -> Self {
        Self {
            tz: RwLock::new(detect_tz()),
        }
    }

    /// Re-detect the OS timezone. Returns the new one.
    pub fn refresh_tz(&self) -> Tz {
        let detected = detect_tz();
        *self.tz.write().unwrap_or_else(PoisonError::into_inner) = detected;
        detected
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }

    fn tz(&self) -> Tz {
        *self.tz.read().unwrap_or_else(PoisonError::into_inner)
    }
}

/// UTC if the OS timezone can't be read or isn't an IANA name we know: dates stay consistent
/// (just possibly off by the UTC offset) instead of the app failing to start.
fn detect_tz() -> Tz {
    iana_time_zone::get_timezone()
        .ok()
        .and_then(|name| name.parse::<Tz>().ok())
        .unwrap_or(Tz::UTC)
}

/// A clock the test controls. Cheap to clone; clones share the same time.
#[derive(Clone)]
pub struct FakeClock {
    state: Arc<Mutex<(i64, Tz)>>,
}

impl FakeClock {
    pub fn new(now_ms: i64, tz: Tz) -> Self {
        Self {
            state: Arc::new(Mutex::new((now_ms, tz))),
        }
    }

    /// At local `date` `hh:mm:ss` in `tz` (the first such instant if it occurs twice).
    pub fn at_local(tz: Tz, date: NaiveDate, hh: u32, mm: u32, ss: u32) -> Self {
        let local = date.and_time(NaiveTime::from_hms_opt(hh, mm, ss).expect("valid time"));
        let ms = tz
            .from_local_datetime(&local)
            .earliest()
            .expect("that local time exists in this zone")
            .timestamp_millis();
        Self::new(ms, tz)
    }

    pub fn set_now_ms(&self, ms: i64) {
        self.state.lock().unwrap_or_else(PoisonError::into_inner).0 = ms;
    }

    pub fn advance_ms(&self, by: i64) {
        self.state.lock().unwrap_or_else(PoisonError::into_inner).0 += by;
    }

    pub fn set_tz(&self, tz: Tz) {
        self.state.lock().unwrap_or_else(PoisonError::into_inner).1 = tz;
    }
}

impl Clock for FakeClock {
    fn now_ms(&self) -> i64 {
        self.state.lock().unwrap_or_else(PoisonError::into_inner).0
    }

    fn tz(&self) -> Tz {
        self.state.lock().unwrap_or_else(PoisonError::into_inner).1
    }
}

/// The instant the local day after `clock.today()` begins, in ms (UTC).
///
/// "Midnight" doesn't always exist: where DST starts at 00:00 the local day begins at 01:00, so
/// the first instant that actually exists on that date is used. Where DST ends at midnight and
/// 00:00 happens twice, the first occurrence starts the day.
pub fn next_local_midnight_ms(clock: &dyn Clock) -> i64 {
    let tz = clock.tz();
    let tomorrow = clock.today().succ_opt().unwrap_or(NaiveDate::MAX);
    first_instant_of(tz, tomorrow)
}

/// The instant the local day `date` begins, in ms (UTC). See [`next_local_midnight_ms`] for the
/// awkward cases (a skipped or repeated midnight).
pub fn start_of_day_ms(clock: &dyn Clock, date: NaiveDate) -> i64 {
    first_instant_of(clock.tz(), date)
}

fn first_instant_of(tz: Tz, date: NaiveDate) -> i64 {
    let midnight = date.and_time(NaiveTime::MIN);
    // Step forward in half-hour increments until a local time exists (a DST gap is at most a few hours).
    for step in 0..=8 {
        let candidate = midnight + Duration::minutes(30 * step);
        if let Some(instant) = tz.from_local_datetime(&candidate).earliest() {
            return instant.timestamp_millis();
        }
    }
    // Unreachable for real zones; fall back to UTC midnight rather than panic.
    Utc.from_utc_datetime(&midnight).timestamp_millis()
}

#[cfg(test)]
mod tests {
    use chrono_tz::{America, Asia, Europe};

    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn local_date_follows_the_clock_timezone_not_utc() {
        // 2026-10-04 02:00 UTC is still Oct 3 in Los Angeles and already Oct 4 in Tokyo.
        let ms = Utc
            .with_ymd_and_hms(2026, 10, 4, 2, 0, 0)
            .unwrap()
            .timestamp_millis();
        let clock = FakeClock::new(ms, America::Los_Angeles);
        assert_eq!(clock.today(), d(2026, 10, 3));
        clock.set_tz(Asia::Tokyo);
        assert_eq!(clock.today(), d(2026, 10, 4));
    }

    #[test]
    fn the_next_midnight_is_the_start_of_the_next_local_day() {
        let clock = FakeClock::at_local(Europe::London, d(2026, 6, 15), 23, 59, 30);
        let next = next_local_midnight_ms(&clock);
        assert_eq!(next - clock.now_ms(), 30_000, "30 seconds to go");
        assert_eq!(clock.local_date(next), d(2026, 6, 16));
        assert_eq!(clock.local_date(next - 1), d(2026, 6, 15));
    }

    #[test]
    fn a_spring_forward_day_is_23_hours_and_a_fall_back_day_is_25() {
        for (tz, date, hours) in [
            (Europe::London, d(2026, 3, 29), 23),
            (Europe::London, d(2026, 10, 25), 25),
            (America::New_York, d(2026, 3, 8), 23),
            (America::New_York, d(2026, 11, 1), 25),
        ] {
            let start_of_day = first_instant_of(tz, date);
            let clock = FakeClock::new(start_of_day, tz);
            assert_eq!(
                clock.today(),
                date,
                "{tz} {date}: the day must start on its own date"
            );
            let next = next_local_midnight_ms(&clock);
            assert_eq!((next - start_of_day) / 3_600_000, hours, "{tz} {date}");
            assert_eq!(clock.local_date(next), date.succ_opt().unwrap());
            assert_eq!(clock.local_date(next - 1), date);
        }
    }

    #[test]
    fn when_dst_skips_midnight_the_day_starts_at_the_first_instant_that_exists() {
        // Cuba springs forward at 00:00 -> 01:00, so 2026-03-08 00:00 does not exist.
        let clock = FakeClock::at_local(America::Havana, d(2026, 3, 7), 12, 0, 0);
        let next = next_local_midnight_ms(&clock);
        assert_eq!(clock.local_date(next), d(2026, 3, 8));
        assert_eq!(
            clock.local_date(next - 1),
            d(2026, 3, 7),
            "one millisecond earlier is still the 7th"
        );
        let local = Utc
            .timestamp_millis_opt(next)
            .unwrap()
            .with_timezone(&America::Havana);
        assert_eq!(local.format("%H:%M").to_string(), "01:00");
    }

    #[test]
    fn when_midnight_happens_twice_the_first_occurrence_starts_the_day() {
        // Cuba falls back at 01:00 -> 00:00 on 2026-11-01, so 00:00-00:59 occurs twice.
        let clock = FakeClock::at_local(America::Havana, d(2026, 10, 31), 12, 0, 0);
        let next = next_local_midnight_ms(&clock);
        let first = America::Havana
            .from_local_datetime(&d(2026, 11, 1).and_time(NaiveTime::MIN))
            .earliest()
            .unwrap();
        assert_eq!(next, first.timestamp_millis());
        assert_eq!(clock.local_date(next), d(2026, 11, 1));
    }

    #[test]
    fn the_fake_clock_is_shared_between_clones_and_advances_on_command() {
        let clock = FakeClock::new(1_000, Tz::UTC);
        let handle = clock.clone();
        handle.advance_ms(500);
        assert_eq!(clock.now_ms(), 1_500);
        clock.set_now_ms(9);
        assert_eq!(handle.now_ms(), 9);
    }

    #[test]
    fn the_system_clock_reports_a_sane_time_and_always_has_a_timezone() {
        let clock = SystemClock::new();
        assert!(clock.now_ms() > 1_700_000_000_000, "later than Nov 2023");
        let _ = clock.tz(); // never panics, falls back to UTC
        let after = clock.refresh_tz();
        assert_eq!(clock.tz(), after);
    }
}
