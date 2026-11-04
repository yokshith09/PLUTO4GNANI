//! Natural-language dates, resolved here and nowhere else (API_CONTRACTS §0). The LLM returns the
//! phrase ("Friday tak", "naalaiku"); this turns it into a calendar date against a reference day.
//!
//! Rules: a bare weekday is its next occurrence after today (never today); "next <weekday>" is that
//! weekday in the week after the current Monday–Sunday week; day + month names ("13 November") use
//! the reference year, or the next year if that day has passed. Anything unclear is `None`, so the
//! UI asks the user instead of guessing.

use chrono::{Datelike, Duration, NaiveDate, Weekday};

const WEEKDAYS: [(&str, Weekday); 7] = [
    ("monday", Weekday::Mon),
    ("tuesday", Weekday::Tue),
    ("wednesday", Weekday::Wed),
    ("thursday", Weekday::Thu),
    ("friday", Weekday::Fri),
    ("saturday", Weekday::Sat),
    ("sunday", Weekday::Sun),
];

const MONTHS: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

const TODAY: [&str; 4] = ["today", "aaj", "inniku", "indu"];
const TOMORROW: [&str; 7] = [
    "tomorrow", "kal", "naalaiku", "naalai", "naale", "nale", "naalekke",
];
const DAY_AFTER: [&str; 3] = ["parso", "parson", "nethu"];
const NEXT: [&str; 4] = ["next", "agle", "adutha", "mundina"];

fn tokens(phrase: &str) -> Vec<String> {
    phrase
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        // "friday-kulla" (Tamil) and "friday-ku" keep the day name.
        .flat_map(|t| t.split('-').map(str::to_owned).collect::<Vec<_>>())
        .filter(|t| !t.is_empty())
        .collect()
}

fn has(toks: &[String], words: &[&str]) -> bool {
    toks.iter().any(|t| words.contains(&t.as_str()))
}

fn weekday_of(tok: &str) -> Option<Weekday> {
    WEEKDAYS
        .iter()
        .find(|(name, _)| tok == *name || (tok.len() == 3 && name.starts_with(tok)))
        .map(|(_, d)| *d)
}

fn month_of(tok: &str) -> Option<u32> {
    MONTHS
        .iter()
        .position(|m| tok == *m || (tok.len() == 3 && m.starts_with(tok)))
        .map(|i| i as u32 + 1)
}

/// Days from `today` to the next `target` weekday, 1..=7 (never today).
fn days_until(today: NaiveDate, target: Weekday) -> i64 {
    let diff = (i64::from(target.num_days_from_monday())
        - i64::from(today.weekday().num_days_from_monday()))
    .rem_euclid(7);
    if diff == 0 {
        7
    } else {
        diff
    }
}

pub fn resolve_date(phrase: &str, today: NaiveDate) -> Option<NaiveDate> {
    let toks = tokens(phrase);
    if toks.is_empty() {
        return None;
    }

    // "13 November" / "November 13" / "13th nov"
    for (i, t) in toks.iter().enumerate() {
        let Some(month) = month_of(t) else { continue };
        let num = |s: &str| {
            s.trim_end_matches(|c: char| c.is_alphabetic())
                .parse::<u32>()
                .ok()
        };
        let day = i
            .checked_sub(1)
            .and_then(|j| num(&toks[j]))
            .or_else(|| toks.get(i + 1).and_then(|s| num(s)));
        if let Some(day) = day {
            let on = |year| NaiveDate::from_ymd_opt(year, month, day);
            return on(today.year())
                .filter(|d| *d >= today)
                .or_else(|| on(today.year() + 1));
        }
    }

    // "in 3 days" / "3 din mein" / "3 naal"
    for (i, t) in toks.iter().enumerate() {
        if let (Ok(n), Some(unit)) = (t.parse::<i64>(), toks.get(i + 1)) {
            if matches!(
                unit.as_str(),
                "day" | "days" | "din" | "naal" | "naalil" | "dina"
            ) && (1..=365).contains(&n)
            {
                return Some(today + Duration::days(n));
            }
        }
    }

    if has(&toks, &DAY_AFTER)
        || (has(&toks, &["day"]) && has(&toks, &["after"]) && has(&toks, &["tomorrow"]))
    {
        return Some(today + Duration::days(2));
    }
    if has(&toks, &TOMORROW) {
        return Some(today + Duration::days(1));
    }
    if has(&toks, &TODAY) {
        return Some(today);
    }

    if let Some(target) = toks.iter().find_map(|t| weekday_of(t)) {
        if has(&toks, &NEXT) {
            // The weekday in the week after the current Monday–Sunday week.
            let to_next_monday = 7 - i64::from(today.weekday().num_days_from_monday());
            return Some(
                today + Duration::days(to_next_monday + i64::from(target.num_days_from_monday())),
            );
        }
        return Some(today + Duration::days(days_until(today, target)));
    }

    if has(&toks, &NEXT) && has(&toks, &["week", "hafte", "vaaram", "vara"]) {
        let to_next_monday = 7 - i64::from(today.weekday().num_days_from_monday());
        return Some(today + Duration::days(to_next_monday));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }
    const MON: fn() -> NaiveDate = || d(2026, 11, 2);
    const WED: fn() -> NaiveDate = || d(2026, 11, 4);

    #[test]
    fn weekdays_resolve_to_the_next_occurrence() {
        assert_eq!(resolve_date("Thursday", MON()), Some(d(2026, 11, 5)));
        assert_eq!(resolve_date("Friday tak", MON()), Some(d(2026, 11, 6)));
        assert_eq!(resolve_date("Friday-kulla", MON()), Some(d(2026, 11, 6)));
        assert_eq!(
            resolve_date("Monday", MON()),
            Some(d(2026, 11, 9)),
            "never today"
        );
        assert_eq!(resolve_date("wed", MON()), Some(d(2026, 11, 4)));
    }

    #[test]
    fn next_weekday_means_the_week_after() {
        assert_eq!(resolve_date("next Monday", MON()), Some(d(2026, 11, 9)));
        assert_eq!(resolve_date("next Monday", WED()), Some(d(2026, 11, 9)));
        assert_eq!(resolve_date("next Thursday", MON()), Some(d(2026, 11, 12)));
        assert_eq!(resolve_date("next week", WED()), Some(d(2026, 11, 9)));
    }

    #[test]
    fn relative_days_in_four_languages() {
        for p in ["tomorrow", "kal tak", "naalaiku", "naale"] {
            assert_eq!(resolve_date(p, MON()), Some(d(2026, 11, 3)), "{p}");
        }
        for p in ["today", "aaj", "inniku"] {
            assert_eq!(resolve_date(p, MON()), Some(d(2026, 11, 2)), "{p}");
        }
        assert_eq!(
            resolve_date("day after tomorrow", MON()),
            Some(d(2026, 11, 4))
        );
        assert_eq!(resolve_date("parso", MON()), Some(d(2026, 11, 4)));
        assert_eq!(resolve_date("in 3 days", MON()), Some(d(2026, 11, 5)));
        assert_eq!(resolve_date("5 din mein", MON()), Some(d(2026, 11, 7)));
    }

    #[test]
    fn day_and_month_names() {
        assert_eq!(resolve_date("13 November", MON()), Some(d(2026, 11, 13)));
        assert_eq!(resolve_date("November 13", MON()), Some(d(2026, 11, 13)));
        assert_eq!(resolve_date("20th Nov", MON()), Some(d(2026, 11, 20)));
        assert_eq!(
            resolve_date("1 January", MON()),
            Some(d(2027, 1, 1)),
            "rolls to next year once passed"
        );
        assert_eq!(resolve_date("31 February", MON()), None, "not a date");
    }

    #[test]
    fn unclear_phrases_are_none() {
        for p in ["", "soon", "ASAP", "when ready", "later this month"] {
            assert_eq!(resolve_date(p, MON()), None, "{p}");
        }
    }
}
