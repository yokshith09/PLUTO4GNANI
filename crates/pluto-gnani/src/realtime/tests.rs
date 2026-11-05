//! Contract tests against a scripted fake Prisma Realtime server (TESTING_STRATEGY §1
//! "Contract"; T-FH-04 for close code 1001). No Gnani key or network is needed.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use pluto_contracts::error::VoiceErrorCode;
use pluto_contracts::speech::{RtConfig, RtEvent, RtInput, RtSession, SpeechToText, FRAME_BYTES};
use pluto_contracts::voice::SttFormat;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::Message;

use super::*;
use crate::http::{OutboundLog, OutboundRecord, StaticKey};

// ---- fake server -----------------------------------------------------------------------------

#[derive(Debug, Default, Clone)]
struct Conn {
    headers: HashMap<String, String>,
    bytes: usize,
    commits: usize,
}

#[derive(Clone, Default)]
struct Script {
    transcripts: Vec<String>,
    /// Close the first connection with this code once it has received `close_after_frames`.
    close_first: Option<u16>,
    close_after_frames: usize,
    /// Refuse the handshake with this HTTP status (and body).
    reject: Option<(u16, &'static str)>,
    /// Accept only this many connections, then stop listening.
    accept_only: Option<usize>,
    /// Send `{"type":"error"}` right after `connected`.
    error_after_connect: bool,
    /// Send `{"type":"error"}` instead of `connected` (Gnani refusing the settings), then close.
    refuse_settings: bool,
}

struct Server {
    url: String,
    conns: Arc<Mutex<Vec<Conn>>>,
}

impl Server {
    fn conns(&self) -> Vec<Conn> {
        self.conns.lock().unwrap().clone()
    }
}

// tungstenite's handshake callback returns `Result<Response, ErrorResponse>`; its size isn't ours.
#[allow(clippy::result_large_err)]
async fn serve(script: Script) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let conns: Arc<Mutex<Vec<Conn>>> = Arc::default();
    let said = Arc::new(Mutex::new(0usize));
    let state = conns.clone();
    tokio::spawn(async move {
        let mut accepted = 0usize;
        loop {
            if script.accept_only.is_some_and(|n| accepted >= n) {
                drop(listener);
                return;
            }
            let Ok((tcp, _)) = listener.accept().await else {
                return;
            };
            accepted += 1;
            let index = accepted - 1;
            let (script, state, said) = (script.clone(), state.clone(), said.clone());
            tokio::spawn(async move {
                let headers = Arc::new(Mutex::new(HashMap::new()));
                let seen = headers.clone();
                let reject = script.reject;
                let callback =
                    move |req: &Request, resp: Response| -> Result<Response, ErrorResponse> {
                        let mut h = seen.lock().unwrap();
                        for (k, v) in req.headers() {
                            h.insert(
                                k.as_str().to_string(),
                                v.to_str().unwrap_or("<non-ascii>").to_string(),
                            );
                        }
                        if let Some((status, body)) = reject {
                            return Err(http::Response::builder()
                                .status(status)
                                .body(Some(body.to_string()))
                                .unwrap());
                        }
                        Ok(resp)
                    };
                let Ok(mut ws) = tokio_tungstenite::accept_hdr_async(tcp, callback).await else {
                    state.lock().unwrap().push(Conn {
                        headers: headers.lock().unwrap().clone(),
                        ..Conn::default()
                    });
                    return;
                };
                state.lock().unwrap().push(Conn {
                    headers: headers.lock().unwrap().clone(),
                    ..Conn::default()
                });
                let slot = state.lock().unwrap().len() - 1;
                if script.refuse_settings {
                    let _ = ws
                        .send(Message::Text(
                            r#"{"type":"error","message":"invalid language_code"}"#.into(),
                        ))
                        .await;
                    let _ = ws.close(None).await;
                    return;
                }
                let rid = format!(r#"{{"type":"connected","request_id":"rq-{index}"}}"#);
                if ws.send(Message::Text(rid.into())).await.is_err() {
                    return;
                }
                if script.error_after_connect {
                    let _ = ws
                        .send(Message::Text(
                            r#"{"type":"error","message":"secret detail"}"#.into(),
                        ))
                        .await;
                }
                let mut frames = 0usize;
                while let Some(Ok(msg)) = ws.next().await {
                    match msg {
                        Message::Binary(b) => {
                            frames += 1;
                            state.lock().unwrap()[slot].bytes += b.len();
                            if index == 0
                                && script.close_first.is_some()
                                && frames >= script.close_after_frames
                            {
                                let code = CloseCode::from(script.close_first.unwrap_or(1000));
                                let _ = ws
                                    .send(Message::Close(Some(CloseFrame {
                                        code,
                                        reason: "".into(),
                                    })))
                                    .await;
                                return;
                            }
                        }
                        Message::Text(t) if t.as_str().contains("commit") => {
                            state.lock().unwrap()[slot].commits += 1;
                            let n = {
                                let mut s = said.lock().unwrap();
                                *s += 1;
                                *s - 1
                            };
                            let text = script.transcripts.get(n).cloned().unwrap_or_default();
                            let _ = ws
                                .send(Message::Text(r#"{"type":"processing"}"#.into()))
                                .await;
                            let reply = serde_json::json!({
                                "type": "transcript", "text": text, "segment_id": format!("seg-{n}"),
                                "segment_index": 0, "detected_language_code": "hi-IN",
                                "audio_duration_ms": 1536.4, "latency": 420
                            });
                            let _ = ws.send(Message::Text(reply.to_string().into())).await;
                        }
                        Message::Close(_) => return,
                        _ => {}
                    }
                }
            });
        }
    });
    Server { url, conns }
}

// ---- helpers ---------------------------------------------------------------------------------

#[derive(Default)]
struct MemoryLog(Mutex<Vec<OutboundRecord>>);
impl OutboundLog for MemoryLog {
    fn record(&self, r: OutboundRecord) {
        self.0.lock().unwrap().push(r);
    }
}

fn fast() -> RealtimePolicy {
    RealtimePolicy {
        rotate_after: Duration::from_secs(3600),
        reconnect_delays: vec![Duration::from_millis(30); 3],
        buffer_frames: 313,
        connect_timeout: Duration::from_secs(2),
    }
}

fn client(url: &str, key: Option<&str>) -> (RealtimeClient, Arc<MemoryLog>) {
    let log = Arc::new(MemoryLog::default());
    let c = RealtimeClient::new(url, Arc::new(StaticKey(key.map(String::from))), log.clone())
        .with_policy(fast());
    (c, log)
}

fn frame() -> RtInput {
    RtInput::Frame(vec![7; FRAME_BYTES])
}

async fn next(s: &mut RtSession) -> RtEvent {
    tokio::time::timeout(Duration::from_secs(5), s.events.recv())
        .await
        .expect("an event within 5 s")
        .expect("the session is still open")
}

/// Skips events until one matches.
async fn until(s: &mut RtSession, want: impl Fn(&RtEvent) -> bool) -> RtEvent {
    loop {
        let e = next(s).await;
        if want(&e) {
            return e;
        }
    }
}

async fn drain(s: &mut RtSession) -> Vec<RtEvent> {
    let mut all = Vec::new();
    while let Ok(Some(e)) = tokio::time::timeout(Duration::from_secs(5), s.events.recv()).await {
        let closed = e == RtEvent::Closed;
        all.push(e);
        if closed {
            break;
        }
    }
    all
}

// ---- tests -----------------------------------------------------------------------------------

#[tokio::test]
async fn the_handshake_sends_every_documented_header() {
    let server = serve(Script::default()).await;
    let (c, _) = client(&server.url, Some("k-123"));
    let config = RtConfig {
        language_code: "hi-IN,en-IN".into(),
        format: SttFormat::Transliterate,
        keyterms: vec!["login fix".into(), "लॉगिन".into()],
        idle_timeout_sec: Some(2),
        min_silence_ms: Some(400),
    };
    let mut s = c.open(config).await.unwrap();
    until(&mut s, |e| matches!(e, RtEvent::Connected { .. })).await;
    let h = &server.conns()[0].headers;
    assert_eq!(h["x-api-key-id"], "k-123");
    assert_eq!(h["model"], "prisma-v3.0");
    assert_eq!(h["x-model"], "prisma-v3.0");
    assert_eq!(h["language_code"], "hi-IN,en-IN");
    assert_eq!(h["format"], "transliterate");
    assert_eq!(h["x-sample-rate"], "16000");
    assert_eq!(
        h["keyterms"],
        r#"["login fix","\u0932\u0949\u0917\u093f\u0928"]"#
    );
    assert_eq!(h["x-idle-timeout-sec"], "5", "clamped to Gnani's minimum");
    assert_eq!(h["x-min-silence-ms"], "400");
    let _ = s.input.send(RtInput::Close).await;
}

#[tokio::test]
async fn defaults_are_indic_auto_codemix_and_no_optional_headers() {
    let server = serve(Script::default()).await;
    let (c, _) = client(&server.url, Some("k"));
    let mut s = c.open(RtConfig::default()).await.unwrap();
    until(&mut s, |e| matches!(e, RtEvent::Connected { .. })).await;
    let h = &server.conns()[0].headers;
    assert_eq!(h["language_code"], "indic-auto");
    assert_eq!(h["format"], "codemix");
    assert!(!h.contains_key("keyterms") && !h.contains_key("x-idle-timeout-sec"));
}

#[tokio::test]
async fn frames_then_commit_give_one_final_transcript_with_the_detected_language() {
    let server = serve(Script {
        transcripts: vec!["kal 5 baje reminder lagao".into()],
        ..Script::default()
    })
    .await;
    let (c, log) = client(&server.url, Some("k"));
    let mut s = c.open(RtConfig::default()).await.unwrap();
    assert_eq!(
        next(&mut s).await,
        RtEvent::Connected {
            request_id: Some("rq-0".into())
        }
    );
    for _ in 0..3 {
        s.input.send(frame()).await.unwrap();
    }
    s.input.send(RtInput::Commit).await.unwrap();
    assert_eq!(next(&mut s).await, RtEvent::Processing);
    let RtEvent::Transcript(t) = next(&mut s).await else {
        panic!("expected a transcript")
    };
    assert_eq!(t.text, "kal 5 baje reminder lagao");
    assert_eq!(t.language.as_deref(), Some("hi-IN"));
    assert_eq!(t.segment_index, 0);
    assert_eq!(t.audio_duration_ms, Some(1536));
    assert_eq!(t.latency_ms, Some(420));
    s.input.send(RtInput::Close).await.unwrap();
    assert_eq!(drain(&mut s).await.last(), Some(&RtEvent::Closed));
    let conn = &server.conns()[0];
    assert_eq!((conn.bytes, conn.commits), (3 * FRAME_BYTES, 1));
    let records = log.0.lock().unwrap().clone();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].destination, Destination::GnaniRealtime);
    assert_eq!(records[0].bytes, (3 * FRAME_BYTES) as u64);
    assert_eq!(records[0].ref_id.as_deref(), Some(s.id.as_str()));
}

// T-FH-04: close code 1001 → the client reconnects; audio sent meanwhile is not lost.
#[tokio::test]
async fn close_code_1001_reconnects_and_buffered_audio_reaches_the_new_connection() {
    let server = serve(Script {
        transcripts: vec!["haan".into()],
        close_first: Some(1001),
        close_after_frames: 2,
        ..Script::default()
    })
    .await;
    let (c, log) = client(&server.url, Some("k"));
    let mut s = c.open(RtConfig::default()).await.unwrap();
    until(&mut s, |e| matches!(e, RtEvent::Connected { .. })).await;
    s.input.send(frame()).await.unwrap();
    s.input.send(frame()).await.unwrap();
    assert_eq!(
        until(&mut s, |e| matches!(e, RtEvent::Reconnecting { .. })).await,
        RtEvent::Reconnecting { attempt: 1 }
    );
    // Sent while disconnected: buffered, then delivered in order, commit last.
    for _ in 0..3 {
        s.input.send(frame()).await.unwrap();
    }
    s.input.send(RtInput::Commit).await.unwrap();
    assert_eq!(
        until(&mut s, |e| matches!(e, RtEvent::Connected { .. })).await,
        RtEvent::Connected {
            request_id: Some("rq-1".into())
        }
    );
    let RtEvent::Transcript(t) = until(&mut s, |e| matches!(e, RtEvent::Transcript(_))).await
    else {
        unreachable!()
    };
    assert_eq!(t.text, "haan");
    s.input.send(RtInput::Close).await.unwrap();
    drain(&mut s).await;
    let conns = server.conns();
    assert_eq!(conns.len(), 2);
    assert_eq!(conns[0].bytes, 2 * FRAME_BYTES);
    assert_eq!((conns[1].bytes, conns[1].commits), (3 * FRAME_BYTES, 1));
    assert_eq!(
        log.0.lock().unwrap().len(),
        2,
        "one privacy-log entry per connection"
    );
}

#[tokio::test]
async fn the_connection_is_rotated_before_the_session_cap() {
    let server = serve(Script::default()).await;
    let log = Arc::new(MemoryLog::default());
    let c = RealtimeClient::new(&server.url, Arc::new(StaticKey(Some("k".into()))), log)
        .with_policy(RealtimePolicy {
            rotate_after: Duration::from_millis(300),
            ..fast()
        });
    let mut s = c.open(RtConfig::default()).await.unwrap();
    until(&mut s, |e| matches!(e, RtEvent::Connected { .. })).await;
    assert_eq!(
        until(&mut s, |e| matches!(e, RtEvent::Reconnecting { .. })).await,
        RtEvent::Reconnecting { attempt: 1 }
    );
    until(&mut s, |e| matches!(e, RtEvent::Connected { .. })).await;
    assert_eq!(server.conns().len(), 2);
    let _ = s.input.send(RtInput::Close).await;
}

#[tokio::test]
async fn a_normal_close_by_gnani_ends_the_session_without_reconnecting() {
    let server = serve(Script {
        close_first: Some(1000),
        close_after_frames: 1,
        ..Script::default()
    })
    .await;
    let (c, _) = client(&server.url, Some("k"));
    let mut s = c.open(RtConfig::default()).await.unwrap();
    s.input.send(frame()).await.unwrap();
    let events = drain(&mut s).await;
    assert_eq!(events.last(), Some(&RtEvent::Closed));
    assert!(!events
        .iter()
        .any(|e| matches!(e, RtEvent::Reconnecting { .. })));
    assert_eq!(server.conns().len(), 1);
}

#[tokio::test]
async fn when_reconnecting_keeps_failing_the_session_reports_offline_and_closes() {
    let server = serve(Script {
        close_first: Some(1001),
        close_after_frames: 1,
        accept_only: Some(1),
        ..Script::default()
    })
    .await;
    let (c, _) = client(&server.url, Some("k"));
    let mut s = c.open(RtConfig::default()).await.unwrap();
    s.input.send(frame()).await.unwrap();
    let events = drain(&mut s).await;
    let attempts: Vec<u32> = events
        .iter()
        .filter_map(|e| match e {
            RtEvent::Reconnecting { attempt } => Some(*attempt),
            _ => None,
        })
        .collect();
    assert_eq!(attempts, [1, 2, 3]);
    let n = events.len();
    assert!(
        matches!(&events[n - 2], RtEvent::Error(e) if e.code == VoiceErrorCode::NetworkOffline && e.retryable)
    );
    assert_eq!(events[n - 1], RtEvent::Closed);
}

#[tokio::test]
async fn a_rejected_handshake_maps_to_the_contract_codes() {
    for (status, body, code) in [
        (401, "{}", VoiceErrorCode::InvalidApiKey),
        (402, "", VoiceErrorCode::InsufficientCredit),
        (429, "", VoiceErrorCode::RateLimited),
        (503, "", VoiceErrorCode::GnaniUpstream),
    ] {
        let server = serve(Script {
            reject: Some((status, body)),
            ..Script::default()
        })
        .await;
        let (c, _) = client(&server.url, Some("k"));
        let err = c.open(RtConfig::default()).await.unwrap_err();
        assert_eq!(err.code, code, "HTTP {status}");
    }
}

#[tokio::test]
async fn a_missing_key_fails_before_connecting() {
    let server = serve(Script::default()).await;
    for key in [None, Some(" ")] {
        let (c, _) = client(&server.url, key);
        let err = c.open(RtConfig::default()).await.unwrap_err();
        assert_eq!(err.code, VoiceErrorCode::MissingApiKey);
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(server.conns().is_empty());
}

#[tokio::test]
async fn an_unreachable_server_is_network_offline() {
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let (c, _) = client(&format!("ws://127.0.0.1:{port}"), Some("k"));
    let err = c.open(RtConfig::default()).await.unwrap_err();
    assert_eq!(err.code, VoiceErrorCode::NetworkOffline);
}

#[tokio::test]
async fn a_server_error_message_is_reported_without_its_text_and_the_session_continues() {
    let server = serve(Script {
        error_after_connect: true,
        transcripts: vec!["ok".into()],
        ..Script::default()
    })
    .await;
    let (c, _) = client(&server.url, Some("k"));
    let mut s = c.open(RtConfig::default()).await.unwrap();
    let RtEvent::Error(e) = until(&mut s, |e| matches!(e, RtEvent::Error(_))).await else {
        unreachable!()
    };
    assert_eq!(e.code, VoiceErrorCode::GnaniUpstream);
    assert!(!e.message.contains("secret"));
    s.input.send(frame()).await.unwrap();
    s.input.send(RtInput::Commit).await.unwrap();
    assert!(matches!(
        until(&mut s, |e| matches!(e, RtEvent::Transcript(_))).await,
        RtEvent::Transcript(_)
    ));
    let _ = s.input.send(RtInput::Close).await;
}

#[tokio::test]
async fn refused_settings_end_the_session_once_instead_of_reconnecting_forever() {
    let server = serve(Script {
        refuse_settings: true,
        ..Script::default()
    })
    .await;
    let (c, _) = client(&server.url, Some("k"));
    let mut s = c
        .open(RtConfig {
            language_code: "hi-IN,en-IN".into(),
            ..RtConfig::default()
        })
        .await
        .unwrap();
    let mut seen = Vec::new();
    while let Ok(Some(e)) = tokio::time::timeout(Duration::from_secs(5), s.events.recv()).await {
        let closed = matches!(e, RtEvent::Closed);
        seen.push(e);
        if closed {
            break;
        }
    }
    let errors: Vec<_> = seen
        .iter()
        .filter_map(|e| match e {
            RtEvent::Error(e) => Some(e.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1, "{seen:?}");
    assert_eq!(errors[0].code, VoiceErrorCode::GnaniBadRequest);
    assert!(!errors[0].retryable);
    assert!(!errors[0].message.contains("invalid"), "Gnani's text is never copied");
    assert!(!seen.iter().any(|e| matches!(e, RtEvent::Reconnecting { .. })));
    assert!(matches!(seen.last(), Some(RtEvent::Closed)));
    assert_eq!(server.conns().len(), 1, "no reconnect");
}

#[tokio::test]
async fn dropping_the_input_sender_ends_the_session() {
    let server = serve(Script::default()).await;
    let (c, _) = client(&server.url, Some("k"));
    let RtSession {
        input, mut events, ..
    } = c.open(RtConfig::default()).await.unwrap();
    drop(input);
    let mut last = None;
    while let Ok(Some(e)) = tokio::time::timeout(Duration::from_secs(5), events.recv()).await {
        last = Some(e);
    }
    assert_eq!(last, Some(RtEvent::Closed));
}

#[test]
fn ascii_json_escapes_everything_outside_ascii() {
    assert_eq!(
        ascii_json(&["DBMS".into(), "login fix".into()]),
        r#"["DBMS","login fix"]"#
    );
    assert_eq!(
        ascii_json(&["கணக்கு".into()]),
        r#"["\u0b95\u0ba3\u0b95\u0bcd\u0b95\u0bc1"]"#
    );
    assert_eq!(
        ascii_json(&["🙂".into()]),
        r#"["\ud83d\ude42"]"#,
        "surrogate pair"
    );
    assert!(ascii_json(&["ಕನ್ನಡ".into()]).is_ascii());
}

#[test]
fn latency_is_milliseconds_as_gnani_sends_it() {
    assert_eq!(latency_ms(74.0), 74);
    assert_eq!(latency_ms(25.4), 25);
    assert_eq!(latency_ms(-3.0), 0);
    assert_eq!(latency_ms(f64::NAN), 0);
}

#[test]
fn the_production_policy_rotates_at_14_minutes_and_buffers_10_seconds() {
    let p = RealtimePolicy::default();
    assert_eq!(p.rotate_after, Duration::from_secs(840));
    assert_eq!(p.buffer_frames * 32, 10_016);
    assert_eq!(p.reconnect_delays.len(), 4);
}
