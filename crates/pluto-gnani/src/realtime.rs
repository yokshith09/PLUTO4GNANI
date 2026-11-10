//! Prisma Realtime client (`wss://api.vachana.ai/stt/v3/stream`, API_CONTRACTS §2.1).
//! Owner: **D1** (T-101). Implements `SpeechToText`; used by the voice runtime (T-301) and the
//! recorder's live captions (T-501).
//!
//! One [`RtSession`] is one logical session; underneath it the client may use several WebSocket
//! connections:
//! - **Rotation:** Gnani ends a session at 15 minutes (close code 1001), so the client reconnects
//!   on its own after [`RealtimePolicy::rotate_after`] (14 min).
//! - **Reconnect:** a dropped connection or a 1001 is retried after 0.5 / 1 / 2 / 4 s. Meanwhile
//!   frames are buffered (the newest 10 s; older ones are dropped) and sent once connected again;
//!   a `commit` asked for meanwhile is sent after them. The caller sees `Reconnecting`, then
//!   `Connected`.
//! - **Ends:** `RtInput::Close`, dropping `input`, Gnani's own normal close (idle timeout), or
//!   running out of reconnect attempts (then an `Error` first). `events` always ends with `Closed`.
//!
//! There are no partial transcripts: each `transcript` message is final. Segment numbers are our
//! own, continuous across reconnects. Every connection is written to the privacy log when it ends,
//! with the bytes sent over it. Gnani's own error text is never copied into a message.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use pluto_contracts::error::{VoiceError, VoiceErrorCode, VoiceResult};
use pluto_contracts::speech::{
    RtConfig, RtEvent, RtInput, RtSession, RtTranscript, SpeechToText, CAPTURE_SAMPLE_RATE,
};
use pluto_contracts::voice::Destination;
use serde::Deserialize;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{sleep, sleep_until, timeout, Instant};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::handshake::client::Request;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::http::{
    cloud_off, map_status, ApiKeySource, CloudSwitch, Family, OutboundLog, OutboundRecord,
};

/// Prisma Realtime endpoint.
pub const DEFAULT_URL: &str = "wss://api.vachana.ai/stt/v3/stream";
/// The model, sent as both `model` and `x-model` (the docs use both spellings).
pub const MODEL: &str = "prisma-v3.0";
/// What the privacy log says each connection was for.
pub const PURPOSE: &str = "live speech to text";

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Timing of rotation and reconnects. [`RealtimePolicy::default`] is the production schedule.
#[derive(Debug, Clone)]
pub struct RealtimePolicy {
    /// Reconnect proactively after this long (Gnani's cap is 15 min).
    pub rotate_after: Duration,
    /// Wait before each reconnect attempt; its length is the number of attempts.
    pub reconnect_delays: Vec<Duration>,
    /// Frames kept while disconnected (the newest win). 313 frames ≈ 10 s.
    pub buffer_frames: usize,
    /// How long one connection attempt (TCP + TLS + handshake) may take.
    pub connect_timeout: Duration,
}

impl Default for RealtimePolicy {
    fn default() -> Self {
        Self {
            rotate_after: Duration::from_secs(14 * 60),
            reconnect_delays: [500, 1_000, 2_000, 4_000]
                .into_iter()
                .map(Duration::from_millis)
                .collect(),
            buffer_frames: 313,
            connect_timeout: Duration::from_secs(10),
        }
    }
}

/// The Prisma Realtime client. Cheap to share (`Arc`); each `open` starts an independent session.
pub struct RealtimeClient {
    url: String,
    keys: Arc<dyn ApiKeySource>,
    log: Arc<dyn OutboundLog>,
    policy: RealtimePolicy,
    cloud: Arc<CloudSwitch>,
}

impl RealtimeClient {
    /// A client for `url` (normally [`DEFAULT_URL`]).
    pub fn new(
        url: impl Into<String>,
        keys: Arc<dyn ApiKeySource>,
        log: Arc<dyn OutboundLog>,
    ) -> Self {
        Self {
            url: url.into(),
            keys,
            log,
            policy: RealtimePolicy::default(),
            cloud: CloudSwitch::new(true),
        }
    }

    /// Shares the app's Cloud voice switch (always on otherwise).
    pub fn with_cloud(mut self, cloud: Arc<CloudSwitch>) -> Self {
        self.cloud = cloud;
        self
    }

    pub fn with_policy(mut self, policy: RealtimePolicy) -> Self {
        self.policy = policy;
        self
    }

    fn key(&self) -> VoiceResult<String> {
        self.keys
            .gnani_key()
            .filter(|k| !k.trim().is_empty())
            .ok_or_else(|| {
                VoiceError::new(
                    VoiceErrorCode::MissingApiKey,
                    "Add your Gnani API key in Settings → Voice & privacy.",
                )
            })
    }
}

#[async_trait::async_trait]
impl SpeechToText for RealtimeClient {
    async fn open(&self, config: RtConfig) -> VoiceResult<RtSession> {
        self.cloud.check()?;
        let key = self.key()?;
        let request = handshake(&self.url, &key, &config)?;
        // A key or credit problem is reported at once. A dropped connection attempt (a reset
        // handshake, a DNS blip, Gnani not yet letting go of the last session) is tried again
        // twice, quickly, before saying the internet is the problem: listening is already shown
        // and what is said meanwhile is kept by the caller.
        let mut tries: u32 = 0;
        let ws = loop {
            match connect(request.clone(), self.policy.connect_timeout).await {
                Ok(ws) => break ws,
                Err(e) if e.code == VoiceErrorCode::NetworkOffline && tries < 2 => {
                    tries += 1;
                    #[cfg(debug_assertions)]
                    eprintln!("[gnani] realtime connect failed, retry {tries}");
                    sleep(Duration::from_millis(300 * u64::from(tries))).await;
                }
                Err(e) => return Err(e),
            }
        };
        let id = pluto_core::ids::new_id();
        let (input_tx, input_rx) = mpsc::channel::<RtInput>(256);
        let (event_tx, event_rx) = mpsc::channel::<RtEvent>(256);
        let driver = Driver {
            session_id: id.clone(),
            url: self.url.clone(),
            key,
            config,
            policy: self.policy.clone(),
            log: self.log.clone(),
            cloud: self.cloud.clone(),
            input: input_rx,
            events: event_tx,
            buffer: VecDeque::new(),
            pending_commit: false,
            next_segment: 0,
            bytes_this_connection: 0,
            connected: false,
        };
        tokio::spawn(driver.run(ws));
        Ok(RtSession {
            id,
            input: input_tx,
            events: event_rx,
        })
    }
}

// ---- handshake ------------------------------------------------------------------------------

/// JSON for a header: non-ASCII characters become `\uXXXX` escapes (HTTP headers are ASCII;
/// API_CONTRACTS §0). Surrogate pairs are used above U+FFFF.
pub fn ascii_json(terms: &[String]) -> String {
    let json = serde_json::to_string(terms).unwrap_or_else(|_| "[]".into());
    let mut out = String::with_capacity(json.len());
    for c in json.chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            let mut units = [0u16; 2];
            for unit in c.encode_utf16(&mut units) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    out
}

fn header(value: &str) -> VoiceResult<HeaderValue> {
    HeaderValue::from_str(value).map_err(|_| {
        VoiceError::new(
            VoiceErrorCode::GnaniBadRequest,
            "A speech setting contains characters Gnani can't accept.",
        )
    })
}

/// The handshake request with every header from API_CONTRACTS §2.1.
pub fn handshake(url: &str, key: &str, config: &RtConfig) -> VoiceResult<Request> {
    let mut request = url.into_client_request().map_err(|_| {
        VoiceError::new(
            VoiceErrorCode::Core,
            "The speech service address is invalid.",
        )
    })?;
    let h = request.headers_mut();
    h.insert("x-api-key-id", header(key)?);
    h.insert("model", HeaderValue::from_static(MODEL));
    h.insert("x-model", HeaderValue::from_static(MODEL));
    let language = if config.language_code.trim().is_empty() {
        "indic-auto"
    } else {
        config.language_code.trim()
    };
    h.insert("language_code", header(language)?);
    h.insert("format", HeaderValue::from_static(config.format.as_gnani()));
    h.insert("x-sample-rate", header(&CAPTURE_SAMPLE_RATE.to_string())?);
    if !config.keyterms.is_empty() {
        let terms: Vec<String> = config.keyterms.iter().take(200).cloned().collect();
        h.insert("keyterms", header(&ascii_json(&terms))?);
    }
    if let Some(secs) = config.idle_timeout_sec {
        h.insert(
            "x-idle-timeout-sec",
            header(&secs.clamp(5, 900).to_string())?,
        );
    }
    if let Some(ms) = config.min_silence_ms {
        h.insert("x-min-silence-ms", header(&ms.to_string())?);
    }
    Ok(request)
}

/// One connection attempt, with Gnani's HTTP answers mapped like the REST calls (T-103).
async fn connect(request: Request, limit: Duration) -> VoiceResult<Ws> {
    match timeout(limit, tokio_tungstenite::connect_async(request)).await {
        Err(_) => Err(VoiceError::new(
            VoiceErrorCode::NetworkOffline,
            "Can't reach Gnani. Check your internet connection.",
        )),
        Ok(Ok((ws, _response))) => Ok(ws),
        Ok(Err(WsError::Http(response))) => {
            let status = reqwest::StatusCode::from_u16(response.status().as_u16())
                .unwrap_or(reqwest::StatusCode::BAD_GATEWAY);
            let body = response
                .body()
                .as_deref()
                .map(String::from_utf8_lossy)
                .unwrap_or_default();
            Err(map_status(Family::SttRest, status, &body))
        }
        Ok(Err(e)) => {
            #[cfg(debug_assertions)]
            eprintln!("[gnani] realtime connect error: {e}");
            #[cfg(not(debug_assertions))]
            let _ = e;
            Err(VoiceError::new(
                VoiceErrorCode::NetworkOffline,
                "Can't reach Gnani. Check your internet connection.",
            ))
        }
    }
}

// ---- server messages ------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ServerMessage {
    #[serde(rename = "type")]
    kind: String,
    request_id: Option<String>,
    text: Option<String>,
    detected_language_code: Option<String>,
    audio_duration_ms: Option<f64>,
    latency: Option<f64>,
    /// Gnani's error text: only classified (key / credit / other), never shown or logged.
    message: Option<String>,
}

/// Maps Gnani's realtime `error` message to a contract code. The text itself is never passed on.
fn realtime_error(message: Option<&str>) -> VoiceError {
    let m = message.unwrap_or_default().to_ascii_lowercase();
    if m.contains("api key")
        || m.contains("api_key")
        || m.contains("unauthor")
        || m.contains("auth")
    {
        VoiceError::new(
            VoiceErrorCode::InvalidApiKey,
            "Gnani didn't accept your API key. Check it in Settings → Voice & privacy.",
        )
    } else if m.contains("credit") || m.contains("quota") || m.contains("balance") {
        VoiceError::new(
            VoiceErrorCode::InsufficientCredit,
            "Your Gnani credit has run out, so voice is off. Add credit, then turn voice back on.",
        )
    } else {
        VoiceError::new(
            VoiceErrorCode::GnaniUpstream,
            "Gnani's speech service reported a problem. Try again.",
        )
    }
}

/// Gnani's `latency` is in milliseconds (undocumented; seen on the live check, T-GN-01:
/// values such as 74, 97 and 164 for short utterances).
fn latency_ms(v: f64) -> u32 {
    if v.is_finite() {
        v.round().clamp(0.0, f64::from(u32::MAX)) as u32
    } else {
        0
    }
}

/// What one server message meant for the connection.
enum Handled {
    Continue,
    /// The caller dropped the events receiver.
    Gone,
    /// Gnani refused the session before `connected`.
    Refused,
}

/// How a connection ended.
enum Ended {
    /// The caller closed the session (or dropped `input`).
    ByCaller,
    /// Gnani closed normally (e.g. idle timeout): the session is over.
    ByServer,
    /// Dropped, 1001, an error, or time to rotate: reconnect.
    Reconnect,
}

struct Driver {
    session_id: String,
    /// What each reconnect rebuilds its handshake from (`http::Request` can't be cloned).
    url: String,
    key: String,
    config: RtConfig,
    policy: RealtimePolicy,
    log: Arc<dyn OutboundLog>,
    /// Turned off mid-session: no more audio is sent, and the session ends.
    cloud: Arc<CloudSwitch>,
    input: mpsc::Receiver<RtInput>,
    events: mpsc::Sender<RtEvent>,
    buffer: VecDeque<Vec<u8>>,
    pending_commit: bool,
    next_segment: u32,
    bytes_this_connection: u64,
    /// Gnani said `connected` on the current connection.
    connected: bool,
}

impl Driver {
    async fn emit(&self, event: RtEvent) -> bool {
        self.events.send(event).await.is_ok()
    }

    fn keep(&mut self, frame: Vec<u8>) {
        self.buffer.push_back(frame);
        while self.buffer.len() > self.policy.buffer_frames {
            self.buffer.pop_front();
        }
    }

    fn log_connection(&mut self) {
        self.log.record(OutboundRecord {
            destination: Destination::GnaniRealtime,
            purpose: PURPOSE.into(),
            bytes: std::mem::take(&mut self.bytes_this_connection),
            ref_id: Some(self.session_id.clone()),
        });
    }

    async fn run(mut self, first: Ws) {
        let mut ws = first;
        loop {
            let ended = self.serve(&mut ws).await;
            self.log_connection();
            match ended {
                Ended::ByCaller => {
                    let _ = ws.close(None).await;
                    break;
                }
                Ended::ByServer => break,
                Ended::Reconnect => {
                    let _ = ws.close(None).await;
                    match self.reconnect().await {
                        Some(next) => ws = next,
                        None => break,
                    }
                }
            }
        }
        let _ = self.emit(RtEvent::Closed).await;
    }

    /// Runs one connection until it ends.
    async fn serve(&mut self, ws: &mut Ws) -> Ended {
        self.connected = false;
        if !self.cloud.is_on() {
            let _ = self.emit(RtEvent::Error(cloud_off())).await;
            return Ended::ByServer;
        }
        let rotate_at = Instant::now() + self.policy.rotate_after;
        // Audio buffered while disconnected goes first, then a commit asked for meanwhile.
        while let Some(frame) = self.buffer.pop_front() {
            let n = frame.len() as u64;
            if ws.send(Message::Binary(frame.into())).await.is_err() {
                return Ended::Reconnect;
            }
            self.bytes_this_connection += n;
        }
        if std::mem::take(&mut self.pending_commit) && ws.send(commit()).await.is_err() {
            self.pending_commit = true;
            return Ended::Reconnect;
        }
        loop {
            tokio::select! {
                input = self.input.recv() => match input {
                    Some(RtInput::Frame(frame)) => {
                        if !self.cloud.is_on() {
                            let _ = self.emit(RtEvent::Error(cloud_off())).await;
                            return Ended::ByServer;
                        }
                        let n = frame.len() as u64;
                        if let Err(e) = ws.send(Message::Binary(frame.clone().into())).await {
                            tracing::debug!(error = %e, "realtime send failed; reconnecting");
                            self.keep(frame);
                            return Ended::Reconnect;
                        }
                        self.bytes_this_connection += n;
                    }
                    Some(RtInput::Commit) => {
                        if ws.send(commit()).await.is_err() {
                            self.pending_commit = true;
                            return Ended::Reconnect;
                        }
                    }
                    Some(RtInput::Close) | None => return Ended::ByCaller,
                },
                message = ws.next() => match message {
                    Some(Ok(Message::Text(text))) => {
                        match self.handle(text.as_str()).await {
                            Handled::Continue => {}
                            Handled::Gone => return Ended::ByCaller, // nobody is listening any more
                            Handled::Refused => return Ended::ByServer,
                        }
                    }
                    Some(Ok(Message::Close(frame))) => {
                        let code = frame.map(|f| f.code);
                        return match code {
                            None | Some(CloseCode::Normal) => Ended::ByServer,
                            Some(_) => Ended::Reconnect, // 1001 (15-min cap) and errors
                        };
                    }
                    Some(Ok(_)) => {} // binary, ping, pong: nothing for us
                    Some(Err(_)) | None => return Ended::Reconnect,
                },
                () = sleep_until(rotate_at) => {
                    tracing::debug!("rotating the realtime connection before Gnani's 15-minute cap");
                    return Ended::Reconnect;
                }
            }
        }
    }

    /// One server message. Returns false when the event receiver is gone.
    async fn handle(&mut self, text: &str) -> Handled {
        let Ok(msg) = serde_json::from_str::<ServerMessage>(text) else {
            tracing::debug!("ignored an unreadable realtime message");
            return Handled::Continue;
        };
        // An error before `connected` means Gnani refused the session's settings: reconnecting
        // with the same ones would only be refused again, forever.
        if msg.kind == "error" && !self.connected {
            let mut e = realtime_error(msg.message.as_deref());
            if e.code == VoiceErrorCode::GnaniUpstream {
                e = VoiceError::new(
                    VoiceErrorCode::GnaniBadRequest,
                    "Gnani refused the speech settings. In Settings → Voice & privacy, choose one language or none (automatic).",
                );
            }
            let e = e.retryable(false);
            tracing::warn!(code = ?e.code, "Gnani refused the realtime session");
            let _ = self.emit(RtEvent::Error(e)).await;
            return Handled::Refused;
        }
        if msg.kind == "connected" {
            self.connected = true;
        }
        let event = match msg.kind.as_str() {
            "connected" => RtEvent::Connected {
                request_id: msg.request_id,
            },
            "speech_start" => RtEvent::SpeechStart,
            "processing" => RtEvent::Processing,
            "transcript" => {
                let segment_index = self.next_segment;
                self.next_segment += 1;
                RtEvent::Transcript(RtTranscript {
                    text: msg.text.unwrap_or_default(),
                    segment_index,
                    language: msg.detected_language_code.filter(|l| !l.trim().is_empty()),
                    audio_duration_ms: msg
                        .audio_duration_ms
                        .map(|v| v.round().clamp(0.0, f64::from(u32::MAX)) as u32),
                    latency_ms: msg.latency.map(latency_ms),
                })
            }
            "error" => RtEvent::Error(realtime_error(msg.message.as_deref())),
            _ => return Handled::Continue,
        };
        if self.emit(event).await {
            Handled::Continue
        } else {
            Handled::Gone
        }
    }

    /// Tries to connect again, buffering input meanwhile. `None` when the session should end.
    async fn reconnect(&mut self) -> Option<Ws> {
        let delays = self.policy.reconnect_delays.clone();
        let mut last = VoiceError::new(
            VoiceErrorCode::NetworkOffline,
            "Lost the connection to Gnani. Check your internet connection.",
        );
        for (i, delay) in delays.iter().enumerate() {
            let attempt = u32::try_from(i + 1).unwrap_or(u32::MAX);
            if !self.emit(RtEvent::Reconnecting { attempt }).await {
                return None;
            }
            // Keep taking input while waiting, so nothing the caller sends is lost or blocked.
            let wait = sleep(*delay);
            tokio::pin!(wait);
            loop {
                tokio::select! {
                    () = &mut wait => break,
                    input = self.input.recv() => match input {
                        Some(RtInput::Frame(frame)) => self.keep(frame),
                        Some(RtInput::Commit) => self.pending_commit = true,
                        Some(RtInput::Close) | None => return None,
                    },
                }
            }
            if !self.cloud.is_on() {
                last = cloud_off();
                break;
            }
            let attempt_result = match handshake(&self.url, &self.key, &self.config) {
                Ok(request) => connect(request, self.policy.connect_timeout).await,
                Err(e) => Err(e),
            };
            match attempt_result {
                Ok(ws) => return Some(ws),
                Err(e) => {
                    let fatal = matches!(
                        e.code,
                        VoiceErrorCode::InvalidApiKey
                            | VoiceErrorCode::MissingApiKey
                            | VoiceErrorCode::InsufficientCredit
                    );
                    last = e;
                    if fatal {
                        break;
                    }
                }
            }
        }
        let _ = self.emit(RtEvent::Error(last)).await;
        None
    }
}

fn commit() -> Message {
    Message::Text(r#"{"type":"commit"}"#.into())
}

#[cfg(test)]
mod tests;
