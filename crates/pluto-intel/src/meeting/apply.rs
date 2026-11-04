//! Review and apply (T-204): the items the user approved become the meeting's decisions and action
//! items, with provenance, and the ones marked "create task" become tasks with their own due date.
//!
//! Everything goes through the base's own functions (`meetings::update`, `convert_action_item`,
//! `tasks::update`), so events, validation and the task link behave as everywhere else. Applying
//! the same extraction again reuses the rows it made before (matched by source segment) instead of
//! adding copies; items the user earlier applied and now leaves out stay (nothing is deleted).

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use pluto_contracts::error::{VoiceError, VoiceErrorCode, VoiceResult};
use pluto_contracts::meeting::{ApplyExtraction, ExtractedItem, ExtractionResult, Relevance};
use pluto_core::clock::Clock;
use pluto_core::db::Database;
use pluto_core::error::AppError;
use pluto_core::ids::new_id;
use pluto_core::meeting_ai_store as store;
use pluto_core::meetings::{self, ActionItemInput, DecisionInput, Meeting, MeetingInput};
use pluto_core::tasks::{self, TaskPatch};

#[derive(Debug, Clone)]
pub struct ApplyOutcome {
    pub meeting: Meeting,
    pub tasks_created: u32,
}

fn core(e: AppError) -> VoiceError {
    VoiceError::new(VoiceErrorCode::Core, e.message)
}

fn invalid(message: &str) -> VoiceError {
    VoiceError::new(VoiceErrorCode::Core, message)
}

fn parse_date(s: &str) -> VoiceResult<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| invalid("A due date must look like 2026-11-06."))
}

/// One approved item, ready to be written to the provenance tables.
struct Write {
    kind: Relevance,
    id: String,
    original: ExtractedItem,
    body: String,
    due: Option<String>,
    priority: Option<String>,
}

struct Chosen<'a> {
    edited: &'a pluto_contracts::meeting::EditedItem,
    original: &'a ExtractedItem,
    /// The row's id after this apply (existing or new).
    id: String,
    due: Option<NaiveDate>,
}

pub async fn apply(
    db: &Database,
    clock: &dyn Clock,
    extraction: &ExtractionResult,
    input: &ApplyExtraction,
) -> VoiceResult<ApplyOutcome> {
    if extraction.meeting_id != input.meeting_id {
        return Err(invalid("That review belongs to a different meeting."));
    }
    let by_id: HashMap<&str, &ExtractedItem> = extraction
        .items
        .iter()
        .map(|i| (i.id.as_str(), i))
        .collect();
    let mut seen = HashSet::new();
    let mut accepted = Vec::new();
    for e in &input.accepted {
        let original = by_id
            .get(e.item_id.as_str())
            .ok_or_else(|| invalid("One of the items is no longer in this review."))?;
        if !seen.insert(e.item_id.as_str()) {
            return Err(invalid("An item was sent twice."));
        }
        // Dates are checked before anything is written.
        let due = e.due_date.as_deref().map(parse_date).transpose()?;
        accepted.push((e, *original, due));
    }

    let meeting_id = input.meeting_id.clone();
    let meeting = meetings::get(db, &meeting_id).await.map_err(core)?;

    // Reuse rows from an earlier apply of the same segments.
    let existing: (HashMap<String, Vec<String>>, HashMap<String, Vec<String>>) = {
        let segs: Vec<String> = accepted
            .iter()
            .map(|(_, o, _)| o.source_segment_id.clone())
            .collect();
        let mid = meeting_id.clone();
        db.read(move |c| {
            let (mut d, mut a) = (HashMap::new(), HashMap::new());
            for s in segs {
                d.insert(s.clone(), store::find_decision_from(c, &mid, &s)?);
                a.insert(s.clone(), store::find_action_item_from(c, &mid, &s)?);
            }
            Ok((d, a))
        })
        .await
        .map_err(core)?
    };
    let (mut taken_d, mut taken_a): (HashSet<String>, HashSet<String>) =
        (HashSet::new(), HashSet::new());
    let mut chosen: Vec<Chosen> = Vec::new();
    for (edited, original, due) in accepted {
        let (pool, taken) = match original.kind {
            Relevance::Decision => (existing.0.get(&original.source_segment_id), &mut taken_d),
            Relevance::ActionItem => (existing.1.get(&original.source_segment_id), &mut taken_a),
            _ => (None, &mut taken_d),
        };
        let reuse = pool.and_then(|ids| ids.iter().find(|id| !taken.contains(*id)).cloned());
        if let Some(r) = &reuse {
            taken.insert(r.clone());
        }
        let id = reuse.clone().unwrap_or_else(new_id);
        chosen.push(Chosen {
            edited,
            original,
            id,
            due,
        });
    }

    // Keep everything the meeting already has; update the reused rows; add the new ones.
    let mut decisions: Vec<DecisionInput> = meeting
        .decisions
        .iter()
        .map(|d| DecisionInput {
            id: Some(d.id.clone()),
            body: d.body.clone(),
        })
        .collect();
    let mut actions: Vec<ActionItemInput> = meeting
        .action_items
        .iter()
        .map(|a| ActionItemInput {
            id: Some(a.id.clone()),
            body: a.body.clone(),
            assignee: a.assignee.clone(),
        })
        .collect();
    for c in &chosen {
        match c.original.kind {
            Relevance::Decision => {
                let row = DecisionInput {
                    id: Some(c.id.clone()),
                    body: c.edited.body.clone(),
                };
                match decisions.iter_mut().find(|d| d.id == row.id) {
                    Some(d) => *d = row,
                    None => decisions.push(row),
                }
            }
            Relevance::ActionItem => {
                let row = ActionItemInput {
                    id: Some(c.id.clone()),
                    body: c.edited.body.clone(),
                    assignee: c.edited.owner.clone(),
                };
                match actions.iter_mut().find(|a| a.id == row.id) {
                    Some(a) => *a = row,
                    None => actions.push(row),
                }
            }
            _ => {}
        }
    }
    let updated = meetings::update(
        db,
        clock,
        &meeting_id,
        MeetingInput {
            title: meeting.title.clone(),
            starts_at: Some(meeting.starts_at),
            notes: meeting.notes.clone(),
            follow_up_date: meeting.follow_up_date,
            participants: meeting.participants.clone(),
            decisions,
            action_items: actions,
        },
    )
    .await
    .map_err(core)?;

    // Provenance and insights, in one transaction.
    let now = clock.now_ms();
    let writes: Vec<Write> = chosen
        .iter()
        .map(|c| Write {
            kind: c.original.kind,
            id: c.id.clone(),
            original: c.original.clone(),
            body: c.edited.body.clone(),
            due: c.edited.due_date.clone(),
            priority: c.edited.priority.map(|p| format!("{p:?}").to_uppercase()),
        })
        .collect();
    let mid = meeting_id.clone();
    db.write(move |tx| {
        let mut insights = Vec::new();
        for Write {
            kind,
            id,
            original: orig,
            body,
            due,
            priority,
        } in &writes
        {
            match kind {
                Relevance::Decision => {
                    store::link_decision(tx, id, Some(&orig.source_segment_id), &orig.quote)?
                }
                Relevance::ActionItem => store::link_action_item(
                    tx,
                    &store::ActionMeta {
                        action_item_id: id.clone(),
                        source_segment_id: Some(orig.source_segment_id.clone()),
                        quote: orig.quote.clone(),
                        due_phrase: orig.due_phrase.clone(),
                        due_date: due.clone(),
                        priority: priority.clone(),
                        confidence: None,
                    },
                )?,
                Relevance::Question | Relevance::Risk => insights.push(store::InsightRow {
                    id: id.clone(),
                    kind: if *kind == Relevance::Question {
                        "question"
                    } else {
                        "risk"
                    }
                    .into(),
                    body: body.clone(),
                    source_segment_id: Some(orig.source_segment_id.clone()),
                }),
                Relevance::Chitchat => {}
            }
        }
        store::set_insights(tx, &mid, &insights, now)?;
        Ok(((), vec![]))
    })
    .await
    .map_err(core)?;

    // Tasks last: the base makes the task (planned today, meeting follow-up date); then the item's
    // own due date and priority replace the follow-up date.
    let mut tasks_created = 0;
    for c in chosen
        .iter()
        .filter(|c| c.original.kind == Relevance::ActionItem && c.edited.create_task)
    {
        let linked = updated
            .action_items
            .iter()
            .find(|a| a.id == c.id)
            .and_then(|a| a.task_id.clone());
        let task_id = match linked {
            Some(t) => t,
            None => {
                tasks_created += 1;
                meetings::convert_action_item(db, clock, &c.id)
                    .await
                    .map_err(core)?
                    .id
            }
        };
        tasks::update(
            db,
            clock,
            &task_id,
            TaskPatch {
                due_date: c.due.map(Some),
                priority: c.edited.priority.map(Some),
                ..Default::default()
            },
        )
        .await
        .map_err(core)?;
    }

    let meeting = meetings::get(db, &meeting_id).await.map_err(core)?;
    Ok(ApplyOutcome {
        meeting,
        tasks_created,
    })
}
