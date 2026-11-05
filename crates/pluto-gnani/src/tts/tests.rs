//! Contract tests for the Timbre client against a fake Timbre WebSocket server and `wiremock`.
//! No Gnani key or network is needed.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine as _;
use futures_util::{SinkExt, StreamExt};
use pluto_contracts::error::VoiceErrorCode;
use pluto_contracts::speech::{AudioContainer, TextToSpeech, TtsRequest};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http;
use tokio_tungstenite::tungstenite::Message;
use wiremock::matchers::{body_partial_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::http::{OutboundLog, OutboundRecord, RetryPolicy, StaticKey};

// ---- fake Timbre WebSocket ---------------------------------------------------------------

#[derive(Clone, Default)]
enum Ending {
    /// The last audio message carries `is_final: true`.
    #[default]
    FinalFlag,
    /// A separate `complete` message.
    Complete,
    /// An `error` message instead of audio.
    Error,
    /// The socket just closes mid-stream.
    Drop,
}

#[derive(Clone, Default)]
struct Script {
    /// PCM chunks sent for every request.
    chunks: Vec<Vec<u8>>,
    ending: Ending,
    /// Delay between audio messages.
    gap: Duration,
    reject: Option<u16>,
}

#[derive(Default)]
struct Seen {
    keys: Vec<String>,
    requests: Vec<Value>,
    /// Audio messages actually sent, over all connections.
    sent: usize,
}

async fn serve(script: Script) -> (String, Arc<Mutex<Seen>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let seen: Arc<Mutex<Seen>> = Arc::default();
    let state = seen.clone();
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            let (script, state) = (script.clone(), state.clone());
            tokio::spawn(async move {
                let keys = state.clone();
                let reject = script.reject;
                #[allow(clippy::result_large_err)] // tungstenite's callback type
                let callback =
                    move |req: &Request, resp: Response| -> Result<Response, ErrorResponse> {
                        let key = req
                            .headers()
                            .get("x-api-key-id")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("")
                            .to_string();
                        keys.lock().unwrap().keys.push(key);
                        match reject {
                            Some(status) => {
                                Err(http::Response::builder().status(status).body(None).unwrap())
                            }
                            None => Ok(resp),
                        }
                    };
                let Ok(mut ws) = tokio_tungstenite::accept_hdr_async(tcp, callback).await else {
                    return;
                };
                let Some(Ok(Message::Text(body))) = ws.next().await else {
                    return;
                };
                state
                    .lock()
                    .unwrap()
                    .requests
                    .push(serde_json::from_str(body.as_str()).unwrap());
                let _ = ws.send(Message::Text(r#"{"type":"start"}"#.into())).await;
                if matches!(script.ending, Ending::Error) {
                    let _ = ws
                        .send(Message::Text(
                            r#"{"type":"error","message":"quota detail"}"#.into(),
                        ))
                        .await;
                    return;
                }
                let n = script.chunks.len();
                for (i, chunk) in script.chunks.iter().enumerate() {
                    if !script.gap.is_zero() {
                        tokio::time::sleep(script.gap).await;
                    }
                    let is_final = matches!(script.ending, Ending::FinalFlag) && i + 1 == n;
                    let msg = json!({"type": "audio", "data": {
                        "chunk_index": i,
                        "audio": base64::engine::general_purpose::STANDARD.encode(chunk),
                        "is_final": is_final,
                    }});
                    if ws
                        .send(Message::Text(msg.to_string().into()))
                        .await
                        .is_err()
                    {
                        return; // the client went away (barge-in)
                    }
                    state.lock().unwrap().sent += 1;
                }
                match script.ending {
                    Ending::Complete => {
                        let _ = ws
                            .send(Message::Text(r#"{"type":"complete"}"#.into()))
                            .await;
                    }
                    Ending::Drop => return,
                    _ => {}
                }
                // Wait for the client's close.
                while let Some(Ok(m)) = ws.next().await {
                    if matches!(m, Message::Close(_)) {
                        break;
                    }
                }
            });
        }
    });
    (url, seen)
}

// ---- helpers ---------------------------------------------------------------------------------

#[derive(Default)]
struct MemoryLog(Mutex<Vec<OutboundRecord>>);
impl OutboundLog for MemoryLog {
    fn record(&self, r: OutboundRecord) {
        self.0.lock().unwrap().push(r);
    }
}

fn client(ws_url: &str, http_base: &str, key: Option<&str>) -> (TimbreClient, Arc<MemoryLog>) {
    let log = Arc::new(MemoryLog::default());
    let keys: Arc<dyn ApiKeySource> = Arc::new(StaticKey(key.map(String::from)));
    let http = GnaniHttp::new(http_base, keys.clone(), log.clone())
        .unwrap()
        .with_policy(RetryPolicy {
            delays: vec![Duration::from_millis(10); 3],
            max_retry_after: Duration::from_secs(1),
        });
    let c = TimbreClient::new(ws_url, Arc::new(http), keys, log.clone())
        .with_timeouts(Duration::from_secs(2), Duration::from_secs(2));
    (c, log)
}

fn req(text: &str) -> TtsRequest {
    TtsRequest {
        text: text.into(),
        language: "hi-IN".into(),
        voice: None,
        model: None,
        speed: None,
    }
}

/// Runs `stream` and collects what reached the sink.
async fn speak(c: &TimbreClient, r: TtsRequest) -> (VoiceResult<()>, Vec<Vec<u8>>) {
    let (tx, mut rx) = mpsc::channel(64);
    let result = c.stream(r, tx).await;
    let mut got = Vec::new();
    while let Ok(chunk) = rx.try_recv() {
        got.push(chunk);
    }
    (result, got)
}

fn pcm(n: usize, v: u8) -> Vec<u8> {
    vec![v; n]
}

// ---- stream ----------------------------------------------------------------------------------

#[tokio::test]
async fn streams_decoded_pcm_chunks_in_order_and_sends_the_documented_body() {
    let (url, seen) = serve(Script {
        chunks: vec![pcm(4800, 1), pcm(4800, 2), pcm(9, 3)],
        ..Script::default()
    })
    .await;
    let (c, log) = client(&url, "http://unused", Some("k-1"));
    let (result, got) = speak(
        &c,
        TtsRequest {
            speed: Some(2.0),
            ..req("Ho gaya.")
        },
    )
    .await;
    result.unwrap();
    assert_eq!(
        got,
        vec![pcm(4800, 1), pcm(4800, 2), pcm(9, 3)],
        "odd-length chunks pass through; the player carries the byte"
    );
    let s = seen.lock().unwrap();
    assert_eq!(s.keys, ["k-1"]);
    let body = &s.requests[0];
    assert_eq!(body["text"], "Ho gaya.");
    assert_eq!(body["language"], "hi-IN");
    assert_eq!(body["voice"], DEFAULT_VOICE);
    assert_eq!(body["model"], "timbre-v3.0");
    assert_eq!(body["speed"], 1.15, "clamped to Gnani's range");
    assert_eq!(
        body["audio_config"],
        json!({"sample_rate": 24000, "num_channels": 1, "sample_width": 2, "encoding": "linear_pcm", "container": "raw"})
    );
    let records = log.0.lock().unwrap().clone();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].destination, Destination::GnaniTts);
    assert_eq!(records[0].bytes, "Ho gaya.".len() as u64);
}

#[tokio::test]
async fn a_separate_complete_message_also_ends_the_stream() {
    let (url, _) = serve(Script {
        chunks: vec![pcm(10, 1)],
        ending: Ending::Complete,
        ..Script::default()
    })
    .await;
    let (c, _) = client(&url, "http://unused", Some("k"));
    let (result, got) = speak(&c, req("ok")).await;
    result.unwrap();
    assert_eq!(got.len(), 1);
}

#[tokio::test]
async fn voice_and_model_follow_the_request_then_the_client_defaults() {
    let (url, seen) = serve(Script {
        chunks: vec![pcm(2, 0)],
        ..Script::default()
    })
    .await;
    let (c, _) = client(&url, "http://unused", Some("k"));
    let c = c.with_defaults("Arya", "timbre-v3.0");
    speak(&c, req("a")).await.0.unwrap();
    speak(
        &c,
        TtsRequest {
            voice: Some("Shivani".into()),
            language: "".into(),
            ..req("b")
        },
    )
    .await
    .0
    .unwrap();
    let s = seen.lock().unwrap();
    assert_eq!(s.requests[0]["voice"], "Arya");
    assert_eq!(s.requests[1]["voice"], "Shivani");
    assert_eq!(
        s.requests[1]["language"], "auto",
        "an empty language lets Timbre detect it"
    );
}

#[tokio::test]
async fn long_text_is_spoken_piece_by_piece_at_sentence_ends() {
    let (url, seen) = serve(Script {
        chunks: vec![pcm(2, 7)],
        ..Script::default()
    })
    .await;
    let (c, log) = client(&url, "http://unused", Some("k"));
    let sentence = format!("{}। ", "क".repeat(299));
    let text = sentence.repeat(3); // 3 × 301 characters
    let (result, got) = speak(&c, req(&text)).await;
    result.unwrap();
    let s = seen.lock().unwrap();
    assert_eq!(s.requests.len(), 3, "one request per piece");
    for r in &s.requests {
        let t = r["text"].as_str().unwrap();
        assert!(t.chars().count() <= MAX_CHARS);
        assert!(t.ends_with('।'), "split at the sentence end: {t}");
    }
    assert_eq!(got.len(), 3);
    assert_eq!(log.0.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn closing_the_sink_stops_the_stream_without_an_error() {
    let (url, seen) = serve(Script {
        chunks: (0..50).map(|i| pcm(100, i)).collect(),
        gap: Duration::from_millis(20),
        ..Script::default()
    })
    .await;
    let (c, _) = client(&url, "http://unused", Some("k"));
    let (tx, mut rx) = mpsc::channel(1);
    let listener = tokio::spawn(async move {
        let first = rx.recv().await;
        drop(rx); // barge-in after the first chunk
        first
    });
    c.stream(req("a long answer"), tx).await.unwrap();
    assert!(listener.await.unwrap().is_some());
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        seen.lock().unwrap().sent < 50,
        "the server stopped being read"
    );
}

#[tokio::test]
async fn a_server_error_or_an_early_close_is_an_error_without_gnanis_text() {
    for (ending, code) in [
        (Ending::Error, VoiceErrorCode::GnaniUpstream),
        (Ending::Drop, VoiceErrorCode::GnaniUpstream),
    ] {
        let (url, _) = serve(Script {
            chunks: vec![pcm(2, 1)],
            ending,
            ..Script::default()
        })
        .await;
        let (c, _) = client(&url, "http://unused", Some("k"));
        let err = speak(&c, req("x")).await.0.unwrap_err();
        assert_eq!(err.code, code);
        assert!(!err.message.contains("quota"));
    }
}

#[tokio::test]
async fn a_refused_handshake_maps_to_the_contract_codes() {
    for (status, code) in [
        (401, VoiceErrorCode::InvalidApiKey),
        (402, VoiceErrorCode::InsufficientCredit),
        (503, VoiceErrorCode::GnaniUpstream),
    ] {
        let (url, _) = serve(Script {
            reject: Some(status),
            ..Script::default()
        })
        .await;
        let (c, _) = client(&url, "http://unused", Some("k"));
        assert_eq!(
            speak(&c, req("x")).await.0.unwrap_err().code,
            code,
            "HTTP {status}"
        );
    }
}

#[tokio::test]
async fn no_key_no_text_or_no_server_fail_cleanly() {
    let (url, seen) = serve(Script::default()).await;
    let (c, _) = client(&url, "http://unused", None);
    assert_eq!(
        speak(&c, req("x")).await.0.unwrap_err().code,
        VoiceErrorCode::MissingApiKey
    );
    let (c, _) = client(&url, "http://unused", Some("k"));
    assert_eq!(
        speak(&c, req("  [unknowntag]  ")).await.0.unwrap_err().code,
        VoiceErrorCode::GnaniBadRequest
    );
    assert!(seen.lock().unwrap().keys.is_empty(), "nothing was sent");
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let (c, _) = client(
        &format!("ws://127.0.0.1:{port}"),
        "http://unused",
        Some("k"),
    );
    assert_eq!(
        speak(&c, req("x")).await.0.unwrap_err().code,
        VoiceErrorCode::NetworkOffline
    );
}

// ---- synthesize (REST) -----------------------------------------------------------------------

#[tokio::test]
async fn synthesize_posts_the_documented_body_and_returns_the_file() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(INFERENCE_PATH))
        .and(header("X-API-Key-ID", "k"))
        .and(body_partial_json(json!({
            "text": "Recap: 3 tasks.", "model": "timbre-v3.0", "language": "en-IN",
            "audio_config": {"container": "mp3", "bitrate": "128k", "sample_rate": 24000}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"ID3fake-mp3".to_vec()))
        .expect(1)
        .mount(&server)
        .await;
    let (c, log) = client("ws://unused", &server.uri(), Some("k"));
    let bytes = c
        .synthesize(
            TtsRequest {
                language: "en-IN".into(),
                ..req("Recap: 3 tasks.")
            },
            AudioContainer::Mp3,
        )
        .await
        .unwrap();
    assert_eq!(bytes, b"ID3fake-mp3");
    assert_eq!(log.0.lock().unwrap()[0].destination, Destination::GnaniTts);
}

#[tokio::test]
async fn a_long_wav_recap_is_one_valid_file() {
    let server = MockServer::start().await;
    let piece = wav_file(&[1, 0, 2, 0], 24_000);
    Mock::given(method("POST"))
        .and(path(INFERENCE_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(piece))
        .expect(2)
        .mount(&server)
        .await;
    let (c, _) = client("ws://unused", &server.uri(), Some("k"));
    let text = format!("{}. {}.", "a".repeat(400), "b".repeat(400));
    let file = c.synthesize(req(&text), AudioContainer::Wav).await.unwrap();
    assert_eq!(&file[0..4], b"RIFF");
    assert_eq!(
        wav_data(&file),
        &[1, 0, 2, 0, 1, 0, 2, 0],
        "both pieces' samples, one header"
    );
    assert_eq!(
        u32::from_le_bytes([file[4], file[5], file[6], file[7]]) as usize,
        file.len() - 8
    );
}

#[tokio::test]
async fn synthesize_errors_come_from_the_http_base() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(402))
        .mount(&server)
        .await;
    let (c, _) = client("ws://unused", &server.uri(), Some("k"));
    let err = c
        .synthesize(req("x"), AudioContainer::Mp3)
        .await
        .unwrap_err();
    assert_eq!(err.code, VoiceErrorCode::InsufficientCredit);
}

// ---- text preparation ------------------------------------------------------------------------

#[test]
fn only_allow_listed_audio_tags_survive() {
    assert_eq!(
        sanitize_tags("Ho gaya [laughs] [LAUGHS]"),
        "Ho gaya [laughs] [laughs]"
    );
    assert_eq!(sanitize_tags("Hello [shouts] there"), "Hello there");
    assert_eq!(sanitize_tags("[whisper] secret"), "[whisper] secret");
    assert_eq!(
        sanitize_tags("Section [3] stays, [a.b] stays"),
        "Section [3] stays, [a.b] stays"
    );
    assert_eq!(sanitize_tags("unclosed [laughs"), "unclosed [laughs");
}

#[test]
fn splitting_prefers_sentence_ends_then_spaces_and_counts_characters() {
    assert_eq!(split_for_tts("Short.", 500), ["Short."]);
    assert_eq!(
        split_for_tts("One. Two. Three.", 10),
        ["One. Two.", "Three."]
    );
    assert_eq!(split_for_tts("aaaa bbbb cccc", 10), ["aaaa bbbb", "cccc"]);
    assert_eq!(
        split_for_tts("abcdefghij", 4),
        ["abcd", "efgh", "ij"],
        "no space: hard split"
    );
    let hindi = "काम हो गया। ".repeat(60);
    for piece in split_for_tts(&hindi, MAX_CHARS) {
        assert!(piece.chars().count() <= MAX_CHARS);
        assert!(piece.ends_with('।'));
    }
    assert!(split_for_tts("   ", 500).is_empty());
}

#[test]
fn wav_data_finds_the_samples_and_tolerates_non_wav() {
    let file = wav_file(&[9, 9], 24_000);
    assert_eq!(wav_data(&file), &[9, 9]);
    assert_eq!(wav_data(b"not a wav"), b"not a wav");
}
