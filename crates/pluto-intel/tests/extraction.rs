//! T-203: extraction validation, dates, duplicates and batching, with a scripted LLM.

use std::path::Path;

use chrono::NaiveDate;
use pluto_contracts::error::{VoiceError, VoiceErrorCode};
use pluto_contracts::meeting::{QualityWarning, Relevance, TranscriptSegment};
use pluto_intel::llm::MockLlm;
use pluto_intel::meeting::extraction::{extract, ExtractInput, OpenTask, BATCH};
use serde_json::{json, Value};

fn segments() -> Vec<TranscriptSegment> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-data/fixtures/meetings/transcript_segments.as1.json");
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    serde_json::from_value(doc["data"].clone()).unwrap()
}

fn input() -> ExtractInput {
    ExtractInput {
        meeting_id: "m-as1".into(),
        meeting_date: NaiveDate::from_ymd_opt(2026, 11, 2).unwrap(), // Monday
        participants: vec!["Asha".into(), "Ravi".into(), "Meena".into()],
        segments: segments(),
        open_tasks: vec![],
    }
}

fn item(kind: &str, body: &str, owner: Value, due: Value, seg: &str, quote: &str) -> Value {
    json!({"kind": kind, "body": body, "owner": owner, "due_phrase": due, "segment_id": seg, "quote": quote})
}

fn llm_with(items: Vec<Value>) -> MockLlm {
    let m = MockLlm::new();
    m.push_json(json!({ "items": items }));
    m
}

#[tokio::test]
async fn good_items_are_kept_with_owner_date_and_quote() {
    let llm = llm_with(vec![
        item(
            "decision",
            "Start with the annual plan.",
            Value::Null,
            Value::Null,
            "seg-2",
            "hum annual plan se start karenge",
        ),
        item(
            "action_item",
            "Finish the pricing copy.",
            json!("ravi"),
            json!("Thursday"),
            "seg-1",
            "Main Thursday tak copy complete kar dunga",
        ),
        item(
            "action_item",
            "Confirm the contract.",
            json!("Meena"),
            json!("next Monday"),
            "seg-4",
            "Monday tak confirm karo",
        ),
    ]);
    let out = extract(&llm, &input()).await.unwrap();
    assert_eq!(out.items.len(), 3);
    assert_eq!(out.dominant_language.as_deref(), Some("hi-IN"));
    let copy = out
        .items
        .iter()
        .find(|i| i.body.starts_with("Finish"))
        .unwrap();
    assert_eq!(
        copy.owner.as_deref(),
        Some("Ravi"),
        "matched to the participant, case-insensitively"
    );
    assert_eq!(copy.due_date.as_deref(), Some("2026-11-05"));
    assert_eq!(copy.due_phrase.as_deref(), Some("Thursday"));
    assert_eq!(copy.source_segment_id, "seg-1");
    let contract = out
        .items
        .iter()
        .find(|i| i.body.starts_with("Confirm"))
        .unwrap();
    assert_eq!(contract.due_date.as_deref(), Some("2026-11-09"));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
}

#[tokio::test]
async fn items_without_a_real_segment_or_kind_are_dropped() {
    let llm = llm_with(vec![
        item(
            "decision",
            "Ghost",
            Value::Null,
            Value::Null,
            "seg-99",
            "anything",
        ),
        item(
            "chitchat",
            "Chai",
            Value::Null,
            Value::Null,
            "seg-6",
            "Chai break?",
        ),
        item(
            "risk",
            "Legal may be late.",
            Value::Null,
            Value::Null,
            "seg-5",
            "legal late hua",
        ),
        json!({"kind": "risk", "body": "", "segment_id": "seg-5", "quote": "x"}),
        json!({"kind": "risk", "segment_id": "seg-5"}),
    ]);
    let out = extract(&llm, &input()).await.unwrap();
    assert_eq!(out.items.len(), 1);
    assert_eq!(out.items[0].kind, Relevance::Risk);
}

#[tokio::test]
async fn a_paraphrased_quote_is_replaced_by_the_real_segment_text() {
    let llm = llm_with(vec![item(
        "decision",
        "Annual plan first.",
        Value::Null,
        Value::Null,
        "seg-2",
        "we will begin with yearly billing",
    )]);
    let out = extract(&llm, &input()).await.unwrap();
    assert!(out.items[0].quote.contains("annual plan se start karenge"));
}

#[tokio::test]
async fn warnings_cover_missing_owner_unresolved_date_and_no_decisions() {
    let llm = llm_with(vec![item(
        "action_item",
        "Do the thing.",
        json!("Stranger"),
        json!("whenever"),
        "seg-4",
        "legal se baat karke",
    )]);
    let out = extract(&llm, &input()).await.unwrap();
    let a = &out.items[0];
    assert_eq!((a.owner.as_deref(), a.due_date.as_deref()), (None, None));
    assert!(out.warnings.contains(&QualityWarning::NoDecisions));
    assert!(out
        .warnings
        .contains(&QualityWarning::ActionItemWithoutOwner {
            item_id: a.id.clone()
        }));
    assert!(out.warnings.contains(&QualityWarning::UnresolvedDueDate {
        item_id: a.id.clone()
    }));
}

#[tokio::test]
async fn an_item_that_repeats_an_open_task_is_flagged_not_dropped() {
    let mut inp = input();
    inp.open_tasks = vec![
        OpenTask {
            id: "t-7".into(),
            title: "Finish pricing page copy".into(),
        },
        OpenTask {
            id: "t-8".into(),
            title: "Book flights".into(),
        },
    ];
    let llm = llm_with(vec![item(
        "action_item",
        "Finish the pricing copy.",
        json!("Ravi"),
        json!("Thursday"),
        "seg-1",
        "copy complete kar dunga",
    )]);
    let out = extract(&llm, &inp).await.unwrap();
    assert_eq!(out.items[0].duplicate_of_task_id.as_deref(), Some("t-7"));
}

#[tokio::test]
async fn repeated_items_are_kept_once() {
    let same = item(
        "decision",
        "Annual plan first.",
        Value::Null,
        Value::Null,
        "seg-2",
        "annual plan se start",
    );
    let out = extract(&llm_with(vec![same.clone(), same]), &input())
        .await
        .unwrap();
    assert_eq!(out.items.len(), 1);
}

#[tokio::test]
async fn long_transcripts_go_in_batches_of_forty() {
    let mut inp = input();
    let base = inp.segments[0].clone();
    inp.segments = (0..85)
        .map(|i| TranscriptSegment {
            id: format!("s{i}"),
            index: i,
            ..base.clone()
        })
        .collect();
    let llm = MockLlm::new();
    for _ in 0..3 {
        llm.push_json(json!({"items": []}));
    }
    extract(&llm, &inp).await.unwrap();
    let reqs = llm.requests();
    assert_eq!(reqs.len(), 3);
    let lines: Vec<usize> = reqs
        .iter()
        .map(|r| {
            r.messages[0]
                .content
                .lines()
                .filter(|l| l.starts_with("[s"))
                .count()
        })
        .collect();
    assert_eq!(lines, [BATCH, BATCH, 5]);
    assert!(reqs[0].messages[0]
        .content
        .contains("Participants: Asha, Ravi, Meena"));
}

#[tokio::test]
async fn an_llm_failure_is_returned_and_nothing_is_invented() {
    let llm = MockLlm::new();
    llm.push_error(VoiceError::new(VoiceErrorCode::LlmUnavailable, "down"));
    let err = extract(&llm, &input()).await.unwrap_err();
    assert_eq!(err.code, VoiceErrorCode::LlmUnavailable);
}
