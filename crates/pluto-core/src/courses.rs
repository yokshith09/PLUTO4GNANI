//! The student pack (T16): courses, exams, which course a task or note belongs to, and the weekly
//! report. A course link lives in `task_course` / `note_course`, so deleting a course removes only
//! the links. Course names are unique ignoring case and Unicode form, like labels.

use chrono::{Days, NaiveDate, TimeZone};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::clock::Clock;
use crate::db::Database;
use crate::error::{AppError, Result};
use crate::ids::new_id;
use crate::labels::fold;

pub const NAME_MAX: usize = 60;
pub const EXAM_TITLE_MAX: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Course {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Exam {
    pub id: String,
    pub course_id: Option<String>,
    pub title: String,
    /// Local calendar day, `YYYY-MM-DD`.
    pub on_date: String,
}

/// Which course each task and each note belongs to (only items that have one).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CourseLinks {
    pub tasks: Vec<ItemCourse>,
    pub notes: Vec<ItemCourse>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ItemCourse {
    pub item_id: String,
    pub course_id: String,
}

/// One course's week: focus time on its tasks and how many of its tasks were completed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CourseWeek {
    pub course: Option<Course>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub focus_ms: i64,
    pub tasks_done: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct WeeklyReport {
    /// First day of the seven, `YYYY-MM-DD`.
    pub from: String,
    /// Last day of the seven (inclusive).
    pub to: String,
    pub courses: Vec<CourseWeek>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total_focus_ms: i64,
    pub total_tasks_done: u32,
}

fn clean_name(name: &str) -> Result<String> {
    let name = name.trim();
    match name.chars().count() {
        0 => Err(AppError::validation("name", "Give the course a name.")),
        n if n > NAME_MAX => Err(AppError::validation(
            "name",
            format!("Course names can be up to {NAME_MAX} characters."),
        )),
        _ => Ok(name.to_owned()),
    }
}

fn parse_day(field: &str, s: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| AppError::validation(field, "Pick a date."))
}

pub async fn list(db: &Database) -> Result<Vec<Course>> {
    db.read(|conn| {
        let mut stmt = conn.prepare("SELECT id, name FROM courses ORDER BY name_folded")?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Course {
                    id: r.get(0)?,
                    name: r.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    })
    .await
}

/// Create a course, or return the existing one if the name is taken in any casing.
pub async fn create(db: &Database, clock: &dyn Clock, name: &str) -> Result<Course> {
    let name = clean_name(name)?;
    let folded = fold(&name);
    let now = clock.now_ms();
    db.write(move |tx| {
        let existing = tx
            .query_row(
                "SELECT id, name FROM courses WHERE name_folded = ?1",
                [&folded],
                |r| {
                    Ok(Course {
                        id: r.get(0)?,
                        name: r.get(1)?,
                    })
                },
            )
            .optional()?;
        if let Some(c) = existing {
            return Ok((c, vec![]));
        }
        let c = Course { id: new_id(), name };
        tx.execute(
            "INSERT INTO courses (id, name, name_folded, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![c.id, c.name, folded, now],
        )?;
        Ok((c, vec![]))
    })
    .await
}

pub async fn rename(db: &Database, id: &str, name: &str) -> Result<Course> {
    let name = clean_name(name)?;
    let folded = fold(&name);
    let id = id.to_owned();
    db.write(move |tx| {
        let other: Option<String> = tx
            .query_row(
                "SELECT id FROM courses WHERE name_folded = ?1",
                [&folded],
                |r| r.get(0),
            )
            .optional()?;
        if other.is_some_and(|o| o != id) {
            return Err(
                AppError::conflict("A course with that name already exists.").with_field("name"),
            );
        }
        let changed = tx.execute(
            "UPDATE courses SET name = ?1, name_folded = ?2 WHERE id = ?3",
            params![name, folded, id],
        )?;
        if changed == 0 {
            return Err(AppError::not_found("That course no longer exists."));
        }
        Ok((Course { id, name }, vec![]))
    })
    .await
}

/// Delete a course. Its exams go with it; tasks and notes stay and just lose the label.
pub async fn delete(db: &Database, id: &str) -> Result<()> {
    let id = id.to_owned();
    db.write(move |tx| {
        if tx.execute("DELETE FROM courses WHERE id = ?1", [&id])? == 0 {
            return Err(AppError::not_found("That course no longer exists."));
        }
        Ok(((), vec![]))
    })
    .await
}

/// Exams from `from` (a day, `YYYY-MM-DD`) onward, soonest first.
pub async fn exams_from(db: &Database, from: &str) -> Result<Vec<Exam>> {
    parse_day("from", from)?;
    let from = from.to_owned();
    db.read(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, course_id, title, on_date FROM exams WHERE on_date >= ?1 ORDER BY on_date, title",
        )?;
        let rows = stmt
            .query_map([&from], |r| {
                Ok(Exam {
                    id: r.get(0)?,
                    course_id: r.get(1)?,
                    title: r.get(2)?,
                    on_date: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    })
    .await
}

pub async fn exam_add(
    db: &Database,
    clock: &dyn Clock,
    course_id: Option<String>,
    title: &str,
    on_date: &str,
) -> Result<Exam> {
    let title = title.trim().to_owned();
    if title.is_empty() || title.chars().count() > EXAM_TITLE_MAX {
        return Err(AppError::validation(
            "title",
            format!("Give the exam a name of up to {EXAM_TITLE_MAX} characters."),
        ));
    }
    parse_day("on_date", on_date)?;
    let (on_date, now) = (on_date.to_owned(), clock.now_ms());
    db.write(move |tx| {
        let exam = Exam {
            id: new_id(),
            course_id,
            title,
            on_date,
        };
        tx.execute(
            "INSERT INTO exams (id, course_id, title, on_date, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![exam.id, exam.course_id, exam.title, exam.on_date, now],
        )
        .map_err(|e| match e {
            rusqlite::Error::SqliteFailure(f, _)
                if f.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                AppError::not_found("That course no longer exists.")
            }
            other => other.into(),
        })?;
        Ok((exam, vec![]))
    })
    .await
}

pub async fn exam_delete(db: &Database, id: &str) -> Result<()> {
    let id = id.to_owned();
    db.write(move |tx| {
        if tx.execute("DELETE FROM exams WHERE id = ?1", [&id])? == 0 {
            return Err(AppError::not_found("That exam no longer exists."));
        }
        Ok(((), vec![]))
    })
    .await
}

async fn set_link(
    db: &Database,
    table: &'static str,
    col: &'static str,
    item: &str,
    course: Option<String>,
) -> Result<()> {
    let item = item.to_owned();
    db.write(move |tx| {
        match course {
            None => {
                tx.execute(&format!("DELETE FROM {table} WHERE {col} = ?1"), [&item])?;
            }
            Some(c) => {
                tx.execute(
                    &format!(
                        "INSERT INTO {table} ({col}, course_id) VALUES (?1, ?2)
                         ON CONFLICT({col}) DO UPDATE SET course_id = excluded.course_id"
                    ),
                    params![item, c],
                )
                .map_err(|e| match e {
                    rusqlite::Error::SqliteFailure(f, _)
                        if f.code == rusqlite::ErrorCode::ConstraintViolation =>
                    {
                        AppError::not_found("That course or item no longer exists.")
                    }
                    other => other.into(),
                })?;
            }
        }
        Ok(((), vec![]))
    })
    .await
}

/// Put a task in a course, or take it out with `None`.
pub async fn set_task_course(db: &Database, task_id: &str, course: Option<String>) -> Result<()> {
    set_link(db, "task_course", "task_id", task_id, course).await
}

pub async fn set_note_course(db: &Database, note_id: &str, course: Option<String>) -> Result<()> {
    set_link(db, "note_course", "note_id", note_id, course).await
}

pub async fn links(db: &Database) -> Result<CourseLinks> {
    db.read(|conn| {
        let read = |sql: &str| -> Result<Vec<ItemCourse>> {
            let mut stmt = conn.prepare(sql)?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(ItemCourse {
                        item_id: r.get(0)?,
                        course_id: r.get(1)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        };
        Ok(CourseLinks {
            tasks: read("SELECT task_id, course_id FROM task_course")?,
            notes: read("SELECT note_id, course_id FROM note_course")?,
        })
    })
    .await
}

/// The seven days starting at `from`: focus time (finished focus sessions, minus paused time) and
/// tasks completed, per course. Work on tasks with no course is the `course: None` row.
pub async fn weekly_report(db: &Database, clock: &dyn Clock, from: &str) -> Result<WeeklyReport> {
    let start = parse_day("from", from)?;
    let end = start + Days::new(6);
    let tz = clock.tz();
    let bound = |d: NaiveDate| {
        tz.from_local_datetime(&d.and_hms_opt(0, 0, 0).expect("midnight"))
            .earliest()
            .map_or(0, |t| t.timestamp_millis())
    };
    let (lo, hi) = (bound(start), bound(end + Days::new(1)));
    let (from_s, to_s) = (start.to_string(), end.to_string());
    let (from_q, to_q) = (from_s.clone(), to_s.clone());
    let courses = db
        .read(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT c.id, c.name,
                        (SELECT COALESCE(SUM(MAX(f.ended_at - f.started_at - f.paused_ms, 0)), 0)
                           FROM focus_sessions f JOIN task_course tc ON tc.task_id = f.task_id
                          WHERE tc.course_id = c.id AND f.kind = 'FOCUS' AND f.ended_at IS NOT NULL
                            AND f.local_date BETWEEN ?1 AND ?2),
                        (SELECT COUNT(*) FROM tasks t JOIN task_course tc ON tc.task_id = t.id
                          WHERE tc.course_id = c.id AND t.completed_at >= ?3 AND t.completed_at < ?4)
                   FROM courses c ORDER BY c.name_folded",
            )?;
            let mut rows = stmt
                .query_map(params![from_q, to_q, lo, hi], |r| {
                    Ok(CourseWeek {
                        course: Some(Course {
                            id: r.get(0)?,
                            name: r.get(1)?,
                        }),
                        focus_ms: r.get(2)?,
                        tasks_done: r.get(3)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let (focus_ms, tasks_done): (i64, u32) = conn.query_row(
                "SELECT
                   (SELECT COALESCE(SUM(MAX(f.ended_at - f.started_at - f.paused_ms, 0)), 0)
                      FROM focus_sessions f
                     WHERE f.kind = 'FOCUS' AND f.ended_at IS NOT NULL
                       AND f.local_date BETWEEN ?1 AND ?2
                       AND (f.task_id IS NULL OR f.task_id NOT IN (SELECT task_id FROM task_course))),
                   (SELECT COUNT(*) FROM tasks t
                     WHERE t.completed_at >= ?3 AND t.completed_at < ?4
                       AND t.id NOT IN (SELECT task_id FROM task_course))",
                params![from_q, to_q, lo, hi],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            if focus_ms > 0 || tasks_done > 0 {
                rows.push(CourseWeek {
                    course: None,
                    focus_ms,
                    tasks_done,
                });
            }
            Ok(rows)
        })
        .await?;
    let total_focus_ms = courses.iter().map(|c| c.focus_ms).sum();
    let total_tasks_done = courses.iter().map(|c| c.tasks_done).sum();
    Ok(WeeklyReport {
        from: from_s,
        to: to_s,
        courses,
        total_focus_ms,
        total_tasks_done,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono_tz::Tz;

    use super::*;
    use crate::bus::EventBus;
    use crate::clock::FakeClock;
    use crate::error::ErrorCode;
    use crate::notes::{self, NoteInput};
    use crate::tasks;

    fn setup() -> (tempfile::TempDir, Arc<Database>, FakeClock) {
        let dir = tempfile::tempdir().unwrap();
        let db =
            Arc::new(Database::open(&dir.path().join("pluto.db"), EventBus::default()).unwrap());
        // 2026-10-28 12:00 UTC
        (dir, db, FakeClock::new(1_793_188_800_000, Tz::UTC))
    }

    #[tokio::test]
    async fn names_are_unique_in_any_casing_and_renaming_into_another_is_a_conflict() {
        let (_d, db, clock) = setup();
        let a = create(&db, &clock, " Physics ").await.unwrap();
        assert_eq!(create(&db, &clock, "PHYSICS").await.unwrap().id, a.id);
        let b = create(&db, &clock, "Maths").await.unwrap();
        let err = rename(&db, &b.id, "physics").await.unwrap_err();
        assert_eq!(err.code, ErrorCode::Conflict);
        assert_eq!(rename(&db, &a.id, "PHYSICS").await.unwrap().name, "PHYSICS");
        assert!(create(&db, &clock, "  ").await.is_err());
        assert!(create(&db, &clock, &"x".repeat(61)).await.is_err());
        assert_eq!(list(&db).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn exams_list_from_a_day_soonest_first_and_die_with_their_course() {
        let (_d, db, clock) = setup();
        let c = create(&db, &clock, "Physics").await.unwrap();
        exam_add(&db, &clock, Some(c.id.clone()), "Final", "2026-12-10")
            .await
            .unwrap();
        exam_add(&db, &clock, None, "Quiz", "2026-11-02")
            .await
            .unwrap();
        exam_add(&db, &clock, None, "Old", "2026-09-01")
            .await
            .unwrap();
        let titles: Vec<_> = exams_from(&db, "2026-10-28")
            .await
            .unwrap()
            .into_iter()
            .map(|e| e.title)
            .collect();
        assert_eq!(titles, ["Quiz", "Final"]);
        assert!(exam_add(&db, &clock, None, "", "2026-11-02").await.is_err());
        assert!(exam_add(&db, &clock, None, "X", "soon").await.is_err());
        assert_eq!(
            exam_add(&db, &clock, Some("gone".into()), "X", "2026-11-02")
                .await
                .unwrap_err()
                .code,
            ErrorCode::NotFound
        );
        delete(&db, &c.id).await.unwrap();
        assert_eq!(exams_from(&db, "2026-10-28").await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_task_or_note_has_one_course_and_deleting_the_course_keeps_the_item() {
        let (_d, db, clock) = setup();
        let a = create(&db, &clock, "Physics").await.unwrap();
        let b = create(&db, &clock, "Maths").await.unwrap();
        let t = tasks::quick_add(&db, &clock, "Read ch 3").await.unwrap();
        let n = notes::create(
            &db,
            &clock,
            NoteInput {
                title: "Lecture".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        set_task_course(&db, &t.id, Some(a.id.clone()))
            .await
            .unwrap();
        set_task_course(&db, &t.id, Some(b.id.clone()))
            .await
            .unwrap();
        set_note_course(&db, &n.id, Some(a.id.clone()))
            .await
            .unwrap();
        let l = links(&db).await.unwrap();
        assert_eq!(
            l.tasks,
            [ItemCourse {
                item_id: t.id.clone(),
                course_id: b.id.clone()
            }]
        );
        assert_eq!(l.notes.len(), 1);
        assert!(set_task_course(&db, &t.id, Some("nope".into()))
            .await
            .is_err());
        delete(&db, &b.id).await.unwrap();
        assert!(links(&db).await.unwrap().tasks.is_empty());
        assert!(tasks::get(&db, &t.id).await.is_ok(), "the task stays");
        set_note_course(&db, &n.id, None).await.unwrap();
        assert!(links(&db).await.unwrap().notes.is_empty());
    }

    #[tokio::test]
    async fn the_weekly_report_counts_focus_minus_pauses_and_done_tasks_per_course() {
        let (_d, db, clock) = setup();
        let phys = create(&db, &clock, "Physics").await.unwrap();
        create(&db, &clock, "Maths").await.unwrap();
        let t1 = tasks::quick_add(&db, &clock, "Problem set").await.unwrap();
        let t2 = tasks::quick_add(&db, &clock, "Laundry").await.unwrap();
        set_task_course(&db, &t1.id, Some(phys.id.clone()))
            .await
            .unwrap();
        let (t1id, t2id) = (t1.id.clone(), t2.id.clone());
        db.write(move |tx| {
            // 50 min focus (10 paused) on the physics task, in the week; one outside it; one on the free task.
            for (id, task, date, start, paused) in [
                ("f1", &t1id, "2026-10-27", 1_793_100_000_000_i64, 600_000_i64),
                ("f2", &t1id, "2026-10-12", 1_791_800_000_000, 0),
                ("f3", &t2id, "2026-10-26", 1_793_000_000_000, 0),
            ] {
                tx.execute(
                    "INSERT INTO focus_sessions (id, kind, task_id, planned_ms, started_at, paused_ms, ended_at, outcome, local_date)
                     VALUES (?1, 'FOCUS', ?2, 3600000, ?3, ?4, ?3 + 3600000, 'DONE', ?5)",
                    params![id, task, start, paused, date],
                )?;
            }
            tx.execute(
                "UPDATE tasks SET status = 'COMPLETED', completed_at = 1793100000000 WHERE id IN (?1, ?2)",
                params![t1id, t2id],
            )?;
            Ok(((), vec![]))
        })
        .await
        .unwrap();
        let r = weekly_report(&db, &clock, "2026-10-22").await.unwrap();
        assert_eq!(
            (r.from.as_str(), r.to.as_str()),
            ("2026-10-22", "2026-10-28")
        );
        let p = r
            .courses
            .iter()
            .find(|c| c.course.as_ref().is_some_and(|c| c.name == "Physics"))
            .unwrap();
        assert_eq!((p.focus_ms, p.tasks_done), (3_000_000, 1));
        let m = r
            .courses
            .iter()
            .find(|c| c.course.as_ref().is_some_and(|c| c.name == "Maths"))
            .unwrap();
        assert_eq!((m.focus_ms, m.tasks_done), (0, 0));
        let free = r.courses.iter().find(|c| c.course.is_none()).unwrap();
        assert_eq!((free.focus_ms, free.tasks_done), (3_600_000, 1));
        assert_eq!((r.total_focus_ms, r.total_tasks_done), (6_600_000, 2));
        assert!(weekly_report(&db, &clock, "x").await.is_err());
    }
}
