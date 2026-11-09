//! The meeting-AI service over a real database and a scripted LLM: the path the shell commands take.

use std::sync::Arc;

use chrono_tz::Tz;
use pluto_contracts::error::VoiceErrorCode;
use pluto_contracts::meeting::{ApplyExtraction, EditedItem, Relevance, SpeakerMapping};
use pluto_contracts::voice::Destination;
use pluto_core::bus::EventBus;
use pluto_core::clock::{Clock, FakeClock};
use pluto_core::db::Database;
use pluto_core::meeting_ai_store as store;
use pluto_core::meetings::{self, MeetingInput};
use pluto_core::voice_store;
use pluto_gnani::http::{OutboundLog, OutboundRecord};
use pluto_intel::llm::privacy::DbOutboundLog;
use pluto_intel::llm::MockLlm;
use pluto_intel::meeting::service::MeetingAi;
use serde_json::json;

struct Rig {
    _dir: tempfile::TempDir,
    db: Arc<Database>,
    clock: Arc<FakeClock>,
    llm: Arc<MockLlm>,
    ai: MeetingAi,
    meeting_id: String,
}

async fn rig() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(Database::open(&dir.path().join("pluto.db"), EventBus::default()).unwrap());
    let noon = chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2026, 11, 2, 6, 30, 0)
        .unwrap()
        .timestamp_millis();
    let clock = Arc::new(FakeClock::new(noon, Tz::Asia__Kolkata));
    let m = meetings::create(
        &db,
        &*clock,
        MeetingInput {
            title: "Launch sync".into(),
            participants: vec!["Asha".into(), "Ravi".into()],
            starts_at: Some(noon),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let mid = m.id.clone();
    db.write({
        let mid = mid.clone();
        move |tx| {
            let seg = |id: &str, i: u32, label: u8, t: &str| store::SegmentRow {
                id: id.into(),
                meeting_id: mid.clone(),
                index: i,
                start_ms: i64::from(i) * 1000,
                end_ms: i64::from(i) * 1000 + 900,
                speaker_label: Some(label),
                participant_position: None,
                text: t.into(),
                language: Some("hi-IN".into()),
                relevance: None,
            };
            store::replace_segments(
                tx,
                &mid,
                &[
                    seg("s1", 0, 1, "annual plan se start karenge"),
                    seg("s2", 1, 2, "copy Thursday tak complete kar dunga"),
                ],
            )?;
            Ok(((), vec![]))
        }
    })
    .await
    .unwrap();
    let llm = Arc::new(MockLlm::new());
    let ai = MeetingAi::new(db.clone(), clock.clone() as Arc<dyn Clock>, llm.clone());
    Rig {
        _dir: dir,
        db,
        clock,
        llm,
        ai,
        meeting_id: mid,
    }
}

#[tokio::test]
async fn the_transcript_gets_names_once_speakers_are_mapped() {
    let r = rig().await;
    let before = r.ai.transcript(&r.meeting_id).await.unwrap();
    assert_eq!(before.len(), 2);
    assert!(before.iter().all(|s| s.participant.is_none()));
    r.ai.set_speakers(
        &r.meeting_id,
        &[
            SpeakerMapping {
                speaker_label: 1,
                participant: Some("asha".into()),
            },
            SpeakerMapping {
                speaker_label: 2,
                participant: Some("Ravi".into()),
            },
        ],
    )
    .await
    .unwrap();
    let after = r.ai.transcript(&r.meeting_id).await.unwrap();
    assert_eq!(
        (
            after[0].participant.as_deref(),
            after[1].participant.as_deref()
        ),
        (Some("Asha"), Some("Ravi"))
    );
    assert_eq!(after[0].language.as_deref(), Some("hi-IN"));
    r.ai.set_speakers(
        &r.meeting_id,
        &[SpeakerMapping {
            speaker_label: 1,
            participant: None,
        }],
    )
    .await
    .unwrap();
    assert!(r.ai.transcript(&r.meeting_id).await.unwrap()[0]
        .participant
        .is_none());
}

#[tokio::test]
async fn a_name_outside_the_participants_is_refused() {
    let r = rig().await;
    let err =
        r.ai.set_speakers(
            &r.meeting_id,
            &[SpeakerMapping {
                speaker_label: 1,
                participant: Some("Stranger".into()),
            }],
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, VoiceErrorCode::Core);
}

#[tokio::test]
async fn extract_then_apply_then_items_round_trip() {
    let r = rig().await;
    r.llm.push_json(json!({"items": [
        {"kind":"decision","body":"Start with the annual plan.","segment_id":"s1","quote":"annual plan se start karenge"},
        {"kind":"action_item","body":"Finish the copy.","owner":"Ravi","due_phrase":"Thursday","segment_id":"s2","quote":"copy Thursday tak complete kar dunga"},
    ]}));
    let ex = r.ai.extract(&r.meeting_id).await.unwrap();
    assert_eq!(ex.items.len(), 2);
    assert_eq!(ex.items[1].due_date.as_deref(), Some("2026-11-05"));
    let prompt = &r.llm.requests()[0].messages[0].content;
    assert!(
        prompt.contains("Meeting date: 2026-11-02") && prompt.contains("Participants: Asha, Ravi")
    );

    let edited = |id: &str, body: &str, owner: Option<&str>, due: Option<&str>, task| EditedItem {
        item_id: id.into(),
        body: body.into(),
        owner: owner.map(Into::into),
        due_date: due.map(Into::into),
        priority: None,
        create_task: task,
    };
    let out =
        r.ai.apply(&ApplyExtraction {
            meeting_id: r.meeting_id.clone(),
            accepted: vec![
                edited(
                    &ex.items[0].id,
                    "Start with the annual plan.",
                    None,
                    None,
                    false,
                ),
                edited(
                    &ex.items[1].id,
                    "Finish the copy.",
                    Some("Ravi"),
                    Some("2026-11-05"),
                    true,
                ),
            ],
        })
        .await
        .unwrap();
    assert_eq!(out.tasks_created, 1);

    let items = r.ai.items(&r.meeting_id).await.unwrap();
    assert_eq!(
        items.iter().map(|i| i.kind).collect::<Vec<_>>(),
        [Relevance::Decision, Relevance::ActionItem]
    );
    assert_eq!(items[1].owner.as_deref(), Some("Ravi"));
    assert_eq!(items[1].due_date.as_deref(), Some("2026-11-05"));
    assert_eq!(items[1].quote, "copy Thursday tak complete kar dunga");
    assert_eq!(items[0].source_segment_id, "s1");
}

#[tokio::test]
async fn apply_needs_a_fresh_extraction_and_an_empty_transcript_cannot_be_extracted() {
    let r = rig().await;
    let err =
        r.ai.apply(&ApplyExtraction {
            meeting_id: r.meeting_id.clone(),
            accepted: vec![],
        })
        .await
        .unwrap_err();
    assert!(err.message.contains("expired"));

    let empty = meetings::create(
        &r.db,
        &*r.clock,
        MeetingInput {
            title: "Typed".into(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let err = r.ai.extract(&empty.id).await.unwrap_err();
    assert!(err.message.contains("no transcript"));
    assert!(
        r.llm.requests().is_empty(),
        "nothing is sent when there is nothing to read"
    );
}

#[tokio::test]
async fn outbound_calls_reach_the_privacy_log_without_content() {
    let r = rig().await;
    let log = DbOutboundLog::new(r.db.clone(), r.clock.clone() as Arc<dyn Clock>);
    log.record(OutboundRecord {
        destination: Destination::Groq,
        purpose: "intent".into(),
        bytes: 640,
        ref_id: None,
    });
    for _ in 0..50 {
        let n =
            r.db.read(|c| Ok(voice_store::privacy_log_list(c, 10)?.len()))
                .await
                .unwrap();
        if n > 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let rows =
        r.db.read(|c| voice_store::privacy_log_list(c, 10))
            .await
            .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (
            rows[0].destination.as_str(),
            rows[0].purpose.as_str(),
            rows[0].bytes
        ),
        ("groq", "intent", 640)
    );
}

#[tokio::test]
async fn commitments_follow_the_task_from_open_to_done() {
    use pluto_contracts::meeting::CommitmentStatus;
    use pluto_core::tasks::{self, TaskStatus};
    let r = rig().await;
    r.llm.push_json(json!({"items": [
        {"kind":"action_item","body":"Finish the copy.","owner":"Ravi","due_phrase":"Thursday","segment_id":"s2","quote":"copy Thursday tak complete kar dunga"},
        {"kind":"action_item","body":"Call the vendor.","owner":"Asha","segment_id":"s1","quote":"annual plan se start karenge"},
    ]}));
    let ex = r.ai.extract(&r.meeting_id).await.unwrap();
    let e = |i: usize, task| EditedItem {
        item_id: ex.items[i].id.clone(),
        body: ex.items[i].body.clone(),
        owner: ex.items[i].owner.clone(),
        due_date: ex.items[i].due_date.clone(),
        priority: None,
        create_task: task,
    };
    let out =
        r.ai.apply(&ApplyExtraction {
            meeting_id: r.meeting_id.clone(),
            accepted: vec![e(0, true), e(1, false)],
        })
        .await
        .unwrap();
    assert_eq!(out.tasks_created, 1);

    let all = r.ai.commitments(None, None).await.unwrap();
    assert_eq!(all.len(), 2);
    let ravi = r.ai.commitments(Some("ravi"), None).await.unwrap();
    assert_eq!(
        (ravi.len(), ravi[0].status, ravi[0].due_date.as_deref()),
        (1, CommitmentStatus::Open, Some("2026-11-05"))
    );
    let asha = r.ai.commitments(Some("Asha"), None).await.unwrap();
    assert_eq!(
        (asha[0].status, asha[0].task_id.clone()),
        (CommitmentStatus::NoTask, None)
    );

    let task_id = ravi[0].task_id.clone().unwrap();
    tasks::transition(&r.db, &*r.clock, &task_id, TaskStatus::Completed)
        .await
        .unwrap();
    assert_eq!(
        r.ai.commitments(Some("Ravi"), None).await.unwrap()[0].status,
        CommitmentStatus::Done
    );
    assert!(r
        .ai
        .commitments(None, Some(CommitmentStatus::Open))
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn a_typed_meeting_has_no_job_and_a_sample_import_runs_the_whole_journey() {
    use pluto_contracts::meeting::{JobStatus, SegmentSource, TranscriptSegment};
    let r = rig().await;
    // The rig's meeting has segments but no job row: like a hand-typed meeting it has no job.
    assert!(r.ai.job(&r.meeting_id).await.unwrap().is_none());

    let sample: Vec<TranscriptSegment> = (0..3u32)
        .map(|i| TranscriptSegment {
            id: format!("fixture-{i}"),
            meeting_id: None,
            source: SegmentSource::Batch,
            index: i,
            start_ms: Some(i64::from(i) * 1000),
            end_ms: Some(i64::from(i) * 1000 + 900),
            speaker_label: Some(1 + (i % 2) as u8),
            participant: Some("must be ignored".into()),
            text: format!("line {i}"),
            language: Some("hi-IN".into()),
            relevance: None,
        })
        .collect();
    let job =
        r.ai.import_with_sample(
            "  Launch review ",
            vec!["Asha".into(), "Ravi".into()],
            sample,
        )
        .await
        .unwrap();
    assert_eq!(job.status, JobStatus::Completed);
    let mid = job.meeting_id.clone();

    let got = r.ai.job(&mid).await.unwrap().unwrap();
    assert_eq!((got.id, got.status), (job.id, JobStatus::Completed));
    let lines = r.ai.transcript(&mid).await.unwrap();
    assert_eq!(lines.len(), 3);
    assert!(
        lines.iter().all(|l| l.participant.is_none()),
        "speakers start unnamed"
    );
    assert!(lines
        .iter()
        .all(|l| l.meeting_id.as_deref() == Some(mid.as_str())));
    let m = pluto_core::meetings::get(&r.db, &mid).await.unwrap();
    assert_eq!(
        (m.title.as_str(), m.participants.len()),
        ("Launch review", 2)
    );
    // Another import does not touch the first one.
    let again = r.ai.import_with_sample("", vec![], vec![]).await.unwrap();
    assert_ne!(again.meeting_id, mid);
    assert_eq!(
        pluto_core::meetings::get(&r.db, &again.meeting_id)
            .await
            .unwrap()
            .title,
        "Imported meeting"
    );
}

// ---- auto mode (Google Meet): everything found is written into the meeting, no review ----------

#[tokio::test]
async fn auto_process_writes_decisions_actions_and_a_summary_without_creating_tasks() {
    let r = rig().await;
    r.llm.push_json(json!({"items": [
        {"kind":"decision","body":"Start with the annual plan.","segment_id":"s1","quote":"annual plan se start karenge"},
        {"kind":"action_item","body":"Finish the copy.","owner":"Ravi","due_phrase":"Thursday","segment_id":"s2","quote":"copy Thursday tak complete kar dunga"},
        {"kind":"risk","body":"The copy may slip.","segment_id":"s2","quote":"copy Thursday tak complete kar dunga"},
    ]}));
    r.llm.push_json(
        json!({"recap": "We start with the annual plan. Ravi finishes the copy by Thursday."}),
    );

    let out = r.ai.auto_process(&r.meeting_id).await.unwrap();
    assert_eq!(
        (out.decisions, out.action_items, out.other, out.summary),
        (1, 1, 1, true)
    );

    let m = meetings::get(&r.db, &r.meeting_id).await.unwrap();
    assert_eq!(m.decisions.len(), 1);
    assert_eq!(m.action_items.len(), 1);
    assert_eq!(m.action_items[0].assignee.as_deref(), Some("Ravi"));
    assert!(m.notes.starts_with("Summary\n"), "{}", m.notes);
    assert!(m.notes.contains("annual plan"));
    // Nothing is pushed into Tasks: the person decides later.
    assert_eq!(m.action_items[0].task_id, None);
    let all = pluto_core::tasks::query(
        &r.db,
        &*r.clock,
        pluto_core::tasks::TaskView::All,
        pluto_core::tasks::TaskFilters::default(),
    )
    .await
    .unwrap();
    assert!(all.is_empty(), "no tasks were created");
}

#[tokio::test]
async fn auto_process_keeps_existing_notes_and_survives_a_busy_summary_service() {
    let r = rig().await;
    // Notes the person already wrote stay, and the summary goes under them.
    let m = meetings::get(&r.db, &r.meeting_id).await.unwrap();
    meetings::update(
        &r.db,
        &*r.clock,
        &r.meeting_id,
        MeetingInput {
            title: m.title,
            starts_at: Some(m.starts_at),
            notes: "My own notes".into(),
            follow_up_date: None,
            participants: m.participants,
            decisions: vec![],
            action_items: vec![],
        },
    )
    .await
    .unwrap();
    r.llm.push_json(json!({"items": [
        {"kind":"decision","body":"Start with the annual plan.","segment_id":"s1","quote":"annual plan se start karenge"},
    ]}));
    r.llm
        .push_json(json!({"recap": "We start with the annual plan."}));
    let out = r.ai.auto_process(&r.meeting_id).await.unwrap();
    assert!(out.summary);
    let m = meetings::get(&r.db, &r.meeting_id).await.unwrap();
    assert!(
        m.notes.starts_with("My own notes\n\nSummary\n"),
        "{}",
        m.notes
    );

    // The summary service failing does not lose the items.
    let r2 = rig().await;
    r2.llm.push_json(json!({"items": [
        {"kind":"decision","body":"Start with the annual plan.","segment_id":"s1","quote":"annual plan se start karenge"},
    ]}));
    // No second reply is queued, so the recap call fails.
    let out = r2.ai.auto_process(&r2.meeting_id).await.unwrap();
    assert_eq!((out.decisions, out.summary), (1, false));
    let m = meetings::get(&r2.db, &r2.meeting_id).await.unwrap();
    assert_eq!(m.decisions.len(), 1);
    assert!(m.notes.is_empty());
}

#[tokio::test]
async fn auto_process_with_nothing_found_leaves_the_meeting_alone() {
    let r = rig().await;
    r.llm.push_json(json!({"items": []}));
    let out = r.ai.auto_process(&r.meeting_id).await.unwrap();
    assert_eq!(out, pluto_intel::meeting::service::AutoSummary::default());
    let m = meetings::get(&r.db, &r.meeting_id).await.unwrap();
    assert!(m.decisions.is_empty() && m.action_items.is_empty() && m.notes.is_empty());
    // And a meeting with no transcript is an error, not an empty success.
    let blank = meetings::create(
        &r.db,
        &*r.clock,
        MeetingInput {
            title: "Blank".into(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(r.ai.auto_process(&blank.id).await.is_err());
}
