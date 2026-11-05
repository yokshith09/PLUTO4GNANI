//! The Gnani HTTP base (ARCHITECTURE §6 "Concurrency", API_CONTRACTS §6). Owner: **D1** (T-103).
//!
//! Every Gnani REST call (Prisma REST, Batch, Timbre REST, voice cloning) goes through
//! [`GnaniHttp::execute`], and nowhere else does retries or rate limiting (WORKING_AGREEMENT §6):
//!
//! - **One request in flight per family.** Gnani answers 429 "Only 1 active request is allowed at a
//!   time", so each [`Family`] has its own single-permit queue. A caller waits for its turn; the
//!   permit is held across that call's retries.
//! - **Retries** on connection errors, timeouts and 429 / 500 / 503 / 504, after 0.5, 1 and 2 s
//!   (up to 4 attempts), honouring a `Retry-After` in seconds. Other 4xx are never retried
//!   (NF-03).
//! - **Errors** become `VoiceError` codes (API_CONTRACTS §6). A 402 sets the credit flag: every
//!   later call fails fast with `insufficient_credit` until [`GnaniHttp::reset_credit`].
//! - **Privacy log.** Every attempt that leaves the device is recorded through [`OutboundLog`]
//!   (destination, purpose, bytes; never content), before it is sent (T-PR-02).
//!
//! Keys come from an [`ApiKeySource`] (the OS keychain in the app, `.env` in examples). They are
//! sent only in the `X-API-Key-ID` header and never logged.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use pluto_contracts::error::{VoiceError, VoiceErrorCode, VoiceResult};
use pluto_contracts::voice::Destination;
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use serde::de::DeserializeOwned;
use tokio::sync::Semaphore;

/// Gnani's API host.
pub const DEFAULT_BASE_URL: &str = "https://api.vachana.ai";
/// The auth header (API_CONTRACTS §1).
pub const API_KEY_HEADER: &str = "X-API-Key-ID";
/// Whole-request timeout. Batch uploads are up to 10 MB.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

// ---- injection points ----------------------------------------------------------------------

/// Where the Gnani key comes from. Implemented over the keychain in the app (T-105).
pub trait ApiKeySource: Send + Sync {
    fn gnani_key(&self) -> Option<String>;
}

/// A fixed key (examples, tests). `None` behaves like an unset key.
#[derive(Clone)]
pub struct StaticKey(pub Option<String>);

impl ApiKeySource for StaticKey {
    fn gnani_key(&self) -> Option<String> {
        self.0.clone()
    }
}

impl std::fmt::Debug for StaticKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the key itself.
        f.write_str(if self.0.is_some() {
            "StaticKey(<set>)"
        } else {
            "StaticKey(<none>)"
        })
    }
}

/// One outbound request, as written to the privacy log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundRecord {
    pub destination: Destination,
    pub purpose: String,
    pub bytes: u64,
    pub ref_id: Option<String>,
}

/// Records outbound requests. In the app this writes `privacy_log` through
/// `pluto_core::voice_store::log_outbound`; it must not block for long.
pub trait OutboundLog: Send + Sync {
    fn record(&self, record: OutboundRecord);
}

/// Discards records (tools and tests that don't check the log).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoLog;

impl OutboundLog for NoLog {
    fn record(&self, _record: OutboundRecord) {}
}

// ---- calls ---------------------------------------------------------------------------------

/// A Gnani REST family. Each has its own one-at-a-time queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Family {
    /// `POST /stt/v3` (clips ≤ 30 s).
    SttRest,
    /// `/stt/v3/batch/jobs…`.
    Batch,
    /// `POST /api/v1/tts/inference`.
    Tts,
    /// `/api/v1/tts/voice-clone/…` (P2).
    Clone,
}

impl Family {
    const ALL: [Family; 4] = [Family::SttRest, Family::Batch, Family::Tts, Family::Clone];

    fn index(self) -> usize {
        match self {
            Family::SttRest => 0,
            Family::Batch => 1,
            Family::Tts => 2,
            Family::Clone => 3,
        }
    }

    /// The privacy-log destination for this family.
    pub fn destination(self) -> Destination {
        match self {
            Family::SttRest => Destination::GnaniRest,
            Family::Batch => Destination::GnaniBatch,
            Family::Tts => Destination::GnaniTts,
            Family::Clone => Destination::GnaniClone,
        }
    }
}

/// What a call is, for the queue and the privacy log.
#[derive(Debug, Clone)]
pub struct Call {
    pub family: Family,
    /// Why it is sent, e.g. "meeting transcript". Never content.
    pub purpose: String,
    /// Approximate request body size.
    pub bytes: u64,
    /// Our id for the thing this call is about (a job, a meeting).
    pub ref_id: Option<String>,
}

impl Call {
    pub fn new(family: Family, purpose: impl Into<String>, bytes: u64) -> Self {
        Self {
            family,
            purpose: purpose.into(),
            bytes,
            ref_id: None,
        }
    }

    pub fn ref_id(mut self, id: impl Into<String>) -> Self {
        self.ref_id = Some(id.into());
        self
    }
}

/// When to retry. The production schedule is [`RetryPolicy::default`]; tests use shorter delays.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Wait before each retry; its length is the number of retries.
    pub delays: Vec<Duration>,
    /// The longest `Retry-After` honoured; a longer one fails as `rate_limited` instead of hanging.
    pub max_retry_after: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            delays: vec![
                Duration::from_millis(500),
                Duration::from_secs(1),
                Duration::from_secs(2),
            ],
            max_retry_after: Duration::from_secs(30),
        }
    }
}

// ---- client --------------------------------------------------------------------------------

/// The shared Gnani HTTP client. Clone the `Arc`, not the client.
pub struct GnaniHttp {
    client: Client,
    base_url: String,
    keys: Arc<dyn ApiKeySource>,
    log: Arc<dyn OutboundLog>,
    policy: RetryPolicy,
    queues: [Semaphore; 4],
    credit_exhausted: AtomicBool,
}

impl GnaniHttp {
    /// A client for `base_url` (normally [`DEFAULT_BASE_URL`]).
    pub fn new(
        base_url: impl Into<String>,
        keys: Arc<dyn ApiKeySource>,
        log: Arc<dyn OutboundLog>,
    ) -> VoiceResult<Self> {
        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|_| {
                VoiceError::new(VoiceErrorCode::Core, "Couldn't start the network client.")
            })?;
        Ok(Self {
            client,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            keys,
            log,
            policy: RetryPolicy::default(),
            queues: Family::ALL.map(|_| Semaphore::new(1)),
            credit_exhausted: AtomicBool::new(false),
        })
    }

    /// Replaces the retry schedule.
    pub fn with_policy(mut self, policy: RetryPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// `base_url` + `path` (`path` starts with `/`).
    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Whether a 402 has disabled Gnani calls.
    pub fn credit_exhausted(&self) -> bool {
        self.credit_exhausted.load(Ordering::SeqCst)
    }

    /// Re-enables calls after credit was added (Settings → retry).
    pub fn reset_credit(&self) {
        self.credit_exhausted.store(false, Ordering::SeqCst);
    }

    /// Runs one Gnani call: waits for the family's turn, then sends `build(client, url_base)` with
    /// the key header, retrying per the policy. `build` is called once per attempt (multipart
    /// bodies cannot be reused). Returns the successful (2xx) response.
    pub async fn execute<F>(&self, call: Call, build: F) -> VoiceResult<Response>
    where
        F: Fn(&Client) -> RequestBuilder,
    {
        if self.credit_exhausted() {
            return Err(insufficient_credit());
        }
        let key = self
            .keys
            .gnani_key()
            .filter(|k| !k.trim().is_empty())
            .ok_or_else(|| {
                VoiceError::new(
                    VoiceErrorCode::MissingApiKey,
                    "Add your Gnani API key in Settings → Voice & privacy.",
                )
            })?;
        let _turn = self.queues[call.family.index()]
            .acquire()
            .await
            .map_err(|_| {
                VoiceError::new(VoiceErrorCode::Busy, "Gnani requests are shutting down.")
            })?;

        let mut attempt = 0usize;
        loop {
            // Re-check: another call may have hit 402 while this one waited its turn.
            if self.credit_exhausted() {
                return Err(insufficient_credit());
            }
            self.log.record(OutboundRecord {
                destination: call.family.destination(),
                purpose: call.purpose.clone(),
                bytes: call.bytes,
                ref_id: call.ref_id.clone(),
            });
            let retry_left = self.policy.delays.get(attempt).copied();
            let outcome = build(&self.client)
                .header(API_KEY_HEADER, &key)
                .send()
                .await;
            let failure = match outcome {
                Ok(resp) if resp.status().is_success() => return Ok(resp),
                Ok(resp) => Failure::Status(resp),
                Err(e) => Failure::Transport(e),
            };
            let wait = match &failure {
                Failure::Transport(e) if e.is_connect() || e.is_timeout() || e.is_request() => {
                    retry_left
                }
                Failure::Status(resp) if is_retryable(resp.status()) => {
                    match (retry_left, retry_after(resp)) {
                        (Some(_), Some(ra)) if ra > self.policy.max_retry_after => None,
                        (Some(d), Some(ra)) => Some(d.max(ra)),
                        (d, _) => d,
                    }
                }
                _ => None,
            };
            match wait {
                Some(d) => {
                    tracing::debug!(family = ?call.family, attempt, "retrying Gnani request");
                    tokio::time::sleep(d).await;
                    attempt += 1;
                }
                None => {
                    let err = self.to_error(call.family, failure).await;
                    tracing::warn!(family = ?call.family, code = ?err.code, attempts = attempt + 1, "Gnani request failed");
                    return Err(err);
                }
            }
        }
    }

    /// [`execute`](Self::execute), then parses the JSON body.
    pub async fn execute_json<T, F>(&self, call: Call, build: F) -> VoiceResult<T>
    where
        T: DeserializeOwned,
        F: Fn(&Client) -> RequestBuilder,
    {
        let resp = self.execute(call, build).await?;
        resp.json::<T>().await.map_err(|_| {
            VoiceError::new(
                VoiceErrorCode::GnaniUpstream,
                "Gnani sent an answer Pluto couldn't read. Try again.",
            )
        })
    }

    async fn to_error(&self, family: Family, failure: Failure) -> VoiceError {
        match failure {
            Failure::Transport(e) if e.is_timeout() => VoiceError::new(
                VoiceErrorCode::GnaniUpstream,
                "Gnani took too long to answer. Try again.",
            ),
            Failure::Transport(_) => VoiceError::new(
                VoiceErrorCode::NetworkOffline,
                "Can't reach Gnani. Check your internet connection.",
            ),
            Failure::Status(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                let err = map_status(family, status, &body);
                if err.code == VoiceErrorCode::InsufficientCredit {
                    self.credit_exhausted.store(true, Ordering::SeqCst);
                }
                err
            }
        }
    }
}

enum Failure {
    Status(Response),
    Transport(reqwest::Error),
}

fn insufficient_credit() -> VoiceError {
    VoiceError::new(
        VoiceErrorCode::InsufficientCredit,
        "Your Gnani credit has run out, so voice is off. Add credit, then turn voice back on.",
    )
}

fn is_retryable(status: StatusCode) -> bool {
    matches!(status.as_u16(), 429 | 500 | 503 | 504)
}

/// `Retry-After` in whole seconds (the HTTP-date form is ignored).
fn retry_after(resp: &Response) -> Option<Duration> {
    resp.headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

/// Maps a final non-2xx answer to the contract's codes (API_CONTRACTS §6). The body is only
/// inspected for Gnani's error codes; it is never copied into the message.
pub fn map_status(family: Family, status: StatusCode, body: &str) -> VoiceError {
    let json: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    let detail_code = json
        .pointer("/detail/error_code")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match status.as_u16() {
        402 => insufficient_credit(),
        401 | 403 if detail_code.contains("missing") => VoiceError::new(
            VoiceErrorCode::MissingApiKey,
            "Add your Gnani API key in Settings → Voice & privacy.",
        ),
        401 | 403 => VoiceError::new(
            VoiceErrorCode::InvalidApiKey,
            "Gnani didn't accept your API key. Check it in Settings → Voice & privacy.",
        ),
        429 => VoiceError::new(
            VoiceErrorCode::RateLimited,
            "Gnani is busy right now. Try again in a moment.",
        ),
        413 => VoiceError::new(
            VoiceErrorCode::FileTooLarge,
            "That file is too large for Gnani (10 MB at most).",
        ),
        415 => VoiceError::new(
            VoiceErrorCode::UnsupportedAudio,
            "Gnani can't read that audio format.",
        ),
        500..=599 => VoiceError::new(
            VoiceErrorCode::GnaniUpstream,
            "Gnani is having trouble right now. Try again shortly.",
        ),
        // Batch errors are `{error, message}` (Create Job reference).
        _ if family == Family::Batch
            && json.get("error").and_then(|v| v.as_str()) == Some("FILE_TOO_LARGE") =>
        {
            VoiceError::new(
                VoiceErrorCode::FileTooLarge,
                "That file is too large for Gnani (10 MB at most).",
            )
        }
        _ if family == Family::Batch => VoiceError::new(
            VoiceErrorCode::BatchJobFailed,
            "Gnani couldn't run that transcription job.",
        ),
        _ => VoiceError::new(
            VoiceErrorCode::GnaniBadRequest,
            "Gnani couldn't process that request.",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::Instant;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[derive(Default)]
    struct MemoryLog(Mutex<Vec<OutboundRecord>>);
    impl OutboundLog for MemoryLog {
        fn record(&self, record: OutboundRecord) {
            self.0.lock().unwrap().push(record);
        }
    }

    fn fast() -> RetryPolicy {
        RetryPolicy {
            delays: vec![Duration::from_millis(10); 3],
            max_retry_after: Duration::from_secs(2),
        }
    }

    fn client(base: &str, key: Option<&str>) -> (GnaniHttp, Arc<MemoryLog>) {
        let log = Arc::new(MemoryLog::default());
        let http = GnaniHttp::new(
            base,
            Arc::new(StaticKey(key.map(String::from))),
            log.clone(),
        )
        .unwrap()
        .with_policy(fast());
        (http, log)
    }

    fn post(http: &GnaniHttp, p: &str) -> impl Fn(&Client) -> RequestBuilder {
        let url = http.url(p);
        move |c: &Client| c.post(&url).body("x")
    }

    async fn status_then(server: &MockServer, p: &str, first: u16, times: u64, then: u16) {
        Mock::given(method("POST"))
            .and(path(p))
            .respond_with(ResponseTemplate::new(first))
            .up_to_n_times(times)
            .mount(server)
            .await;
        Mock::given(method("POST"))
            .and(path(p))
            .respond_with(ResponseTemplate::new(then).set_body_string(r#"{"ok":true}"#))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn sends_the_key_header_and_returns_the_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/stt/v3"))
            .and(header(API_KEY_HEADER, "k-123"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(r#"{"success":true,"transcript":"haan"}"#),
            )
            .expect(1)
            .mount(&server)
            .await;
        let (http, log) = client(&server.uri(), Some("k-123"));
        let v: serde_json::Value = http
            .execute_json(
                Call::new(Family::SttRest, "clip", 1).ref_id("m1"),
                post(&http, "/stt/v3"),
            )
            .await
            .unwrap();
        assert_eq!(v["transcript"], "haan");
        let records = log.0.lock().unwrap().clone();
        assert_eq!(
            records,
            vec![OutboundRecord {
                destination: Destination::GnaniRest,
                purpose: "clip".into(),
                bytes: 1,
                ref_id: Some("m1".into())
            }]
        );
    }

    // T-FH-02: a 429 → 200 sequence succeeds silently.
    #[tokio::test]
    async fn a_429_then_200_succeeds_without_an_error() {
        let server = MockServer::start().await;
        status_then(&server, "/stt/v3", 429, 1, 200).await;
        let (http, log) = client(&server.uri(), Some("k"));
        let resp = http
            .execute(
                Call::new(Family::SttRest, "clip", 1),
                post(&http, "/stt/v3"),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(log.0.lock().unwrap().len(), 2, "both attempts are logged");
    }

    #[tokio::test]
    async fn server_errors_500_503_504_are_retried_until_success() {
        for code in [500, 503, 504] {
            let server = MockServer::start().await;
            status_then(&server, "/x", code, 3, 200).await;
            let (http, _) = client(&server.uri(), Some("k"));
            let resp = http
                .execute(Call::new(Family::Tts, "reply", 1), post(&http, "/x"))
                .await;
            assert!(resp.is_ok(), "{code} should be retried");
        }
    }

    #[tokio::test]
    async fn retries_stop_after_four_attempts() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(503))
            .expect(4)
            .mount(&server)
            .await;
        let (http, _) = client(&server.uri(), Some("k"));
        let err = http
            .execute(Call::new(Family::Tts, "reply", 1), post(&http, "/x"))
            .await
            .unwrap_err();
        assert_eq!(err.code, VoiceErrorCode::GnaniUpstream);
        assert!(err.retryable);
    }

    #[tokio::test]
    async fn persistent_429_becomes_rate_limited() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&server)
            .await;
        let (http, _) = client(&server.uri(), Some("k"));
        let err = http
            .execute(Call::new(Family::SttRest, "clip", 1), post(&http, "/x"))
            .await
            .unwrap_err();
        assert_eq!(err.code, VoiceErrorCode::RateLimited);
    }

    #[tokio::test]
    async fn retry_after_is_honoured_and_an_excessive_one_is_not_waited_for() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/short"))
            .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "1"))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/short"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/long"))
            .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "3600"))
            .expect(1)
            .mount(&server)
            .await;
        let (http, _) = client(&server.uri(), Some("k"));
        let started = Instant::now();
        http.execute(Call::new(Family::SttRest, "clip", 1), post(&http, "/short"))
            .await
            .unwrap();
        assert!(
            started.elapsed() >= Duration::from_millis(950),
            "waited the Retry-After"
        );
        let started = Instant::now();
        let err = http
            .execute(Call::new(Family::SttRest, "clip", 1), post(&http, "/long"))
            .await
            .unwrap_err();
        assert_eq!(err.code, VoiceErrorCode::RateLimited);
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "did not hang for an hour"
        );
    }

    // T-FH-03: a 402 → insufficient_credit, and voice stays off until reset.
    #[tokio::test]
    async fn a_402_disables_all_calls_until_credit_is_reset() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/stt/v3"))
            .respond_with(ResponseTemplate::new(402))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/tts"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        let (http, log) = client(&server.uri(), Some("k"));
        let err = http
            .execute(
                Call::new(Family::SttRest, "clip", 1),
                post(&http, "/stt/v3"),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, VoiceErrorCode::InsufficientCredit);
        assert!(!err.retryable);
        assert!(http.credit_exhausted());
        // Another family is refused without a request.
        let again = http
            .execute(Call::new(Family::Tts, "reply", 1), post(&http, "/tts"))
            .await
            .unwrap_err();
        assert_eq!(again.code, VoiceErrorCode::InsufficientCredit);
        assert_eq!(
            log.0.lock().unwrap().len(),
            1,
            "nothing else left the device"
        );
        http.reset_credit();
        http.execute(Call::new(Family::Tts, "reply", 1), post(&http, "/tts"))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn other_4xx_are_not_retried_and_map_to_the_contract_codes() {
        let cases: [(u16, &str, Family, VoiceErrorCode); 7] = [
            (401, "{}", Family::SttRest, VoiceErrorCode::InvalidApiKey),
            (
                403,
                r#"{"detail":{"error_code":"API_KEY_MISSING"}}"#,
                Family::SttRest,
                VoiceErrorCode::MissingApiKey,
            ),
            (
                400,
                r#"{"success":false,"error":{"type":"bad_audio"}}"#,
                Family::SttRest,
                VoiceErrorCode::GnaniBadRequest,
            ),
            (
                400,
                r#"{"error":"x","message":"bad config"}"#,
                Family::Batch,
                VoiceErrorCode::BatchJobFailed,
            ),
            (
                400,
                r#"{"error":"FILE_TOO_LARGE","message":"too big"}"#,
                Family::Batch,
                VoiceErrorCode::FileTooLarge,
            ),
            (413, "", Family::Batch, VoiceErrorCode::FileTooLarge),
            (415, "", Family::SttRest, VoiceErrorCode::UnsupportedAudio),
        ];
        for (status, body, family, expected) in cases {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(status).set_body_string(body))
                .expect(1)
                .mount(&server)
                .await;
            let (http, _) = client(&server.uri(), Some("k"));
            let err = http
                .execute(Call::new(family, "x", 1), post(&http, "/x"))
                .await
                .unwrap_err();
            assert_eq!(err.code, expected, "{status} {body}");
            assert!(
                !err.message.contains("bad"),
                "the body is never copied into the message"
            );
        }
    }

    #[tokio::test]
    async fn a_missing_key_fails_before_anything_is_sent() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;
        for key in [None, Some("  ")] {
            let (http, log) = client(&server.uri(), key);
            let err = http
                .execute(Call::new(Family::SttRest, "clip", 1), post(&http, "/x"))
                .await
                .unwrap_err();
            assert_eq!(err.code, VoiceErrorCode::MissingApiKey);
            assert!(log.0.lock().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn an_unreachable_host_is_retried_then_reports_offline() {
        // Bind a port, then free it: connecting to it is refused.
        let port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let (http, log) = client(&format!("http://127.0.0.1:{port}"), Some("k"));
        let err = http
            .execute(Call::new(Family::SttRest, "clip", 1), post(&http, "/x"))
            .await
            .unwrap_err();
        assert_eq!(err.code, VoiceErrorCode::NetworkOffline);
        assert!(err.retryable);
        assert_eq!(log.0.lock().unwrap().len(), 4, "1 attempt + 3 retries");
    }

    #[tokio::test]
    async fn one_request_in_flight_per_family_but_families_run_in_parallel() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(300)))
            .mount(&server)
            .await;
        let (http, _) = client(&server.uri(), Some("k"));
        let http = Arc::new(http);
        let run = |family: Family| {
            let http = http.clone();
            async move {
                let url = http.url("/x");
                http.execute(Call::new(family, "x", 1), move |c| c.post(&url))
                    .await
                    .unwrap();
            }
        };

        let started = Instant::now();
        tokio::join!(run(Family::Batch), run(Family::Batch));
        assert!(
            started.elapsed() >= Duration::from_millis(580),
            "same family: one after the other"
        );

        let started = Instant::now();
        tokio::join!(run(Family::Batch), run(Family::Tts));
        assert!(
            started.elapsed() < Duration::from_millis(560),
            "different families: at the same time"
        );
    }

    #[test]
    fn the_production_schedule_is_half_one_two_seconds() {
        let p = RetryPolicy::default();
        assert_eq!(
            p.delays,
            [
                Duration::from_millis(500),
                Duration::from_secs(1),
                Duration::from_secs(2)
            ]
        );
    }

    #[test]
    fn the_key_is_never_printed() {
        assert_eq!(
            format!("{:?}", StaticKey(Some("secret".into()))),
            "StaticKey(<set>)"
        );
        assert_eq!(format!("{:?}", StaticKey(None)), "StaticKey(<none>)");
    }

    #[test]
    fn families_log_to_their_destinations() {
        assert_eq!(Family::SttRest.destination(), Destination::GnaniRest);
        assert_eq!(Family::Batch.destination(), Destination::GnaniBatch);
        assert_eq!(Family::Tts.destination(), Destination::GnaniTts);
        assert_eq!(Family::Clone.destination(), Destination::GnaniClone);
        assert_eq!(
            map_status(Family::Tts, StatusCode::BAD_GATEWAY, "").code,
            VoiceErrorCode::GnaniUpstream
        );
    }
}
