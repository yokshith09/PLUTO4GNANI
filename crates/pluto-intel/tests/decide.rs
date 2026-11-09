//! T-302 / T-ML-04 groundwork: rules never misroute AS-4, and the LLM path resolves dates and times
//! the way the ground truth expects. The LLM itself is scripted, so this tests Rust, not the model.

use std::path::Path;
use std::sync::Arc;

use chrono::DateTime;
use pluto_contracts::decide::{DecideContext, Decider, DecisionSource, Intent};
use pluto_contracts::error::{VoiceError, VoiceErrorCode};
use pluto_intel::decide::{rules, DefaultDecider};
use pluto_intel::llm::MockLlm;
use serde_json::{json, Value};

fn as4() -> Value {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/ground-truth/as4.json");
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn ctx(doc: &Value) -> DecideContext {
    DecideContext {
        recent_turns: vec![],
        open_task_titles: vec!["Budget report".into()],
        participants: vec!["Ravi".into(), "Meena".into()],
        timezone: "Asia/Kolkata".into(),
        now_ms: DateTime::parse_from_rfc3339(doc["reference_now"].as_str().unwrap())
            .unwrap()
            .timestamp_millis(),
    }
}

fn intent_of(name: &str) -> Intent {
    match name {
        "meetingquery" => Intent::MeetingQuery,
        "task" => Intent::Task,
        "reminder" => Intent::Reminder,
        "note" => Intent::Note,
        "focus" => Intent::Focus,
        "project" => Intent::Project,
        "navigate" => Intent::Navigate,
        "approval" => Intent::Approval,
        "reject" => Intent::Reject,
        "cancel" => Intent::Cancel,
        "noise" => Intent::Noise,
        other => panic!("unknown intent {other}"),
    }
}

/// What a correct model would send for this command: phrases only, never computed dates.
fn model_reply(cmd: &Value) -> Value {
    let (intent, tool, args) = (cmd["intent"].as_str().unwrap(), &cmd["tool"], &cmd["args"]);
    let mut raw = args.clone();
    if let Some(o) = raw.as_object_mut() {
        o.remove("due_date");
        if tool == "reminder_create" {
            let at = o
                .remove("remind_at")
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap();
            if o["when_phrase"].as_str().unwrap().starts_with("in ") {
                o.insert("in_minutes".into(), json!(120));
            } else {
                o.insert("time".into(), json!(&at[11..16]));
            }
        }
    }
    let intent = if intent == "meetingquery" {
        "meeting_query"
    } else {
        intent
    };
    json!({"intent": intent, "tool": tool, "args": raw, "confidence": 0.9})
}

#[test]
fn rules_never_misroute_an_as4_command() {
    let doc = as4();
    let mut hits = 0;
    for c in doc["commands"].as_array().unwrap() {
        let Some(hit) = rules::apply(c["text"].as_str().unwrap()) else {
            continue;
        };
        hits += 1;
        assert_eq!(
            hit.intent,
            intent_of(c["intent"].as_str().unwrap()),
            "{}",
            c["text"]
        );
        assert_eq!(
            hit.tool.map(Value::from).unwrap_or(Value::Null),
            c["tool"],
            "{}",
            c["text"]
        );
        if let Some(m) = c["args"]["minutes"].as_u64() {
            assert_eq!(hit.args["minutes"].as_u64(), Some(m), "{}", c["text"]);
        }
    }
    assert!(
        hits >= 10,
        "the rules should cover the answers and focus commands (got {hits})"
    );
}

#[tokio::test]
async fn every_other_command_resolves_to_the_expected_tool_and_arguments() {
    let doc = as4();
    let ctx = ctx(&doc);
    for c in doc["commands"].as_array().unwrap() {
        let text = c["text"].as_str().unwrap();
        if rules::apply(text).is_some() {
            continue;
        }
        let llm = MockLlm::new();
        llm.push_json(model_reply(c));
        let d = DefaultDecider::new(Arc::new(llm))
            .decide(text, &ctx)
            .await
            .unwrap();
        assert_eq!(d.source, DecisionSource::Llm);
        assert_eq!(d.intent, intent_of(c["intent"].as_str().unwrap()), "{text}");
        assert_eq!(d.tool.as_deref(), c["tool"].as_str(), "{text}");
        for (k, want) in c["args"].as_object().unwrap() {
            if k == "remind_at" {
                let want_ms = DateTime::parse_from_rfc3339(want.as_str().unwrap())
                    .unwrap()
                    .timestamp_millis();
                assert_eq!(d.args[k].as_i64(), Some(want_ms), "{text}: remind_at");
            } else {
                assert_eq!(&d.args[k], want, "{text}: {k}");
            }
        }
    }
}

async fn decide_with(reply: Value, text: &str) -> pluto_contracts::decide::AssistantDecision {
    let llm = MockLlm::new();
    llm.push_json(reply);
    DefaultDecider::new(Arc::new(llm))
        .decide(text, &ctx(&as4()))
        .await
        .unwrap()
}

#[tokio::test]
async fn a_made_up_tool_or_incomplete_arguments_do_nothing() {
    let d = decide_with(
        json!({"intent":"task","tool":"delete_everything","args":{}}),
        "wipe it",
    )
    .await;
    assert_eq!((d.intent, d.tool), (Intent::Noise, None));
    let d = decide_with(
        json!({"intent":"task","tool":"task_create","args":{}}),
        "add a task",
    )
    .await;
    assert_eq!((d.intent, d.tool), (Intent::Noise, None), "no title");
    let d = decide_with(
        json!({"intent":"focus","tool":"focus_start","args":{"minutes":9999}}),
        "focus forever",
    )
    .await;
    assert_eq!(d.tool, None, "absurd length");
    let d = decide_with(
        json!({"intent":"noise","tool":null,"args":{}}),
        "lovely weather",
    )
    .await;
    assert_eq!(d.intent, Intent::Noise);
}

#[tokio::test]
async fn open_page_names_a_real_page_or_does_nothing() {
    for (said, page) in [
        ("Meetings", "meetings"),
        ("settings page", "settings"),
        ("home", "today"),
        ("reminders", "tasks"),
        ("trash", "bin"),
    ] {
        let d = decide_with(
            json!({"intent":"navigate","tool":"open_page","args":{"page":said}}),
            "open it",
        )
        .await;
        assert_eq!(d.intent, Intent::Navigate, "{said}");
        assert_eq!(d.tool.as_deref(), Some("open_page"), "{said}");
        assert_eq!(d.args, json!({"page": page}), "{said}");
    }
    let d = decide_with(
        json!({"intent":"navigate","tool":"open_page","args":{"page":"kitchen"}}),
        "open the kitchen",
    )
    .await;
    assert_eq!((d.intent, d.tool), (Intent::Noise, None), "not a page");
}

#[tokio::test]
async fn a_reminder_with_no_time_asks_instead_of_guessing() {
    let d = decide_with(json!({"intent":"reminder","tool":"reminder_create","args":{"title":"Dentist","when_phrase":"Friday"}}), "remind me friday dentist").await;
    assert_eq!(d.tool.as_deref(), Some("reminder_create"));
    assert!(d.args["remind_at"].is_null(), "needs a time");
}

#[tokio::test]
async fn a_time_that_already_passed_today_means_tomorrow() {
    // 09:00 now; 08:00 with no day -> tomorrow 08:00 IST.
    let d = decide_with(
        json!({"intent":"reminder","tool":"reminder_create","args":{"title":"Gym","time":"08:00"}}),
        "remind me at 8",
    )
    .await;
    let want = DateTime::parse_from_rfc3339("2026-11-05T08:00:00+05:30")
        .unwrap()
        .timestamp_millis();
    assert_eq!(d.args["remind_at"].as_i64(), Some(want));
    // 17:00 is still ahead -> today.
    let d = decide_with(json!({"intent":"reminder","tool":"reminder_create","args":{"title":"Call","time":"17:00"}}), "remind me at 5 pm").await;
    let want = DateTime::parse_from_rfc3339("2026-11-04T17:00:00+05:30")
        .unwrap()
        .timestamp_millis();
    assert_eq!(d.args["remind_at"].as_i64(), Some(want));
}

#[tokio::test]
async fn an_unknown_intent_is_an_invalid_output_and_llm_errors_pass_through() {
    let llm = MockLlm::new();
    llm.push_json(json!({"intent":"dance","tool":null,"args":{}}));
    let err = DefaultDecider::new(Arc::new(llm))
        .decide("something", &ctx(&as4()))
        .await
        .unwrap_err();
    assert_eq!(err.code, VoiceErrorCode::LlmInvalidOutput);

    let llm = MockLlm::new();
    llm.push_error(VoiceError::new(VoiceErrorCode::LlmUnavailable, "down"));
    let err = DefaultDecider::new(Arc::new(llm))
        .decide("something", &ctx(&as4()))
        .await
        .unwrap_err();
    assert_eq!(err.code, VoiceErrorCode::LlmUnavailable);
}

#[tokio::test]
async fn rules_answer_without_calling_the_llm() {
    let llm = Arc::new(MockLlm::new());
    let d = DefaultDecider::new(llm.clone())
        .decide("haan kar do", &ctx(&as4()))
        .await
        .unwrap();
    assert_eq!(
        (d.intent, d.source),
        (Intent::Approval, DecisionSource::Rules)
    );
    assert!(llm.requests().is_empty());
}

#[tokio::test]
async fn the_prompt_carries_now_open_tasks_and_the_command_but_no_keys() {
    let llm = Arc::new(MockLlm::new());
    llm.push_json(json!({"intent":"noise","tool":null,"args":{}}));
    DefaultDecider::new(llm.clone())
        .decide("kuch bhi", &ctx(&as4()))
        .await
        .unwrap();
    let r = &llm.requests()[0];
    let p = &r.messages[0].content;
    assert!(
        p.contains("Wednesday 2026-11-04 09:00")
            && p.contains("Budget report")
            && p.contains("Command: kuch bhi")
    );
    assert_eq!(r.timeout_ms, 4_000, "intent calls have the short timeout");
}

#[tokio::test]
async fn a_single_word_task_title_becomes_what_was_said() {
    // The model returned one word for a whole sentence: the task must still say what it is.
    let d = decide_with(
        json!({"intent":"task","tool":"task_create","args":{"title":"send"}}),
        "Pluto add a task to send the budget report to Ravi",
    )
    .await;
    assert_eq!(d.tool.as_deref(), Some("task_create"));
    assert_eq!(d.args["title"], "Send the budget report to Ravi");

    // A proper title from the model is left alone.
    let d = decide_with(
        json!({"intent":"task","tool":"task_create","args":{"title":"Call the dentist","due_phrase":"tomorrow"}}),
        "Pluto add a task to call the dentist tomorrow",
    )
    .await;
    assert_eq!(d.args["title"], "Call the dentist");

    // The same for a reminder's title.
    let d = decide_with(
        json!({"intent":"reminder","tool":"reminder_create","args":{"title":"call","when_phrase":"Friday"}}),
        "Pluto remind me to call the dentist on Friday",
    )
    .await;
    assert_eq!(d.args["title"], "Call the dentist on Friday");
}
