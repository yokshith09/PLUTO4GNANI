//! T-403 safety suite, network side (TESTING_STRATEGY §6):
//! - **T-PR-04** Cloud voice off → zero outbound requests, on every path that can send.
//! - **T-PR-02** Every request that leaves is written to the privacy log.
//! - **T-PR-01** The API key never reaches logs, error messages, SQLite or the data export.
//!
//! No real network: a wiremock server stands in for Gnani's REST host and a local listener for
//! the WebSocket hosts; both count what reaches them. T-AS-05 (no write without approval) is in
//! `src/assistant/tests.rs`, T-PR-03 (consent) in `pluto-intel/tests/recorder.rs`,
//! `pluto-core`'s `meeting_ai_store` and `src/features/meetings/RecordDialog.test.tsx`.

use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use pluto_contracts::error::{VoiceError, VoiceErrorCode, VoiceResult};
use pluto_contracts::llm::{Llm, LlmRequest, LlmResponse, LlmTask};
use pluto_contracts::speech::{AudioContainer, RtConfig, SpeechToText, TextToSpeech, TtsRequest};
use pluto_core::bus::EventBus;
use pluto_core::clock::{Clock, FakeClock};
use pluto_core::db::Database;
use pluto_core::{data, voice_store};
use pluto_gnani::batch::{BatchApi, BatchClient, BatchConfig, BatchFile};
use pluto_gnani::http::{
    Call, Family, GnaniHttp, OutboundLog, OutboundRecord, RetryPolicy, StaticKey,
};
use pluto_gnani::realtime::RealtimeClient;
use pluto_gnani::tts::TimbreClient;
use pluto_voice::privacy::{CloudGated, CloudSwitch};
use serde_json::json;
use tokio::net::TcpListener;
use wiremock::matchers::{any, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A key that must never show up anywhere except the `x-api-key-id` header sent to Gnani.
const KEY: &str = "gn-SAFETY-7f3a91c0d2e4";

#[derive(Default)]
struct MemoryLog(Mutex<Vec<OutboundRecord>>);

impl OutboundLog for MemoryLog {
    fn record(&self, record: OutboundRecord) {
        self.0.lock().unwrap().push(record);
    }
}

impl MemoryLog {
    fn all(&self) -> Vec<OutboundRecord> {
        self.0.lock().unwrap().clone()
    }
}

/// An LLM that counts the calls that reach it (Gemini / Groq stand-in).
#[derive(Default)]
struct CountingLlm(AtomicUsize);

#[async_trait]
impl Llm for CountingLlm {
    async fn complete_json(&self, _req: LlmRequest) -> VoiceResult<LlmResponse> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err(VoiceError::new(VoiceErrorCode::LlmUnavailable, "scripted"))
    }
}

fn llm_request() -> LlmRequest {
    LlmRequest {
        task: LlmTask::Intent,
        system: "route".into(),
        messages: vec![],
        json_schema: json!({}),
        timeout_ms: 1000,
    }
}

/// A WebSocket host stand-in: counts connections, then drops them.
async fn ws_host() -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let accepted = Arc::new(AtomicUsize::new(0));
    let count = accepted.clone();
    tokio::spawn(async move {
        while let Ok((sock, _)) = listener.accept().await {
            count.fetch_add(1, Ordering::SeqCst);
            drop(sock);
        }
    });
    (url, accepted)
}

fn fast() -> RetryPolicy {
    RetryPolicy {
        delays: vec![Duration::from_millis(10); 2],
        max_retry_after: Duration::from_secs(1),
    }
}

struct Clients {
    http: Arc<GnaniHttp>,
    batch: BatchClient,
    stt: RealtimeClient,
    tts: TimbreClient,
    llm: CloudGated,
    llm_calls: Arc<CountingLlm>,
}

fn clients(rest: &str, ws: &str, cloud: &Arc<CloudSwitch>, log: &Arc<MemoryLog>) -> Clients {
    let keys = Arc::new(StaticKey(Some(KEY.into())));
    let log: Arc<dyn OutboundLog> = log.clone();
    let http = Arc::new(
        GnaniHttp::new(rest, keys.clone(), log.clone())
            .unwrap()
            .with_policy(fast())
            .with_cloud(cloud.clone()),
    );
    let llm_calls = Arc::new(CountingLlm::default());
    Clients {
        batch: BatchClient::new(http.clone(), log.clone()).unwrap(),
        stt: RealtimeClient::new(ws, keys.clone(), log.clone()).with_cloud(cloud.clone()),
        tts: TimbreClient::new(ws, http.clone(), keys, log),
        llm: CloudGated::new(llm_calls.clone(), cloud.clone()),
        llm_calls,
        http,
    }
}

async fn rest_call(c: &Clients) -> VoiceResult<()> {
    let url = c.http.url("/stt/v3");
    c.http
        .execute(Call::new(Family::SttRest, "voice clip", 1024), |client| {
            client.post(&url).body(vec![0u8; 1024])
        })
        .await
        .map(|_| ())
}

fn tts_request() -> TtsRequest {
    TtsRequest {
        text: "Ho gaya.".into(),
        language: "hi-IN".into(),
        voice: None,
        model: None,
        speed: None,
    }
}

/// Every path that can send, each returning its result code.
async fn try_every_path(c: &Clients, rest: &str) -> Vec<(&'static str, VoiceErrorCode)> {
    let code = |r: VoiceResult<()>| r.err().map(|e| e.code).unwrap_or(VoiceErrorCode::Core);
    let file = BatchFile {
        name: "m.wav".into(),
        bytes: vec![0; 64],
    };
    let config = BatchConfig::for_meeting(&["Ravi".into()], &[], None);
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    vec![
        ("gnani rest", code(rest_call(c).await)),
        (
            "batch create",
            code(c.batch.create(&[file], &config, "m1").await.map(|_| ())),
        ),
        (
            "batch status",
            code(c.batch.status("job-1").await.map(|_| ())),
        ),
        (
            "batch files",
            code(c.batch.files("job-1").await.map(|_| ())),
        ),
        (
            "batch download",
            code(
                c.batch
                    .transcript(&format!("{rest}/files/t.json"), "m1")
                    .await
                    .map(|_| ()),
            ),
        ),
        (
            "realtime",
            code(c.stt.open(RtConfig::default()).await.map(|_| ())),
        ),
        ("timbre stream", code(c.tts.stream(tts_request(), tx).await)),
        (
            "timbre file",
            code(
                c.tts
                    .synthesize(tts_request(), AudioContainer::Wav)
                    .await
                    .map(|_| ()),
            ),
        ),
        (
            "llm",
            code(c.llm.complete_json(llm_request()).await.map(|_| ())),
        ),
    ]
}

#[tokio::test]
async fn t_pr_04_cloud_voice_off_sends_nothing_on_any_path() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;
    let (ws, ws_connections) = ws_host().await;
    let cloud = CloudSwitch::new(false);
    let log = Arc::new(MemoryLog::default());
    let c = clients(&server.uri(), &ws, &cloud, &log);

    for (path, code) in try_every_path(&c, &server.uri()).await {
        assert_eq!(code, VoiceErrorCode::CloudVoiceDisabled, "{path}");
    }
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "nothing reached Gnani's REST host"
    );
    assert_eq!(
        ws_connections.load(Ordering::SeqCst),
        0,
        "nothing reached a WebSocket host"
    );
    assert_eq!(c.llm_calls.0.load(Ordering::SeqCst), 0, "no LLM call");
    assert!(log.all().is_empty(), "nothing to log because nothing left");

    // Control: the same clients do send once Cloud voice is on, so the switch is what stopped them.
    cloud.set(true);
    rest_call(&c).await.unwrap();
    let _ = c.llm.complete_json(llm_request()).await;
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    assert_eq!(c.llm_calls.0.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn t_pr_04_turning_cloud_voice_off_stops_calls_already_waiting_to_retry() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let (ws, _) = ws_host().await;
    let cloud = CloudSwitch::new(true);
    let log = Arc::new(MemoryLog::default());
    let c = clients(&server.uri(), &ws, &cloud, &log);
    let http = c.http.clone();
    // The first attempt fails with 503; switch off before the retry is due.
    let call = tokio::spawn(async move {
        let url = http.url("/stt/v3");
        http.execute(Call::new(Family::SttRest, "voice clip", 1), |client| {
            client.post(&url)
        })
        .await
        .map(|_| ())
    });
    tokio::time::sleep(Duration::from_millis(3)).await;
    cloud.set(false);
    let err = call.await.unwrap().unwrap_err();
    assert!(
        matches!(
            err.code,
            VoiceErrorCode::CloudVoiceDisabled | VoiceErrorCode::GnaniUpstream
        ),
        "{err:?}"
    );
    let sent = server.received_requests().await.unwrap().len();
    assert!(
        sent <= 1,
        "no retry went out after Cloud voice was turned off ({sent} sent)"
    );
}

#[tokio::test]
async fn t_pr_02_every_request_that_leaves_is_logged() {
    let server = MockServer::start().await;
    // Success, a retried failure, and an error: each attempt that leaves must have a log line.
    Mock::given(method("POST"))
        .and(path("/stt/v3"))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/stt/v3"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;
    Mock::given(path("/files/t.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"segments": []})))
        .mount(&server)
        .await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({"error": "BAD"})))
        .mount(&server)
        .await;
    let (ws, _) = ws_host().await;
    let cloud = CloudSwitch::new(true);
    let log = Arc::new(MemoryLog::default());
    let c = clients(&server.uri(), &ws, &cloud, &log);

    rest_call(&c).await.unwrap(); // 500, then 200: two requests
    let _ = c.batch.status("job-1").await; // 400
    let _ = c.batch.files("job-1").await; // 400
    let _ = c
        .batch
        .transcript(&format!("{}/files/t.json", server.uri()), "m1")
        .await;

    let sent = server.received_requests().await.unwrap().len();
    let logged = log.all();
    assert!(sent >= 5, "the scenario sent {sent} requests");
    assert_eq!(logged.len(), sent, "one log line per request that left");
    for r in &logged {
        assert!(!r.purpose.is_empty(), "every line says why: {r:?}");
    }
}

/// Collects everything written through `tracing`, at every level.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn t_pr_01_the_key_never_reaches_logs_errors_sqlite_or_the_export() {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || writer.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/stt/v3"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({"error": "UNAUTHORIZED"})))
        .mount(&server)
        .await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let (ws, _) = ws_host().await;
    let cloud = CloudSwitch::new(true);
    let log = Arc::new(MemoryLog::default());
    let c = clients(&server.uri(), &ws, &cloud, &log);

    // Failures are where a key leaks: errors, retries, dropped connections.
    let mut errors = Vec::new();
    errors.push(rest_call(&c).await.unwrap_err());
    errors.push(c.batch.status("job-1").await.unwrap_err());
    errors.push(c.stt.open(RtConfig::default()).await.unwrap_err());
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    errors.push(c.tts.stream(tts_request(), tx).await.unwrap_err());

    // The key did go to Gnani, in its header, so the check below isn't vacuous.
    let first = &server.received_requests().await.unwrap()[0];
    assert_eq!(first.headers["x-api-key-id"], KEY);

    let logs = String::from_utf8_lossy(&captured.0.lock().unwrap()).to_string();
    assert!(!logs.contains(KEY), "tracing output");
    for e in &errors {
        assert!(
            !e.message.contains(KEY) && !format!("{e:?}").contains(KEY),
            "{e:?}"
        );
    }
    assert!(
        !format!("{:?}", StaticKey(Some(KEY.into()))).contains(KEY),
        "Debug"
    );
    for r in log.all() {
        assert!(!format!("{r:?}").contains(KEY), "privacy log line {r:?}");
    }

    // SQLite: the privacy log rows as the app writes them, then the whole file and the export.
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("pluto.db");
    let db = Database::open(&file, EventBus::default()).unwrap();
    let clock = FakeClock::new(1_762_329_600_000, chrono_tz::Asia::Kolkata);
    let rows = log.all();
    assert!(!rows.is_empty());
    let at = clock.now_ms();
    db.write(move |tx| {
        for r in &rows {
            let destination = serde_json::to_value(r.destination).unwrap();
            voice_store::log_outbound(
                tx,
                at,
                destination.as_str().unwrap(),
                &r.purpose,
                i64::try_from(r.bytes).unwrap(),
                r.ref_id.as_deref(),
            )?;
        }
        Ok(((), vec![]))
    })
    .await
    .unwrap();
    let export = data::export(&db, &clock, "test").await.unwrap().to_string();
    assert!(
        export.contains("privacy_log"),
        "the export includes the log"
    );
    assert!(!export.contains(KEY), "data export");
    drop(db);
    for entry in std::fs::read_dir(dir.path()).unwrap() {
        let bytes = std::fs::read(entry.unwrap().path()).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains(KEY), "database files");
    }
}
