//! T-305: recall is grounded. Answers come only from the user's transcripts, with real citations.

use std::sync::Arc;

use chrono_tz::Tz;
use pluto_contracts::error::VoiceErrorCode;
use pluto_core::bus::EventBus;
use pluto_core::clock::{Clock, FakeClock};
use pluto_core::db::Database;
use pluto_core::meeting_ai_store as store;
use pluto_core::meetings::{self, MeetingInput};
use pluto_intel::llm::MockLlm;
use pluto_intel::meeting::recall::NOT_FOUND;
use pluto_intel::meeting::service::MeetingAi;
use serde_json::json;

const START: i64 = 1_762_070_000_000;

async fn rig() -> (tempfile::TempDir, Arc<MockLlm>, MeetingAi) {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(Database::open(&dir.path().join("pluto.db"), EventBus::default()).unwrap());
    let clock = Arc::new(FakeClock::new(START, Tz::Asia__Kolkata));
    let m = meetings::create(
        &db,
        &*clock,
        MeetingInput {
            title: "Launch sync".into(),
            participants: vec!["Asha".into(), "Ravi".into()],
            starts_at: Some(START),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let other = meetings::create(
        &db,
        &*clock,
        MeetingInput {
            title: "Offsite".into(),
            starts_at: Some(START + 86_400_000),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    db.write({
        let (a, b) = (m.id.clone(), other.id.clone());
        move |tx| {
            let seg = |mid: &str, id: &str, i: u32, pos: Option<u32>, t: &str| store::SegmentRow {
                id: id.into(),
                meeting_id: mid.into(),
                index: i,
                start_ms: i64::from(i) * 5000,
                end_ms: i64::from(i) * 5000 + 4000,
                speaker_label: Some(1),
                participant_position: pos,
                text: t.into(),
                language: Some("hi-IN".into()),
                relevance: None,
            };
            store::replace_segments(
                tx,
                &a,
                &[
                    seg(
                        &a,
                        "s1",
                        0,
                        Some(0),
                        "launch 20 November ko hoga, yeh final hai",
                    ),
                    seg(&a, "s2", 1, Some(1), "pricing page copy Thursday tak"),
                ],
            )?;
            store::replace_segments(
                tx,
                &b,
                &[seg(&b, "s3", 0, None, "offsite venue book karna hai")],
            )?;
            Ok(((), vec![]))
        }
    })
    .await
    .unwrap();
    let llm = Arc::new(MockLlm::new());
    (
        dir,
        llm.clone(),
        MeetingAi::new(db, clock as Arc<dyn Clock>, llm),
    )
}

#[tokio::test]
async fn a_grounded_answer_carries_real_citations() {
    let (_d, llm, ai) = rig().await;
    llm.push_json(
        json!({"found": true, "answer": "The launch is on 20 November.", "sources": ["S1"]}),
    );
    let a = ai
        .recall("Launch date kya decide hui thi?", Some("hi-IN"))
        .await
        .unwrap();
    assert_eq!(
        (a.answer_text.as_str(), a.language.as_str()),
        ("The launch is on 20 November.", "hi-IN")
    );
    assert_eq!(a.citations.len(), 1);
    let c = &a.citations[0];
    assert_eq!(
        (
            c.segment_id.as_str(),
            c.meeting_title.as_str(),
            c.speaker.as_deref()
        ),
        ("s1", "Launch sync", Some("Asha"))
    );
    assert_eq!(
        c.quote, "launch 20 November ko hoga, yeh final hai",
        "the quote is the real line, not the model's words"
    );
    assert_eq!(c.starts_at, START, "meeting start plus the line's offset");
    let prompt = &llm.requests()[0].messages[0].content;
    assert!(
        prompt.contains("[S1] Launch sync — Asha:") && !prompt.contains("offsite"),
        "only matching lines are sent"
    );
}

#[tokio::test]
async fn an_answer_with_no_valid_source_is_thrown_away() {
    let (_d, llm, ai) = rig().await;
    llm.push_json(
        json!({"found": true, "answer": "It is on the 5th.", "sources": ["S9", "nonsense"]}),
    );
    let a = ai.recall("launch date", None).await.unwrap();
    assert_eq!((a.answer_text.as_str(), a.citations.len()), (NOT_FOUND, 0));
}

#[tokio::test]
async fn the_model_saying_not_found_is_respected() {
    let (_d, llm, ai) = rig().await;
    llm.push_json(json!({"found": false, "answer": "", "sources": []}));
    assert_eq!(
        ai.recall("launch date", None).await.unwrap().answer_text,
        NOT_FOUND
    );
}

#[tokio::test]
async fn nothing_matching_means_no_llm_call_at_all() {
    let (_d, llm, ai) = rig().await;
    let a = ai
        .recall("what about the quarterly salary revision", None)
        .await
        .unwrap();
    assert_eq!(a.answer_text, NOT_FOUND);
    assert!(llm.requests().is_empty());
}

#[tokio::test]
async fn blank_questions_are_refused_and_llm_failures_surface() {
    let (_d, llm, ai) = rig().await;
    assert_eq!(
        ai.recall("   ", None).await.unwrap_err().code,
        VoiceErrorCode::Core
    );
    llm.push_error(pluto_contracts::error::VoiceError::new(
        VoiceErrorCode::LlmUnavailable,
        "down",
    ));
    assert_eq!(
        ai.recall("launch", None).await.unwrap_err().code,
        VoiceErrorCode::LlmUnavailable
    );
}

#[tokio::test]
async fn unmapped_speakers_keep_their_number() {
    let (_d, llm, ai) = rig().await;
    llm.push_json(json!({"found": true, "answer": "Venue to book.", "sources": ["S1"]}));
    let a = ai.recall("offsite venue", None).await.unwrap();
    assert_eq!(a.citations[0].meeting_title, "Offsite");
    assert_eq!(
        a.citations[0].speaker.as_deref(),
        Some("Speaker 1"),
        "unmapped speakers keep their number"
    );
}
