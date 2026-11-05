//! T-205: the recap uses only applied items, includes due dates, respects the language and length,
//! and can be turned into audio through `TextToSpeech`.

use std::sync::Arc;

use chrono_tz::Tz;
use pluto_contracts::error::{VoiceError, VoiceErrorCode};
use pluto_core::bus::EventBus;
use pluto_core::clock::{Clock, FakeClock};
use pluto_core::db::Database;
use pluto_core::meeting_ai_store as store;
use pluto_core::meetings::{self, ActionItemInput, DecisionInput, MeetingInput};
use pluto_gnani::mock::MockTts;
use pluto_intel::llm::MockLlm;
use pluto_intel::meeting::recap::{self, MAX_WORDS};
use pluto_intel::meeting::service::MeetingAi;
use serde_json::json;

const START: i64 = 1_793_600_000_000; // 2 Nov 2026, 11:43 IST

async fn rig(with_items: bool) -> (tempfile::TempDir, Arc<MockLlm>, MeetingAi, String) {
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
            decisions: if with_items {
                vec![DecisionInput {
                    id: None,
                    body: "Launch on 20 November".into(),
                }]
            } else {
                vec![]
            },
            action_items: if with_items {
                vec![ActionItemInput {
                    id: Some("a1".into()),
                    body: "Finish the pricing copy".into(),
                    assignee: Some("Ravi".into()),
                }]
            } else {
                vec![]
            },
            ..Default::default()
        },
    )
    .await
    .unwrap();
    if with_items {
        db.write({
            let (id, action_id) = (m.id.clone(), m.action_items[0].id.clone());
            move |tx| {
                tx.execute("INSERT INTO transcript_segments (id, meeting_id, seg_index, start_ms, end_ms, text) VALUES ('s1', ?1, 0, 0, 1, 'x')", [&id])?;
                store::link_action_item(tx, &store::ActionMeta { action_item_id: action_id, source_segment_id: Some("s1".into()), quote: "q".into(), due_phrase: Some("Thursday".into()), due_date: Some("2026-11-05".into()), priority: None, confidence: None })?;
                Ok(((), vec![]))
            }
        }).await.unwrap();
    }
    let llm = Arc::new(MockLlm::new());
    let ai = MeetingAi::new(db, clock as Arc<dyn Clock>, llm.clone());
    (dir, llm, ai, m.id)
}

#[tokio::test]
async fn the_prompt_has_only_applied_items_with_owner_and_due_date_in_the_chosen_language() {
    let (_d, llm, ai, id) = rig(true).await;
    llm.push_json(json!({"recap": "Launch is on 20 November. Ravi will finish the pricing copy by 5 November."}));
    let text = ai.recap_text(&id, "hi-IN").await.unwrap();
    assert!(text.starts_with("Launch is on 20 November"));
    let p = &llm.requests()[0].messages[0].content;
    assert!(p.contains("Language: Hindi") && p.contains("Decision: Launch on 20 November"));
    assert!(
        p.contains("Action: Ravi — Finish the pricing copy (by 2026-11-05)"),
        "{p}"
    );
    assert!(p.contains("Launch sync on 2026-11-02"));
}

#[tokio::test]
async fn nothing_applied_means_no_recap_and_no_llm_call() {
    let (_d, llm, ai, id) = rig(false).await;
    let err = ai.recap_text(&id, "en-IN").await.unwrap_err();
    assert!(err.message.contains("Apply the review first"));
    assert!(llm.requests().is_empty());
}

#[tokio::test]
async fn over_long_or_empty_answers_are_handled() {
    let (_d, llm, ai, id) = rig(true).await;
    llm.push_json(json!({"recap": "We agreed to launch soon. ".repeat(60)}));
    let t = ai.recap_text(&id, "en-IN").await.unwrap();
    assert!(t.split_whitespace().count() <= MAX_WORDS && t.ends_with('.'));
    llm.push_json(json!({"recap": "  "}));
    assert_eq!(
        ai.recap_text(&id, "en-IN").await.unwrap_err().code,
        VoiceErrorCode::LlmInvalidOutput
    );
    llm.push_error(VoiceError::new(VoiceErrorCode::LlmUnavailable, "down"));
    assert_eq!(
        ai.recap_text(&id, "en-IN").await.unwrap_err().code,
        VoiceErrorCode::LlmUnavailable
    );
}

#[tokio::test]
async fn the_recap_can_be_made_into_audio() {
    let bytes = recap::to_mp3(&MockTts::new(), "Launch is on 20 November.", "en-IN")
        .await
        .unwrap();
    assert!(!bytes.is_empty());
    let failing = MockTts::failing(VoiceError::new(
        VoiceErrorCode::InsufficientCredit,
        "no credit",
    ));
    assert_eq!(
        recap::to_mp3(&failing, "x", "en-IN")
            .await
            .unwrap_err()
            .code,
        VoiceErrorCode::InsufficientCredit
    );
}
