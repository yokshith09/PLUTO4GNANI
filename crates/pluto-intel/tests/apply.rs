//! T-204: applying approved items to a real database through the base's meeting and task code.

use chrono_tz::Tz;
use pluto_contracts::meeting::{
    ApplyExtraction, EditedItem, ExtractedItem, ExtractionResult, Relevance,
};
use pluto_core::bus::EventBus;
use pluto_core::clock::FakeClock;
use pluto_core::db::Database;
use pluto_core::meeting_ai_store as store;
use pluto_core::meetings::{self, MeetingInput};
use pluto_core::tasks::{self, Priority, TaskFilters};
use pluto_intel::meeting::apply::apply;

struct Rig {
    _dir: tempfile::TempDir,
    db: Database,
    clock: FakeClock,
    meeting_id: String,
    extraction: ExtractionResult,
}

fn item(
    id: &str,
    kind: Relevance,
    body: &str,
    seg: &str,
    owner: Option<&str>,
    due: Option<&str>,
) -> ExtractedItem {
    ExtractedItem {
        id: id.into(),
        kind,
        body: body.into(),
        owner: owner.map(Into::into),
        due_phrase: due.map(|_| "Friday".into()),
        due_date: due.map(Into::into),
        priority: None,
        source_segment_id: seg.into(),
        quote: format!("quote of {id}"),
        duplicate_of_task_id: None,
    }
}

async fn rig() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("pluto.db"), EventBus::default()).unwrap();
    let noon = chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2026, 11, 2, 6, 30, 0)
        .unwrap()
        .timestamp_millis();
    let clock = FakeClock::new(noon, Tz::Asia__Kolkata);
    let m = meetings::create(
        &db,
        &clock,
        MeetingInput {
            title: "Launch sync".into(),
            participants: vec!["Asha".into(), "Ravi".into()],
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let mid = m.id.clone();
    db.write({
        let mid = mid.clone();
        move |tx| {
            let seg = |id: &str, i: u32, t: &str| store::SegmentRow {
                id: id.into(),
                meeting_id: mid.clone(),
                index: i,
                start_ms: 0,
                end_ms: 1,
                speaker_label: Some(1),
                participant_position: None,
                text: t.into(),
                language: None,
                relevance: None,
            };
            store::replace_segments(
                tx,
                &mid,
                &[
                    seg("s1", 0, "annual plan"),
                    seg("s2", 1, "copy by friday"),
                    seg("s3", 2, "legal late"),
                    seg("s4", 3, "who owns it"),
                ],
            )?;
            Ok(((), vec![]))
        }
    })
    .await
    .unwrap();
    let extraction = ExtractionResult {
        meeting_id: mid.clone(),
        items: vec![
            item(
                "x1",
                Relevance::Decision,
                "Start with the annual plan.",
                "s1",
                None,
                None,
            ),
            item(
                "x2",
                Relevance::ActionItem,
                "Finish the pricing copy.",
                "s2",
                Some("Ravi"),
                Some("2026-11-06"),
            ),
            item(
                "x3",
                Relevance::Risk,
                "Legal may be late.",
                "s3",
                None,
                None,
            ),
            item(
                "x4",
                Relevance::Question,
                "Who owns the contract?",
                "s4",
                None,
                None,
            ),
        ],
        warnings: vec![],
        dominant_language: None,
    };
    Rig {
        _dir: dir,
        db,
        clock,
        meeting_id: mid,
        extraction,
    }
}

fn ed(id: &str, body: &str, owner: Option<&str>, due: Option<&str>, task: bool) -> EditedItem {
    EditedItem {
        item_id: id.into(),
        body: body.into(),
        owner: owner.map(Into::into),
        due_date: due.map(Into::into),
        priority: None,
        create_task: task,
    }
}

fn all(r: &Rig) -> ApplyExtraction {
    ApplyExtraction {
        meeting_id: r.meeting_id.clone(),
        accepted: vec![
            ed("x1", "Start with the annual plan.", None, None, false),
            ed(
                "x2",
                "Finish the pricing copy.",
                Some("Ravi"),
                Some("2026-11-06"),
                true,
            ),
            ed("x3", "Legal may be late.", None, None, false),
            ed("x4", "Who owns the contract?", None, None, false),
        ],
    }
}

#[tokio::test]
async fn approved_items_become_rows_with_provenance_and_a_task_with_its_own_due_date() {
    let r = rig().await;
    let out = apply(&r.db, &r.clock, &r.extraction, &all(&r))
        .await
        .unwrap();
    assert_eq!(out.tasks_created, 1);
    assert_eq!(out.meeting.decisions.len(), 1);
    let a = &out.meeting.action_items[0];
    assert_eq!(
        (a.body.as_str(), a.assignee.as_deref()),
        ("Finish the pricing copy.", Some("Ravi"))
    );
    let task = tasks::get(&r.db, a.task_id.as_ref().unwrap())
        .await
        .unwrap();
    assert_eq!(
        task.due_date.map(|d| d.to_string()).as_deref(),
        Some("2026-11-06"),
        "the item's date, not the meeting's"
    );

    let (aid, did, mid) = (
        a.id.clone(),
        out.meeting.decisions[0].id.clone(),
        r.meeting_id.clone(),
    );
    let (meta, insights, dec_src) =
        r.db.read(move |c| {
            let dec: Option<String> = c
                .query_row(
                    "SELECT source_segment_id FROM decision_meta WHERE decision_id = ?1",
                    [&did],
                    |x| x.get(0),
                )
                .unwrap();
            Ok((
                store::get_action_meta(c, &aid)?,
                store::list_insights(c, &mid)?,
                dec,
            ))
        })
        .await
        .unwrap();
    let meta = meta.unwrap();
    assert_eq!(
        (
            meta.source_segment_id.as_deref(),
            meta.quote.as_str(),
            meta.due_date.as_deref()
        ),
        (Some("s2"), "quote of x2", Some("2026-11-06"))
    );
    assert_eq!(dec_src.as_deref(), Some("s1"));
    assert_eq!(
        insights.iter().map(|i| i.kind.as_str()).collect::<Vec<_>>(),
        ["risk", "question"]
    );
}

#[tokio::test]
async fn applying_twice_adds_nothing_and_edits_update_in_place() {
    let r = rig().await;
    apply(&r.db, &r.clock, &r.extraction, &all(&r))
        .await
        .unwrap();
    let mut again = all(&r);
    again.accepted[1] = ed(
        "x2",
        "Finish pricing copy and FAQ.",
        Some("Ravi"),
        Some("2026-11-05"),
        true,
    );
    let out = apply(&r.db, &r.clock, &r.extraction, &again).await.unwrap();
    assert_eq!(out.tasks_created, 0, "the task already exists");
    assert_eq!(
        (out.meeting.decisions.len(), out.meeting.action_items.len()),
        (1, 1)
    );
    assert_eq!(
        out.meeting.action_items[0].body,
        "Finish pricing copy and FAQ."
    );
    let n = tasks::query(
        &r.db,
        &r.clock,
        tasks::TaskView::All,
        TaskFilters::default(),
    )
    .await
    .unwrap()
    .len();
    assert_eq!(n, 1, "still one task");
    let task = tasks::get(&r.db, out.meeting.action_items[0].task_id.as_ref().unwrap())
        .await
        .unwrap();
    assert_eq!(
        task.due_date.map(|d| d.to_string()).as_deref(),
        Some("2026-11-05"),
        "date edit reaches the task"
    );
}

#[tokio::test]
async fn items_not_marked_create_task_stay_plain_action_items() {
    let r = rig().await;
    let mut input = all(&r);
    input.accepted[1].create_task = false;
    let out = apply(&r.db, &r.clock, &r.extraction, &input).await.unwrap();
    assert_eq!(out.tasks_created, 0);
    assert!(out.meeting.action_items[0].task_id.is_none());
}

#[tokio::test]
async fn existing_rows_the_user_wrote_by_hand_are_kept() {
    let r = rig().await;
    let mut m = meetings::get(&r.db, &r.meeting_id).await.unwrap();
    m.decisions.push(meetings::Decision {
        id: "manual".into(),
        body: "Typed by hand".into(),
    });
    meetings::update(
        &r.db,
        &r.clock,
        &r.meeting_id,
        MeetingInput {
            title: m.title.clone(),
            starts_at: Some(m.starts_at),
            participants: m.participants.clone(),
            decisions: vec![meetings::DecisionInput {
                id: None,
                body: "Typed by hand".into(),
            }],
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let out = apply(&r.db, &r.clock, &r.extraction, &all(&r))
        .await
        .unwrap();
    assert_eq!(out.meeting.decisions.len(), 2);
    assert!(out
        .meeting
        .decisions
        .iter()
        .any(|d| d.body == "Typed by hand"));
}

#[tokio::test]
async fn bad_input_changes_nothing() {
    let r = rig().await;
    let mut input = all(&r);
    input.accepted[1].due_date = Some("Friday".into());
    assert!(
        apply(&r.db, &r.clock, &r.extraction, &input).await.is_err(),
        "text is not a date"
    );
    let mut unknown = all(&r);
    unknown.accepted.push(ed("zzz", "Ghost", None, None, false));
    assert!(apply(&r.db, &r.clock, &r.extraction, &unknown)
        .await
        .is_err());
    let mut dup = all(&r);
    dup.accepted.push(ed("x1", "Again", None, None, false));
    assert!(apply(&r.db, &r.clock, &r.extraction, &dup).await.is_err());
    let mut wrong = all(&r);
    wrong.meeting_id = "other".into();
    assert!(apply(&r.db, &r.clock, &r.extraction, &wrong).await.is_err());
    let m = meetings::get(&r.db, &r.meeting_id).await.unwrap();
    assert!(
        m.decisions.is_empty() && m.action_items.is_empty(),
        "nothing was written"
    );
    assert_eq!(Priority::High, Priority::High);
}
