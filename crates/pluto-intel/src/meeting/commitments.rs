//! Commitments (T-503): "what did Ravi commit to?" and "what is overdue?" across all meetings.
//! Pure rules over the action items and their tasks; nothing is sent anywhere.

use chrono::NaiveDate;
use pluto_contracts::meeting::{Commitment, CommitmentStatus};
use pluto_core::meeting_ai_store::CommitmentRow;

fn status_of(row: &CommitmentRow, today: NaiveDate) -> Option<(CommitmentStatus, Option<i64>)> {
    let due = row
        .due_date
        .as_deref()
        .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
    let late = due.filter(|d| *d < today).map(|d| (today - d).num_days());
    match row.task_status.as_deref() {
        Some("COMPLETED") => Some((CommitmentStatus::Done, None)),
        // A cancelled task means the commitment was dropped on purpose.
        Some("CANCELLED") => None,
        Some(_) => Some(late.map_or((CommitmentStatus::Open, None), |n| {
            (CommitmentStatus::Overdue, Some(n))
        })),
        None => Some(late.map_or((CommitmentStatus::NoTask, None), |n| {
            (CommitmentStatus::Overdue, Some(n))
        })),
    }
}

/// The commitments, filtered, most urgent first: overdue (longest first), then by due date, done last.
pub fn list(
    rows: Vec<CommitmentRow>,
    today: NaiveDate,
    owner: Option<&str>,
    status: Option<CommitmentStatus>,
) -> Vec<Commitment> {
    let mut out: Vec<Commitment> = rows
        .into_iter()
        .filter(|r| {
            owner.is_none_or(|o| {
                r.owner
                    .as_deref()
                    .is_some_and(|x| x.eq_ignore_ascii_case(o.trim()))
            })
        })
        .filter_map(|r| {
            let (st, days_overdue) = status_of(&r, today)?;
            Some(Commitment {
                action_item_id: r.action_item_id,
                meeting_id: r.meeting_id,
                meeting_title: r.meeting_title,
                owner: r.owner,
                body: r.body,
                due_date: r.due_date,
                task_id: r.task_id,
                status: st,
                days_overdue,
            })
        })
        .filter(|c| status.is_none_or(|s| s == c.status))
        .collect();
    let rank = |c: &Commitment| match c.status {
        CommitmentStatus::Overdue => 0,
        CommitmentStatus::Open | CommitmentStatus::NoTask => 1,
        CommitmentStatus::Done => 2,
    };
    // Stable: ties keep the newest-meeting-first order they came in.
    out.sort_by(|a, b| {
        rank(a)
            .cmp(&rank(b))
            .then_with(|| match (a.status, b.status) {
                (CommitmentStatus::Overdue, CommitmentStatus::Overdue) => {
                    b.days_overdue.cmp(&a.days_overdue)
                }
                _ => a
                    .due_date
                    .is_none()
                    .cmp(&b.due_date.is_none())
                    .then_with(|| a.due_date.cmp(&b.due_date)),
            })
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const TODAY: &str = "2026-11-10";
    fn today() -> NaiveDate {
        NaiveDate::parse_from_str(TODAY, "%Y-%m-%d").unwrap()
    }
    fn row(id: &str, owner: Option<&str>, due: Option<&str>, task: Option<&str>) -> CommitmentRow {
        CommitmentRow {
            action_item_id: id.into(),
            meeting_id: "m".into(),
            meeting_title: "Sync".into(),
            owner: owner.map(Into::into),
            body: format!("do {id}"),
            due_date: due.map(Into::into),
            task_id: task.map(|_| format!("t-{id}")),
            task_status: task.map(Into::into),
        }
    }

    #[test]
    fn statuses_follow_the_task_and_the_date() {
        let rows = vec![
            row("done", Some("Ravi"), Some("2026-11-01"), Some("COMPLETED")),
            row(
                "late_task",
                Some("Ravi"),
                Some("2026-11-05"),
                Some("IN_PROGRESS"),
            ),
            row(
                "ok_task",
                Some("Meena"),
                Some("2026-11-12"),
                Some("PLANNED"),
            ),
            row("no_task", None, Some("2026-11-20"), None),
            row("late_no_task", Some("Ravi"), Some("2026-11-08"), None),
            row("dropped", Some("Ravi"), None, Some("CANCELLED")),
            row("undated", Some("Meena"), None, None),
        ];
        let all = list(rows, today(), None, None);
        let st = |id: &str| {
            all.iter()
                .find(|c| c.action_item_id == id)
                .map(|c| (c.status, c.days_overdue))
        };
        assert_eq!(st("done"), Some((CommitmentStatus::Done, None)));
        assert_eq!(st("late_task"), Some((CommitmentStatus::Overdue, Some(5))));
        assert_eq!(st("ok_task"), Some((CommitmentStatus::Open, None)));
        assert_eq!(st("no_task"), Some((CommitmentStatus::NoTask, None)));
        assert_eq!(
            st("late_no_task"),
            Some((CommitmentStatus::Overdue, Some(2)))
        );
        assert_eq!(st("undated"), Some((CommitmentStatus::NoTask, None)));
        assert_eq!(
            st("dropped"),
            None,
            "a cancelled task is a dropped commitment"
        );
    }

    #[test]
    fn most_urgent_first_and_done_last() {
        let all = list(
            vec![
                row("done", None, Some("2026-11-01"), Some("COMPLETED")),
                row("soon", None, Some("2026-11-11"), Some("PLANNED")),
                row("later", None, Some("2026-11-30"), Some("PLANNED")),
                row("late2", None, Some("2026-11-08"), Some("PLANNED")),
                row("late5", None, Some("2026-11-05"), Some("PLANNED")),
                row("undated", None, None, None),
            ],
            today(),
            None,
            None,
        );
        assert_eq!(
            all.iter()
                .map(|c| c.action_item_id.as_str())
                .collect::<Vec<_>>(),
            ["late5", "late2", "soon", "later", "undated", "done"]
        );
    }

    #[test]
    fn filters_by_owner_ignoring_case_and_by_status() {
        let rows = || {
            vec![
                row("a", Some("Ravi"), Some("2026-11-05"), Some("PLANNED")),
                row("b", Some("Meena"), Some("2026-11-30"), Some("PLANNED")),
                row("c", None, None, None),
            ]
        };
        assert_eq!(list(rows(), today(), Some(" ravi "), None).len(), 1);
        assert!(list(rows(), today(), Some("Nobody"), None).is_empty());
        let overdue = list(rows(), today(), None, Some(CommitmentStatus::Overdue));
        assert_eq!(
            overdue
                .iter()
                .map(|c| c.action_item_id.as_str())
                .collect::<Vec<_>>(),
            ["a"]
        );
        assert_eq!(
            list(rows(), today(), None, Some(CommitmentStatus::NoTask)).len(),
            1
        );
    }
}
