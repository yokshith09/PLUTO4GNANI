//! What a task can carry beyond its fields (owner, 2026-10-23): links shown next to it (FR-106),
//! a star (FR-107), several chosen days (FR-108) and a repeat rule ticked day by day (FR-109).
//!
//! The repeat arithmetic ([`Repeat::occurs_on`], [`repeat_days`]) is pure and tested here; the
//! storage functions write the tables from migration 004 and publish `TaskUpdated` like any edit.
//! Days that were not ticked are only ever *reported* ("not ticked"), never punished (FR-92).

use std::collections::BTreeSet;

use chrono::{Datelike, Days, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::clock::Clock;
use crate::db::Database;
use crate::error::{AppError, Result};
use crate::events::Event;
use crate::ids::new_id;
use crate::links::validate_external_url;
use crate::tasks::{self, Task, TaskEvent};

pub const LINKS_MAX: usize = 20;
pub const LINK_TITLE_MAX: usize = 200;
pub const DAYS_MAX: usize = 60;
pub const EVERY_MAX: u16 = 365;
/// How far back the repeat history looks.
pub const HISTORY_DAYS: u64 = 60;

/// How a task repeats. Stored as `daily`, `weekly:1,3,5` (ISO weekdays, Monday = 1) or `every:N`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Repeat {
    Daily,
    /// On these weekdays, Monday = 1 … Sunday = 7.
    Weekly {
        days: Vec<u8>,
    },
    /// Every `n` days from the start day.
    Every {
        n: u16,
    },
}

fn bad_rule() -> AppError {
    AppError::internal("A task has a repeat rule Pluto can't read.")
}

impl Repeat {
    pub fn to_db(&self) -> String {
        match self {
            Self::Daily => "daily".into(),
            Self::Weekly { days } => format!(
                "weekly:{}",
                days.iter().map(u8::to_string).collect::<Vec<_>>().join(",")
            ),
            Self::Every { n } => format!("every:{n}"),
        }
    }

    pub fn from_db(s: &str) -> Result<Self> {
        if s == "daily" {
            return Ok(Self::Daily);
        }
        if let Some(list) = s.strip_prefix("weekly:") {
            let days = list
                .split(',')
                .map(|d| d.parse::<u8>().map_err(|_| bad_rule()))
                .collect::<Result<Vec<_>>>()?;
            return Self::Weekly { days }.checked().map_err(|_| bad_rule());
        }
        if let Some(n) = s.strip_prefix("every:") {
            let n = n.parse::<u16>().map_err(|_| bad_rule())?;
            return Self::Every { n }.checked().map_err(|_| bad_rule());
        }
        Err(bad_rule())
    }

    /// The rule as Pluto keeps it: weekdays sorted and unique, within their ranges.
    pub fn checked(self) -> Result<Self> {
        match self {
            Self::Daily => Ok(Self::Daily),
            Self::Weekly { days } => {
                let set: BTreeSet<u8> = days.into_iter().collect();
                if set.is_empty() {
                    return Err(AppError::validation(
                        "repeat",
                        "Pick at least one day of the week.",
                    ));
                }
                if set.iter().any(|d| !(1..=7).contains(d)) {
                    return Err(AppError::validation(
                        "repeat",
                        "That isn't a day of the week.",
                    ));
                }
                Ok(Self::Weekly {
                    days: set.into_iter().collect(),
                })
            }
            Self::Every { n } if (1..=EVERY_MAX).contains(&n) => Ok(Self::Every { n }),
            Self::Every { .. } => Err(AppError::validation(
                "repeat",
                format!("Repeat every 1 to {EVERY_MAX} days."),
            )),
        }
    }

    /// Does a task starting on `start` (and ending on `until`, if set) repeat on `day`?
    pub fn occurs_on(&self, start: NaiveDate, until: Option<NaiveDate>, day: NaiveDate) -> bool {
        if day < start || until.is_some_and(|u| day > u) {
            return false;
        }
        match self {
            Self::Daily => true,
            Self::Weekly { days } => days.contains(&(day.weekday().number_from_monday() as u8)),
            Self::Every { n } => (day - start).num_days() % i64::from(*n) == 0,
        }
    }
}

/// Every day from `from` to `to` (inclusive) on which the rule repeats, oldest first.
pub fn repeat_days(
    rule: &Repeat,
    start: NaiveDate,
    until: Option<NaiveDate>,
    from: NaiveDate,
    to: NaiveDate,
) -> Vec<NaiveDate> {
    from.iter_days()
        .take_while(|d| *d <= to)
        .filter(|d| rule.occurs_on(start, until, *d))
        .collect()
}

/// A repeating task's start day: its planned day. Tasks always get one when a rule is set.
fn start_of(task: &Task) -> Option<NaiveDate> {
    task.planned_date
}

/// Does this repeating task repeat on `day`? False for tasks that don't repeat.
pub fn occurs(task: &Task, day: NaiveDate) -> bool {
    match (&task.repeat, start_of(task)) {
        (Some(rule), Some(start)) => rule.occurs_on(start, task.repeat_until, day),
        _ => false,
    }
}

/// A link on a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct TaskLink {
    pub id: String,
    pub url: String,
    /// What to show; empty means "show the address".
    pub title: String,
}

/// A link as the editor sends it. A bare `example.com/page` gets `https://` in front.
#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(default)]
pub struct LinkInput {
    pub url: String,
    pub title: String,
}

/// The days and repeat rule of a task, as the editor sends them. Chosen days and a repeat rule
/// can't both be set.
#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, optional_fields = nullable))]
#[serde(default)]
pub struct TaskPlan {
    pub days: Vec<NaiveDate>,
    pub repeat: Option<Repeat>,
    pub repeat_until: Option<NaiveDate>,
}

/// One repeat day in the history: was it ticked?
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RepeatDay {
    pub day: NaiveDate,
    pub ticked: bool,
}

fn clean_url(url: &str) -> Result<String> {
    let url = url.trim();
    let has_scheme = url.split_once(':').is_some_and(|(s, _)| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
            && !s.contains('.')
    });
    let full = if has_scheme || url.is_empty() {
        url.to_owned()
    } else {
        format!("https://{url}")
    };
    validate_external_url(&full).map_err(|e| e.with_field("links"))
}

/// Check and tidy links: addresses made openable, names trimmed, duplicates dropped.
pub fn clean_links(links: Vec<LinkInput>) -> Result<Vec<LinkInput>> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for l in links {
        let url = clean_url(&l.url)?;
        let title = l.title.trim().to_owned();
        if title.chars().count() > LINK_TITLE_MAX {
            return Err(AppError::validation(
                "links",
                format!("Link names can be up to {LINK_TITLE_MAX} characters."),
            ));
        }
        if seen.insert(url.clone()) {
            out.push(LinkInput { url, title });
        }
    }
    if out.len() > LINKS_MAX {
        return Err(AppError::validation(
            "links",
            format!("A task can have up to {LINKS_MAX} links."),
        ));
    }
    Ok(out)
}

/// Check a plan: days sorted and unique, a rule within range, an end day not before the start.
pub fn clean_plan(plan: TaskPlan, start: NaiveDate) -> Result<TaskPlan> {
    let days: Vec<NaiveDate> = plan
        .days
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if days.len() > DAYS_MAX {
        return Err(AppError::validation(
            "days",
            format!("Pick up to {DAYS_MAX} days."),
        ));
    }
    let repeat = plan.repeat.map(Repeat::checked).transpose()?;
    if repeat.is_some() && !days.is_empty() {
        return Err(AppError::validation(
            "days",
            "A repeating task can't also have chosen days. Pick one or the other.",
        ));
    }
    if repeat.is_none() && plan.repeat_until.is_some() {
        return Err(AppError::validation(
            "repeat_until",
            "Only a repeating task has an end day.",
        ));
    }
    if plan.repeat_until.is_some_and(|u| u < start) {
        return Err(AppError::validation(
            "repeat_until",
            "The last day can't be before the first.",
        ));
    }
    Ok(TaskPlan {
        days,
        repeat,
        repeat_until: plan.repeat_until,
    })
}

// ---- storage -------------------------------------------------------------------------------

/// Load a task's days and links into it.
pub(crate) fn fill(conn: &Connection, task: &mut Task) -> Result<()> {
    let mut stmt =
        conn.prepare_cached("SELECT day FROM task_days WHERE task_id = ?1 ORDER BY day")?;
    task.days = stmt
        .query_map([&task.id], |r| r.get::<_, String>(0))?
        .map(|d| {
            d.map_err(AppError::from).and_then(|d| {
                d.parse()
                    .map_err(|_| AppError::internal("A task has a day Pluto can't read."))
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut stmt = conn.prepare_cached(
        "SELECT id, url, title FROM task_links WHERE task_id = ?1 ORDER BY position",
    )?;
    task.links = stmt
        .query_map([&task.id], |r| {
            Ok(TaskLink {
                id: r.get(0)?,
                url: r.get(1)?,
                title: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(())
}

pub(crate) fn ticked(conn: &Connection, task_id: &str, day: NaiveDate) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM task_ticks WHERE task_id = ?1 AND day = ?2",
            params![task_id, day.to_string()],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn updated(conn: &Connection, id: &str, now: i64) -> Result<(Task, Vec<Event>)> {
    conn.execute(
        "UPDATE tasks SET updated_at = ?1 WHERE id = ?2",
        params![now, id],
    )?;
    let task = tasks::load(conn, id)?;
    Ok((task.clone(), vec![Event::TaskUpdated { at: now, task }]))
}

/// Replace a task's links (FR-106). Order is kept as given.
pub async fn set_links(
    db: &Database,
    clock: &dyn Clock,
    id: &str,
    links: Vec<LinkInput>,
) -> Result<Task> {
    let links = clean_links(links)?;
    let (id, now) = (id.to_owned(), clock.now_ms());
    db.write(move |tx| {
        tasks::load(tx, &id)?;
        tx.execute("DELETE FROM task_links WHERE task_id = ?1", [&id])?;
        for (i, l) in links.iter().enumerate() {
            tx.execute(
                "INSERT INTO task_links (id, task_id, url, title, position, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![new_id(), id, l.url, l.title, i as i64, now],
            )?;
        }
        updated(tx, &id, now)
    })
    .await
}

/// Star or unstar a task (FR-107).
pub async fn star(db: &Database, clock: &dyn Clock, id: &str, starred: bool) -> Result<Task> {
    let (id, now) = (id.to_owned(), clock.now_ms());
    db.write(move |tx| {
        let task = tasks::load(tx, &id)?;
        if task.starred == starred {
            return Ok((task, vec![]));
        }
        tx.execute(
            "UPDATE tasks SET starred = ?1 WHERE id = ?2",
            params![starred, id],
        )?;
        updated(tx, &id, now)
    })
    .await
}

/// Set the chosen days (FR-108) or the repeat rule (FR-109). With days, the planned day becomes
/// the first of them; a repeating task without a planned day starts today.
pub async fn set_plan(db: &Database, clock: &dyn Clock, id: &str, plan: TaskPlan) -> Result<Task> {
    let (id, now, today) = (id.to_owned(), clock.now_ms(), clock.today());
    let local = clock.local_date(now).to_string();
    db.write(move |tx| {
        let task = tasks::load(tx, &id)?;
        let start = task.planned_date.unwrap_or(today);
        let plan = clean_plan(plan, start)?;
        let planned = match (plan.days.first(), &plan.repeat) {
            (Some(first), _) => Some(*first),
            (None, Some(_)) => Some(start),
            (None, None) => task.planned_date,
        };
        tx.execute("DELETE FROM task_days WHERE task_id = ?1", [&id])?;
        for d in &plan.days {
            tx.execute(
                "INSERT INTO task_days (task_id, day) VALUES (?1, ?2)",
                params![id, d.to_string()],
            )?;
        }
        tx.execute(
            "UPDATE tasks SET planned_date = ?1, repeat_rule = ?2, repeat_until = ?3 WHERE id = ?4",
            params![
                tasks::date_text(planned),
                plan.repeat.as_ref().map(Repeat::to_db),
                tasks::date_text(plan.repeat_until),
                id
            ],
        )?;
        let moved = planned != task.planned_date;
        tasks::record(
            tx,
            &id,
            &TaskEvent {
                from_date: moved.then(|| tasks::date_text(task.planned_date)).flatten(),
                to_date: moved.then(|| tasks::date_text(planned)).flatten(),
                ..tasks::history_row("EDITED", now, &local)
            },
        )?;
        updated(tx, &id, now)
    })
    .await
}

/// Tick (or untick) one day of a repeating task (FR-109). Only days it repeats on, and not in
/// the future. Ticking one day never touches another.
pub async fn tick(
    db: &Database,
    clock: &dyn Clock,
    id: &str,
    day: NaiveDate,
    done: bool,
) -> Result<Task> {
    let (id, now, today) = (id.to_owned(), clock.now_ms(), clock.today());
    db.write(move |tx| {
        let task = tasks::load(tx, &id)?;
        if task.repeat.is_none() {
            return Err(AppError::validation(
                "day",
                "Only a repeating task is ticked day by day.",
            ));
        }
        if day > today {
            return Err(AppError::validation(
                "day",
                "A day that hasn't come yet can't be ticked.",
            ));
        }
        if !occurs(&task, day) {
            return Err(AppError::validation(
                "day",
                "This task doesn't repeat on that day.",
            ));
        }
        let changed = if done {
            tx.execute(
                "INSERT OR IGNORE INTO task_ticks (task_id, day, done_at) VALUES (?1, ?2, ?3)",
                params![id, day.to_string(), now],
            )?
        } else {
            tx.execute(
                "DELETE FROM task_ticks WHERE task_id = ?1 AND day = ?2",
                params![id, day.to_string()],
            )?
        };
        if changed == 0 {
            return Ok((task, vec![]));
        }
        let (task, mut events) = updated(tx, &id, now)?;
        events.insert(
            0,
            Event::TaskTicked {
                at: now,
                id: id.clone(),
                day: day.to_string(),
                done,
            },
        );
        Ok((task, events))
    })
    .await
}

/// The repeat days up to today (newest first, at most `limit`, looking back [`HISTORY_DAYS`]),
/// each with whether it was ticked.
pub async fn history(
    db: &Database,
    clock: &dyn Clock,
    id: &str,
    limit: usize,
) -> Result<Vec<RepeatDay>> {
    let (id, today) = (id.to_owned(), clock.today());
    db.read(move |conn| {
        let task = tasks::load(conn, &id)?;
        let (Some(rule), Some(start)) = (&task.repeat, start_of(&task)) else {
            return Ok(Vec::new());
        };
        let from = today
            .checked_sub_days(Days::new(HISTORY_DAYS))
            .unwrap_or(start)
            .max(start);
        let mut out = Vec::new();
        for day in repeat_days(rule, start, task.repeat_until, from, today)
            .into_iter()
            .rev()
            .take(limit)
        {
            out.push(RepeatDay {
                day,
                ticked: ticked(conn, &id, day)?,
            });
        }
        Ok(out)
    })
    .await
}

#[cfg(test)]
mod tests {
    use chrono_tz::Tz;

    use super::*;
    use crate::bus::EventBus;
    use crate::clock::FakeClock;
    use crate::error::ErrorCode;
    use crate::tasks::{TaskFilters, TaskInput, TaskStatus, TaskView};

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    // ---- the repeat rule (pure) ----------------------------------------------------------------

    #[test]
    fn rules_round_trip_through_the_database_text() {
        for r in [
            Repeat::Daily,
            Repeat::Weekly {
                days: vec![1, 3, 5],
            },
            Repeat::Every { n: 3 },
        ] {
            assert_eq!(Repeat::from_db(&r.to_db()).unwrap(), r);
        }
        for bad in [
            "weekly:", "weekly:0", "weekly:8", "every:0", "every:x", "monthly",
        ] {
            assert!(Repeat::from_db(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn weekdays_are_sorted_and_unique_and_checked() {
        assert_eq!(
            Repeat::Weekly {
                days: vec![5, 1, 5, 3]
            }
            .checked()
            .unwrap(),
            Repeat::Weekly {
                days: vec![1, 3, 5]
            }
        );
        assert!(Repeat::Weekly { days: vec![] }.checked().is_err());
        assert!(Repeat::Weekly { days: vec![0] }.checked().is_err());
        assert!(Repeat::Every { n: 0 }.checked().is_err());
        assert!(Repeat::Every { n: EVERY_MAX + 1 }.checked().is_err());
    }

    #[test]
    fn a_rule_repeats_only_between_its_start_and_end() {
        // Mon 19 Oct 2026 … Sun 1 Nov 2026.
        let start = d(2026, 10, 19);
        let until = Some(d(2026, 10, 30));
        let mwf = Repeat::Weekly {
            days: vec![1, 3, 5],
        };
        assert_eq!(
            repeat_days(&mwf, start, until, d(2026, 10, 1), d(2026, 11, 30)),
            [
                d(2026, 10, 19),
                d(2026, 10, 21),
                d(2026, 10, 23),
                d(2026, 10, 26),
                d(2026, 10, 28),
                d(2026, 10, 30)
            ]
        );
        assert_eq!(
            repeat_days(&Repeat::Every { n: 3 }, start, None, start, d(2026, 10, 28)),
            [
                d(2026, 10, 19),
                d(2026, 10, 22),
                d(2026, 10, 25),
                d(2026, 10, 28)
            ]
        );
        assert!(Repeat::Daily.occurs_on(start, None, start));
        assert!(
            !Repeat::Daily.occurs_on(start, None, d(2026, 10, 18)),
            "not before the start"
        );
        assert!(
            !Repeat::Daily.occurs_on(start, until, d(2026, 10, 31)),
            "not after the end"
        );
    }

    #[test]
    fn links_get_https_lose_duplicates_and_refuse_unsafe_addresses() {
        let l = |u: &str| LinkInput {
            url: u.into(),
            title: String::new(),
        };
        let out = clean_links(vec![
            l("example.com/a"),
            l("https://example.com/a"),
            l("mailto:me@x.org"),
            l("http://b.org"),
        ])
        .unwrap();
        let urls: Vec<_> = out.iter().map(|l| l.url.as_str()).collect();
        assert_eq!(
            urls,
            ["https://example.com/a", "mailto:me@x.org", "http://b.org"]
        );
        for bad in ["javascript:alert(1)", "file:///c:/x", "", "not a link"] {
            assert_eq!(
                clean_links(vec![l(bad)]).unwrap_err().code,
                ErrorCode::Validation,
                "{bad}"
            );
        }
        let many = (0..=LINKS_MAX).map(|i| l(&format!("a{i}.com"))).collect();
        assert!(clean_links(many).is_err());
    }

    #[test]
    fn a_plan_is_days_or_a_rule_never_both() {
        let start = d(2026, 10, 23);
        let p = clean_plan(
            TaskPlan {
                days: vec![d(2026, 10, 29), d(2026, 10, 27), d(2026, 10, 29)],
                ..Default::default()
            },
            start,
        )
        .unwrap();
        assert_eq!(p.days, [d(2026, 10, 27), d(2026, 10, 29)]);
        assert!(clean_plan(
            TaskPlan {
                days: vec![start],
                repeat: Some(Repeat::Daily),
                repeat_until: None
            },
            start
        )
        .is_err());
        assert!(clean_plan(
            TaskPlan {
                repeat: Some(Repeat::Daily),
                repeat_until: Some(d(2026, 10, 22)),
                ..Default::default()
            },
            start
        )
        .is_err());
        assert!(clean_plan(
            TaskPlan {
                repeat_until: Some(start),
                ..Default::default()
            },
            start
        )
        .is_err());
    }

    // ---- against a real database ---------------------------------------------------------------

    struct Fixture {
        db: Database,
        clock: FakeClock,
        _dir: tempfile::TempDir,
    }

    async fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("pluto.db"), EventBus::default()).unwrap();
        // Friday 23 Oct 2026, 10:00 in Kolkata.
        let clock = FakeClock::at_local(Tz::Asia__Kolkata, d(2026, 10, 23), 10, 0, 0);
        Fixture {
            db,
            clock,
            _dir: dir,
        }
    }

    async fn new_task(f: &Fixture, title: &str, planned: Option<NaiveDate>) -> Task {
        tasks::create(
            &f.db,
            &f.clock,
            TaskInput {
                title: title.into(),
                planned_date: planned,
                ..Default::default()
            },
        )
        .await
        .unwrap()
    }

    async fn today_titles(f: &Fixture) -> Vec<String> {
        tasks::query(&f.db, &f.clock, TaskView::Today, TaskFilters::default())
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.task.title)
            .collect()
    }

    #[tokio::test]
    async fn links_are_saved_in_order_shown_with_the_task_and_replaced_as_a_whole() {
        let f = fixture().await;
        let t = new_task(&f, "Read the paper", Some(f.clock.today())).await;
        let links = vec![
            LinkInput {
                url: "arxiv.org/abs/1".into(),
                title: "Paper".into(),
            },
            LinkInput {
                url: "https://github.com/x/y".into(),
                title: String::new(),
            },
        ];
        let got = set_links(&f.db, &f.clock, &t.id, links).await.unwrap();
        assert_eq!(
            got.links
                .iter()
                .map(|l| (l.url.as_str(), l.title.as_str()))
                .collect::<Vec<_>>(),
            [
                ("https://arxiv.org/abs/1", "Paper"),
                ("https://github.com/x/y", "")
            ]
        );
        // The Today list carries them too.
        let row = tasks::query(&f.db, &f.clock, TaskView::Today, TaskFilters::default())
            .await
            .unwrap()
            .remove(0);
        assert_eq!(row.task.links.len(), 2);
        // Replacing keeps only what was sent.
        let got = set_links(
            &f.db,
            &f.clock,
            &t.id,
            vec![LinkInput {
                url: "b.org".into(),
                title: String::new(),
            }],
        )
        .await
        .unwrap();
        assert_eq!(got.links.len(), 1);
        assert_eq!(
            set_links(&f.db, &f.clock, "nope", vec![])
                .await
                .unwrap_err()
                .code,
            ErrorCode::NotFound
        );
    }

    #[tokio::test]
    async fn starred_tasks_come_first_on_today_and_in_all() {
        let f = fixture().await;
        let today = Some(f.clock.today());
        new_task(&f, "Plain", today).await;
        f.clock.advance_ms(10);
        let s = new_task(&f, "Important", today).await;
        assert_eq!(today_titles(&f).await, ["Plain", "Important"]);
        assert!(star(&f.db, &f.clock, &s.id, true).await.unwrap().starred);
        assert_eq!(today_titles(&f).await, ["Important", "Plain"]);
        let all = tasks::query(&f.db, &f.clock, TaskView::All, TaskFilters::default())
            .await
            .unwrap();
        assert_eq!(all[0].task.title, "Important");
        assert!(!star(&f.db, &f.clock, &s.id, false).await.unwrap().starred);
    }

    #[tokio::test]
    async fn a_task_on_chosen_days_shows_on_today_on_each_of_them() {
        let f = fixture().await;
        let t = new_task(&f, "Lab report", None).await;
        let (fri, mon, wed) = (d(2026, 10, 23), d(2026, 10, 26), d(2026, 10, 28));
        let got = set_plan(
            &f.db,
            &f.clock,
            &t.id,
            TaskPlan {
                days: vec![wed, fri, mon],
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(got.days, [fri, mon, wed]);
        assert_eq!(
            got.planned_date,
            Some(fri),
            "planned on the first chosen day"
        );
        assert_eq!(today_titles(&f).await, ["Lab report"]);
        // Tuesday: not one of its days. Monday is in Upcoming from Friday.
        let up = tasks::query(&f.db, &f.clock, TaskView::Upcoming, TaskFilters::default())
            .await
            .unwrap();
        assert_eq!(up.len(), 1);
        f.clock
            .set_now_ms(FakeClock::at_local(Tz::Asia__Kolkata, d(2026, 10, 27), 10, 0, 0).now_ms());
        assert!(today_titles(&f).await.is_empty());
        f.clock
            .set_now_ms(FakeClock::at_local(Tz::Asia__Kolkata, wed, 10, 0, 0).now_ms());
        assert_eq!(today_titles(&f).await, ["Lab report"]);
        // Completing it once completes it.
        tasks::transition(&f.db, &f.clock, &t.id, TaskStatus::Completed)
            .await
            .unwrap();
        assert!(today_titles(&f).await.is_empty());
    }

    #[tokio::test]
    async fn a_repeating_task_is_ticked_day_by_day_and_its_unticked_days_are_kept() {
        let f = fixture().await;
        let t = new_task(&f, "Revise 30 min", None).await;
        let rule = Repeat::Weekly {
            days: vec![1, 3, 5],
        }; // Mon, Wed, Fri
        let got = set_plan(
            &f.db,
            &f.clock,
            &t.id,
            TaskPlan {
                repeat: Some(rule),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(got.planned_date, Some(d(2026, 10, 23)), "starts today");
        assert_eq!(
            today_titles(&f).await,
            ["Revise 30 min"],
            "Friday is a repeat day"
        );

        let ticked = tick(&f.db, &f.clock, &t.id, f.clock.today(), true)
            .await
            .unwrap();
        assert_eq!(
            ticked.status,
            TaskStatus::Planned,
            "ticking a day never finishes the task"
        );
        assert!(today_titles(&f).await.is_empty(), "ticked for today");
        let all = tasks::query(&f.db, &f.clock, TaskView::All, TaskFilters::default())
            .await
            .unwrap();
        assert!(all[0].done_today);

        // Saturday: not a repeat day. Monday: back, and Friday stays ticked.
        f.clock
            .set_now_ms(FakeClock::at_local(Tz::Asia__Kolkata, d(2026, 10, 24), 9, 0, 0).now_ms());
        assert!(today_titles(&f).await.is_empty());
        f.clock
            .set_now_ms(FakeClock::at_local(Tz::Asia__Kolkata, d(2026, 10, 26), 9, 0, 0).now_ms());
        assert_eq!(today_titles(&f).await, ["Revise 30 min"]);

        // Wednesday, Monday not ticked: the history says so, newest first.
        f.clock
            .set_now_ms(FakeClock::at_local(Tz::Asia__Kolkata, d(2026, 10, 28), 9, 0, 0).now_ms());
        let h = history(&f.db, &f.clock, &t.id, 14).await.unwrap();
        assert_eq!(
            h,
            [
                RepeatDay {
                    day: d(2026, 10, 28),
                    ticked: false
                },
                RepeatDay {
                    day: d(2026, 10, 26),
                    ticked: false
                },
                RepeatDay {
                    day: d(2026, 10, 23),
                    ticked: true
                },
            ]
        );
        // A late tick for Monday is fine; untick works; ticking twice changes nothing.
        tick(&f.db, &f.clock, &t.id, d(2026, 10, 26), true)
            .await
            .unwrap();
        tick(&f.db, &f.clock, &t.id, d(2026, 10, 26), true)
            .await
            .unwrap();
        assert!(history(&f.db, &f.clock, &t.id, 14).await.unwrap()[1].ticked);
        tick(&f.db, &f.clock, &t.id, d(2026, 10, 26), false)
            .await
            .unwrap();
        assert!(!history(&f.db, &f.clock, &t.id, 14).await.unwrap()[1].ticked);
    }

    #[tokio::test]
    async fn ticks_are_refused_off_the_rule_in_the_future_and_on_ordinary_tasks() {
        let f = fixture().await;
        let plain = new_task(&f, "Plain", None).await;
        assert_eq!(
            tick(&f.db, &f.clock, &plain.id, f.clock.today(), true)
                .await
                .unwrap_err()
                .code,
            ErrorCode::Validation
        );
        let t = new_task(&f, "Walk", None).await;
        set_plan(
            &f.db,
            &f.clock,
            &t.id,
            TaskPlan {
                repeat: Some(Repeat::Every { n: 2 }),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(
            tick(&f.db, &f.clock, &t.id, d(2026, 10, 24), true)
                .await
                .is_err(),
            "tomorrow"
        );
        f.clock
            .set_now_ms(FakeClock::at_local(Tz::Asia__Kolkata, d(2026, 10, 24), 9, 0, 0).now_ms());
        assert!(
            tick(&f.db, &f.clock, &t.id, d(2026, 10, 24), true)
                .await
                .is_err(),
            "not a repeat day"
        );
        assert!(
            tick(&f.db, &f.clock, &t.id, d(2026, 10, 22), true)
                .await
                .is_err(),
            "before it started"
        );
    }

    #[tokio::test]
    async fn clearing_the_plan_keeps_the_planned_day_and_drops_days_and_rule() {
        let f = fixture().await;
        let t = new_task(&f, "Essay", None).await;
        set_plan(
            &f.db,
            &f.clock,
            &t.id,
            TaskPlan {
                days: vec![d(2026, 10, 27), d(2026, 10, 29)],
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let got = set_plan(&f.db, &f.clock, &t.id, TaskPlan::default())
            .await
            .unwrap();
        assert!(got.days.is_empty() && got.repeat.is_none());
        assert_eq!(got.planned_date, Some(d(2026, 10, 27)));
        // And the history records the planned-day move.
        let h = tasks::history(&f.db, &t.id).await.unwrap();
        assert!(h
            .iter()
            .any(|e| e.kind == "EDITED" && e.to_date.as_deref() == Some("2026-10-27")));
    }
}
