//! T-FH-07 / T-FH-08 and the client behaviour behind them, against local fake providers.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use pluto_contracts::error::VoiceErrorCode;
use pluto_contracts::llm::{Llm, LlmMessage, LlmProvider, LlmRequest, LlmRole, LlmTask};
use pluto_contracts::voice::Destination;
use pluto_core::secrets::{Account, MemorySecrets, SecretStore};
use pluto_gnani::http::{OutboundLog, OutboundRecord};
use pluto_intel::llm::gemini::GeminiClient;
use pluto_intel::llm::groq::GroqClient;
use pluto_intel::llm::router::Router;
use serde_json::{json, Value};
use wiremock::matchers::{header, method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[derive(Default)]
struct Log(Mutex<Vec<OutboundRecord>>);
impl OutboundLog for Log {
    fn record(&self, r: OutboundRecord) {
        self.0.lock().unwrap().push(r);
    }
}

const GEMINI_KEY: &str = "AIza-test-key";
const GROQ_KEY: &str = "gsk_test_key";

struct Rig {
    gemini: MockServer,
    groq: MockServer,
    router: Router,
    log: Arc<Log>,
}

async fn rig(keys: &[(Account, &str)]) -> Rig {
    let (gemini, groq) = (MockServer::start().await, MockServer::start().await);
    let secrets = Arc::new(MemorySecrets::new());
    for (a, k) in keys {
        secrets.set(*a, k).unwrap();
    }
    let log = Arc::new(Log::default());
    let g =
        GeminiClient::new("gemini-test", secrets.clone(), log.clone()).with_base_url(gemini.uri());
    let q = GroqClient::new("groq-test", secrets, log.clone()).with_base_url(groq.uri());
    Rig {
        gemini,
        groq,
        router: Router::new(Arc::new(g), Arc::new(q)),
        log,
    }
}

fn both() -> Vec<(Account, &'static str)> {
    vec![(Account::Gemini, GEMINI_KEY), (Account::Groq, GROQ_KEY)]
}

fn req(task: LlmTask) -> LlmRequest {
    LlmRequest {
        task,
        system: "Extract items.".into(),
        messages: vec![LlmMessage {
            role: LlmRole::User,
            content: "secret meeting words".into(),
        }],
        json_schema: json!({"type":"object","required":["items"],"properties":{"items":{"type":"array"}}}),
        timeout_ms: 2_000,
    }
}

fn gemini_ok(text: &str) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_json(json!({"candidates":[{"content":{"parts":[{"text":text}]}}]}))
}
fn groq_ok(text: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({"choices":[{"message":{"content":text}}]}))
}

async fn on_gemini(r: &Rig, t: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path_regex("generateContent$"))
        .respond_with(t)
        .mount(&r.gemini)
        .await;
}
async fn on_groq(r: &Rig, t: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path_regex("chat/completions$"))
        .respond_with(t)
        .mount(&r.groq)
        .await;
}

#[tokio::test]
async fn each_task_goes_to_its_primary_without_falling_back() {
    let r = rig(&both()).await;
    on_gemini(&r, gemini_ok(r#"{"items":[1]}"#)).await;
    on_groq(&r, groq_ok(r#"{"items":[2]}"#)).await;
    let g = r.router.complete_json(req(LlmTask::Extract)).await.unwrap();
    assert_eq!(
        (g.provider, g.fell_back, g.model.as_str()),
        (LlmProvider::Gemini, false, "gemini-test")
    );
    let q = r.router.complete_json(req(LlmTask::Intent)).await.unwrap();
    assert_eq!((q.provider, q.fell_back), (LlmProvider::Groq, false));
}

#[tokio::test]
async fn a_groq_429_is_answered_by_gemini_with_fell_back_set() {
    // T-FH-07
    let r = rig(&both()).await;
    on_groq(&r, ResponseTemplate::new(429)).await;
    on_gemini(&r, gemini_ok(r#"{"items":[]}"#)).await;
    let out = r.router.complete_json(req(LlmTask::Intent)).await.unwrap();
    assert_eq!(out.provider, LlmProvider::Gemini);
    assert!(out.fell_back);
}

#[tokio::test]
async fn both_failing_gives_llm_unavailable_and_no_prompt_text() {
    // T-FH-08
    let r = rig(&both()).await;
    on_gemini(&r, ResponseTemplate::new(503)).await;
    on_groq(&r, ResponseTemplate::new(429)).await;
    let err = r
        .router
        .complete_json(req(LlmTask::Extract))
        .await
        .unwrap_err();
    assert_eq!(err.code, VoiceErrorCode::LlmUnavailable);
    assert!(err.retryable);
    assert!(!err.message.contains("secret meeting words"));
}

#[tokio::test]
async fn invalid_json_is_repaired_once_then_the_fallback_is_used() {
    let r = rig(&both()).await;
    // Gemini: bad twice (first try and the repair) -> the router falls back to Groq.
    on_gemini(&r, gemini_ok("sorry, here you go")).await;
    on_groq(&r, groq_ok(r#"{"items":[]}"#)).await;
    let out = r.router.complete_json(req(LlmTask::Extract)).await.unwrap();
    assert_eq!((out.provider, out.fell_back), (LlmProvider::Groq, true));
    assert_eq!(
        r.gemini.received_requests().await.unwrap().len(),
        2,
        "one try plus one repair"
    );

    // A repair that works stays on the same provider.
    let r2 = rig(&both()).await;
    Mock::given(method("POST"))
        .and(path_regex("generateContent$"))
        .respond_with(gemini_ok("nope"))
        .up_to_n_times(1)
        .mount(&r2.gemini)
        .await;
    on_gemini(&r2, gemini_ok(r#"{"items":[]}"#)).await;
    let ok = r2
        .router
        .complete_json(req(LlmTask::Extract))
        .await
        .unwrap();
    assert_eq!((ok.provider, ok.fell_back), (LlmProvider::Gemini, false));
    let second: Value =
        serde_json::from_slice(&r2.gemini.received_requests().await.unwrap()[1].body).unwrap();
    assert_eq!(
        second["contents"].as_array().unwrap().len(),
        3,
        "the repair shows the bad answer"
    );
}

#[tokio::test]
async fn a_missing_key_falls_back_and_two_missing_keys_say_so() {
    let r = rig(&[(Account::Gemini, GEMINI_KEY)]).await;
    on_gemini(&r, gemini_ok(r#"{"items":[]}"#)).await;
    let out = r.router.complete_json(req(LlmTask::Intent)).await.unwrap();
    assert_eq!((out.provider, out.fell_back), (LlmProvider::Gemini, true));
    assert!(
        r.groq.received_requests().await.unwrap().is_empty(),
        "no key, no call"
    );

    let none = rig(&[]).await;
    let err = none
        .router
        .complete_json(req(LlmTask::Extract))
        .await
        .unwrap_err();
    assert_eq!(err.code, VoiceErrorCode::MissingApiKey);
}

#[tokio::test]
async fn a_slow_primary_times_out_and_the_fallback_answers() {
    let r = rig(&both()).await;
    on_groq(
        &r,
        groq_ok(r#"{"items":[]}"#).set_delay(Duration::from_millis(600)),
    )
    .await;
    on_gemini(&r, gemini_ok(r#"{"items":[]}"#)).await;
    let mut q = req(LlmTask::Intent);
    q.timeout_ms = 150;
    let out = r.router.complete_json(q).await.unwrap();
    assert_eq!((out.provider, out.fell_back), (LlmProvider::Gemini, true));
}

#[tokio::test]
async fn keys_travel_only_in_headers_and_every_call_is_logged_without_content() {
    let r = rig(&both()).await;
    Mock::given(method("POST"))
        .and(header("x-goog-api-key", GEMINI_KEY))
        .respond_with(gemini_ok(r#"{"items":[]}"#))
        .mount(&r.gemini)
        .await;
    Mock::given(method("POST"))
        .and(header(
            "authorization",
            format!("Bearer {GROQ_KEY}").as_str(),
        ))
        .respond_with(groq_ok(r#"{"items":[]}"#))
        .mount(&r.groq)
        .await;
    r.router.complete_json(req(LlmTask::Extract)).await.unwrap();
    r.router.complete_json(req(LlmTask::Intent)).await.unwrap();
    for srv in [&r.gemini, &r.groq] {
        for rec in srv.received_requests().await.unwrap() {
            let body = String::from_utf8_lossy(&rec.body);
            assert!(
                !body.contains(GEMINI_KEY) && !body.contains(GROQ_KEY),
                "keys never go in a body"
            );
            assert!(!rec.url.to_string().contains("key"), "or in a URL");
        }
    }
    let log = r.log.0.lock().unwrap();
    assert_eq!(log.len(), 2);
    assert_eq!(log[0].destination, Destination::Gemini);
    assert_eq!(log[1].destination, Destination::Groq);
    assert!(log
        .iter()
        .all(|l| l.bytes > 0 && !l.purpose.contains("secret")));
}

#[tokio::test]
async fn gemini_receives_the_converted_schema_and_the_system_prompt() {
    let r = rig(&both()).await;
    on_gemini(&r, gemini_ok(r#"{"items":[]}"#)).await;
    r.router.complete_json(req(LlmTask::Recap)).await.unwrap();
    let body: Value =
        serde_json::from_slice(&r.gemini.received_requests().await.unwrap()[0].body).unwrap();
    assert_eq!(body["generationConfig"]["responseSchema"]["type"], "OBJECT");
    assert_eq!(
        body["generationConfig"]["responseMimeType"],
        "application/json"
    );
    assert_eq!(
        body["systemInstruction"]["parts"][0]["text"],
        "Extract items."
    );
}
