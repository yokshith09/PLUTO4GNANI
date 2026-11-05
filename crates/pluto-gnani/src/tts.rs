//! Timbre client (API_CONTRACTS §2.4). Owner: **D1** (T-102). Implements `TextToSpeech`; used by
//! the assistant's spoken replies (T-303), `voice_speak`, and D2's recap (T-205).
//!
//! - **`stream`** speaks over the WebSocket (`wss://api.vachana.ai/api/v1/tts`) with
//!   `container: "raw"`: Gnani sends `start`, then `audio` messages with base64 PCM16 24 kHz mono
//!   chunks, then `complete` (or `error`). Each decoded chunk goes to the [`PcmSink`] as it
//!   arrives, so playback starts before the sentence is finished. A closed sink is barge-in: the
//!   socket is closed and `stream` returns `Ok`.
//! - **`synthesize`** makes a whole file over REST (`POST /api/v1/tts/inference`) through
//!   [`GnaniHttp`] (one request at a time, retries, error mapping): mp3 for the recap export.
//! - Text longer than [`MAX_CHARS`] is split at sentence ends (`.` `!` `?` `।` `॥`, newlines) and
//!   spoken piece by piece, one connection per piece. Characters are counted, not bytes.
//! - Only the v3.0 audio tags in [`AUDIO_TAGS`] reach Gnani; any other `[tag]` is removed (tags
//!   are billable characters, and an unknown one would be read aloud).
//! - Every request is written to the privacy log (`gnani_tts`, the text's size in bytes).

use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use futures_util::{SinkExt, StreamExt};
use pluto_contracts::error::{VoiceError, VoiceErrorCode, VoiceResult};
use pluto_contracts::speech::{
    AudioContainer, PcmSink, TextToSpeech, TtsRequest, PLAYBACK_SAMPLE_RATE,
};
use pluto_contracts::voice::Destination;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};

use crate::http::{
    map_status, ApiKeySource, Call, Family, GnaniHttp, OutboundLog, OutboundRecord, API_KEY_HEADER,
};

/// Timbre streaming endpoint.
pub const DEFAULT_WS_URL: &str = "wss://api.vachana.ai/api/v1/tts";
/// Timbre REST path (under the HTTP base URL).
pub const INFERENCE_PATH: &str = "/api/v1/tts/inference";
/// Gnani's documented model and an example voice (API_CONTRACTS §2.4).
pub const DEFAULT_MODEL: &str = "timbre-v3.0";
pub const DEFAULT_VOICE: &str = "Shivani";
/// Timbre v3.0's voices (docs.gnani.ai, Available Voices). Any other name is refused with 400.
pub const VOICES_V3: [&str; 13] = [
    "Shivani", "Lakshmi", "Kalpana", "Chetana", "Vasudha", "Riyaa", "Tarika", "Avinash", "Shubham",
    "Prakash", "Devansh", "Yashraj", "Harshad",
];
/// The most characters sent in one Timbre request.
pub const MAX_CHARS: usize = 500;
/// The v3.0 audio-tag allow-list.
pub const AUDIO_TAGS: [&str; 7] = [
    "laughs", "giggles", "sigh", "exhale", "cheer", "whisper", "silence",
];
/// What the privacy log says these requests were for.
pub const PURPOSE: &str = "spoken reply";

// ---- text preparation -----------------------------------------------------------------------

/// Removes every `[tag]` that is not in [`AUDIO_TAGS`] (case-insensitive) and tidies the spaces it
/// leaves. Brackets that don't look like a tag (digits, punctuation, over 20 characters) stay.
pub fn sanitize_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find(']') {
            Some(close) => {
                let inner = &after[..close];
                let looks_like_tag = !inner.is_empty()
                    && inner.chars().count() <= 20
                    && inner
                        .chars()
                        .all(|c| c.is_ascii_alphabetic() || c == ' ' || c == '_');
                if !looks_like_tag {
                    out.push('[');
                    out.push_str(inner);
                    out.push(']');
                } else if AUDIO_TAGS.contains(&inner.trim().to_ascii_lowercase().as_str()) {
                    out.push('[');
                    out.push_str(&inner.trim().to_ascii_lowercase());
                    out.push(']');
                }
                rest = &after[close + 1..];
            }
            None => {
                out.push('[');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    // Collapse the double spaces a removed tag leaves behind.
    let mut tidy = String::with_capacity(out.len());
    let mut prev_space = false;
    for c in out.chars() {
        if c == ' ' {
            if !prev_space {
                tidy.push(c);
            }
            prev_space = true;
        } else {
            tidy.push(c);
            prev_space = false;
        }
    }
    tidy.trim().to_string()
}

fn is_sentence_end(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | '।' | '॥' | '\n')
}

/// Splits `text` into pieces of at most `max` characters, at sentence ends where possible, else
/// at the last space, else hard. Pieces are trimmed; empty ones are dropped.
pub fn split_for_tts(text: &str, max: usize) -> Vec<String> {
    let max = max.max(1);
    let chars: Vec<char> = text.chars().collect();
    let mut pieces = Vec::new();
    let mut start = 0usize;
    while start < chars.len() {
        if chars.len() - start <= max {
            pieces.push(chars[start..].iter().collect::<String>());
            break;
        }
        let window = &chars[start..start + max];
        let cut = window
            .iter()
            .rposition(|&c| is_sentence_end(c))
            .or_else(|| window.iter().rposition(|&c| c == ' '))
            .map_or(max, |i| i + 1);
        pieces.push(chars[start..start + cut].iter().collect::<String>());
        start += cut;
    }
    pieces
        .into_iter()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

// ---- client ---------------------------------------------------------------------------------

/// The Timbre client. Cheap to share (`Arc`).
pub struct TimbreClient {
    ws_url: String,
    http: Arc<GnaniHttp>,
    keys: Arc<dyn ApiKeySource>,
    log: Arc<dyn OutboundLog>,
    default_voice: String,
    default_model: String,
    connect_timeout: Duration,
    /// Longest silence between two messages before the stream is given up.
    idle_timeout: Duration,
}

impl TimbreClient {
    /// `http` serves `synthesize` (its base URL is the REST host); `ws_url` serves `stream`.
    pub fn new(
        ws_url: impl Into<String>,
        http: Arc<GnaniHttp>,
        keys: Arc<dyn ApiKeySource>,
        log: Arc<dyn OutboundLog>,
    ) -> Self {
        Self {
            ws_url: ws_url.into(),
            http,
            keys,
            log,
            default_voice: DEFAULT_VOICE.into(),
            default_model: DEFAULT_MODEL.into(),
            connect_timeout: Duration::from_secs(10),
            idle_timeout: Duration::from_secs(20),
        }
    }

    /// The voice and model used when a request leaves them unset (from the voice settings).
    pub fn with_defaults(mut self, voice: impl Into<String>, model: impl Into<String>) -> Self {
        self.default_voice = voice.into();
        self.default_model = model.into();
        self
    }

    pub fn with_timeouts(mut self, connect: Duration, idle: Duration) -> Self {
        self.connect_timeout = connect;
        self.idle_timeout = idle;
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

    /// The request body for one piece of text (API_CONTRACTS §2.4).
    fn body(&self, request: &TtsRequest, text: &str, container: &str) -> Value {
        let mut audio_config = json!({
            "sample_rate": PLAYBACK_SAMPLE_RATE,
            "num_channels": 1,
            "sample_width": 2,
            "encoding": "linear_pcm",
            "container": container,
        });
        if container == "mp3" {
            audio_config["bitrate"] = json!("128k");
        }
        let language = match request.language.trim() {
            "" => "auto",
            l => l,
        };
        json!({
            "text": text,
            "voice": request.voice.clone().unwrap_or_else(|| self.default_voice.clone()),
            "model": request.model.clone().unwrap_or_else(|| self.default_model.clone()),
            "language": language,
            // In f64 and to 2 decimals, so Gnani gets 1.15, not 1.149999976.
            "speed": (f64::from(request.speed.unwrap_or(1.0)).clamp(0.85, 1.15) * 100.0).round() / 100.0,
            "audio_config": audio_config,
        })
    }

    fn prepare(request: &TtsRequest) -> VoiceResult<Vec<String>> {
        let pieces = split_for_tts(&sanitize_tags(&request.text), MAX_CHARS);
        if pieces.is_empty() {
            return Err(VoiceError::new(
                VoiceErrorCode::GnaniBadRequest,
                "There is nothing to say.",
            ));
        }
        Ok(pieces)
    }

    /// Speaks one piece. Returns `false` when the sink closed (barge-in).
    async fn stream_piece(&self, key: &str, body: &Value, out: &PcmSink) -> VoiceResult<bool> {
        let offline = || {
            VoiceError::new(
                VoiceErrorCode::NetworkOffline,
                "Can't reach Gnani. Check your internet connection.",
            )
        };
        let mut request = self.ws_url.as_str().into_client_request().map_err(|_| {
            VoiceError::new(
                VoiceErrorCode::Core,
                "The voice service address is invalid.",
            )
        })?;
        request.headers_mut().insert(
            API_KEY_HEADER,
            HeaderValue::from_str(key).map_err(|_| {
                VoiceError::new(
                    VoiceErrorCode::InvalidApiKey,
                    "The saved Gnani key contains characters Gnani can't accept.",
                )
            })?,
        );
        let mut ws = match timeout(
            self.connect_timeout,
            tokio_tungstenite::connect_async(request),
        )
        .await
        {
            Err(_)
            | Ok(Err(
                WsError::Io(_)
                | WsError::Tls(_)
                | WsError::ConnectionClosed
                | WsError::AlreadyClosed,
            )) => return Err(offline()),
            Ok(Err(WsError::Http(response))) => {
                let status = reqwest::StatusCode::from_u16(response.status().as_u16())
                    .unwrap_or(reqwest::StatusCode::BAD_GATEWAY);
                let body = response
                    .body()
                    .as_deref()
                    .map(String::from_utf8_lossy)
                    .unwrap_or_default();
                return Err(map_status(Family::Tts, status, &body));
            }
            Ok(Err(_)) => return Err(offline()),
            Ok(Ok((ws, _))) => ws,
        };
        let text = body["text"].as_str().unwrap_or_default();
        self.log.record(OutboundRecord {
            destination: Destination::GnaniTts,
            purpose: PURPOSE.into(),
            bytes: text.len() as u64,
            ref_id: None,
        });
        if ws
            .send(Message::Text(body.to_string().into()))
            .await
            .is_err()
        {
            return Err(offline());
        }
        loop {
            let next = match timeout(self.idle_timeout, ws.next()).await {
                Err(_) => {
                    let _ = ws.close(None).await;
                    return Err(VoiceError::new(
                        VoiceErrorCode::GnaniUpstream,
                        "Gnani's voice stopped answering. Try again.",
                    ));
                }
                Ok(next) => next,
            };
            let text = match next {
                Some(Ok(Message::Text(t))) => t,
                Some(Ok(Message::Close(_))) | None => {
                    return Err(VoiceError::new(
                        VoiceErrorCode::GnaniUpstream,
                        "Gnani's voice ended early. Try again.",
                    ));
                }
                Some(Ok(_)) => continue,
                // Connected, then cut off mid-reply: the reply ended early (retryable).
                Some(Err(_)) => {
                    return Err(VoiceError::new(
                        VoiceErrorCode::GnaniUpstream,
                        "Gnani's voice ended early. Try again.",
                    ));
                }
            };
            let Ok(msg) = serde_json::from_str::<ServerMessage>(text.as_str()) else {
                continue;
            };
            match msg.kind.as_str() {
                "audio" => {
                    let Some(data) = msg.data else { continue };
                    let pcm = base64::engine::general_purpose::STANDARD
                        .decode(data.audio.as_bytes())
                        .map_err(|_| {
                            VoiceError::new(
                                VoiceErrorCode::GnaniUpstream,
                                "Gnani sent audio Pluto couldn't read.",
                            )
                        })?;
                    if !pcm.is_empty() && out.send(pcm).await.is_err() {
                        let _ = ws.close(None).await;
                        return Ok(false); // barge-in
                    }
                    if data.is_final {
                        let _ = ws.close(None).await;
                        return Ok(true);
                    }
                }
                "complete" => {
                    let _ = ws.close(None).await;
                    return Ok(true);
                }
                "error" => {
                    let _ = ws.close(None).await;
                    return Err(VoiceError::new(
                        VoiceErrorCode::GnaniUpstream,
                        "Gnani couldn't speak that. Try again.",
                    ));
                }
                _ => {} // start and anything new
            }
        }
    }
}

/// The samples of a PCM WAV file (the `data` chunk); the whole input if it isn't a RIFF file.
fn wav_data(file: &[u8]) -> &[u8] {
    if file.len() < 12 || &file[0..4] != b"RIFF" || &file[8..12] != b"WAVE" {
        return file;
    }
    let mut at = 12;
    while at + 8 <= file.len() {
        let id = &file[at..at + 4];
        let len =
            u32::from_le_bytes([file[at + 4], file[at + 5], file[at + 6], file[at + 7]]) as usize;
        let start = at + 8;
        if id == b"data" {
            return &file[start..(start + len).min(file.len())];
        }
        at = start + len + (len % 2);
    }
    &[]
}

/// A 16-bit mono PCM WAV file around `pcm`.
fn wav_file(pcm: &[u8], rate: u32) -> Vec<u8> {
    let data_len = u32::try_from(pcm.len()).unwrap_or(u32::MAX);
    let mut out = Vec::with_capacity(44 + pcm.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend_from_slice(pcm);
    out
}

#[derive(Debug, Deserialize)]
struct ServerMessage {
    #[serde(rename = "type")]
    kind: String,
    data: Option<AudioData>,
}

#[derive(Debug, Deserialize)]
struct AudioData {
    #[serde(default)]
    audio: String,
    #[serde(default)]
    is_final: bool,
}

#[async_trait::async_trait]
impl TextToSpeech for TimbreClient {
    async fn stream(&self, request: TtsRequest, out: PcmSink) -> VoiceResult<()> {
        let key = self.key()?;
        for piece in Self::prepare(&request)? {
            if out.is_closed() {
                return Ok(());
            }
            let body = self.body(&request, &piece, "raw");
            if !self.stream_piece(&key, &body, &out).await? {
                return Ok(());
            }
        }
        Ok(())
    }

    async fn synthesize(
        &self,
        request: TtsRequest,
        container: AudioContainer,
    ) -> VoiceResult<Vec<u8>> {
        let pieces = Self::prepare(&request)?;
        let format = match container {
            AudioContainer::Mp3 => "mp3",
            AudioContainer::Wav => "wav",
        };
        let mut audio = Vec::new();
        let mut wav_pcm = Vec::new();
        for piece in pieces {
            let body = self.body(&request, &piece, format);
            let url = self.http.url(INFERENCE_PATH);
            let call = Call::new(Family::Tts, "recap file", piece.len() as u64);
            let resp = self
                .http
                .execute(call, |c| c.post(&url).json(&body))
                .await?;
            let bytes = resp.bytes().await.map_err(|_| {
                VoiceError::new(
                    VoiceErrorCode::NetworkOffline,
                    "The audio download was interrupted. Try again.",
                )
            })?;
            match container {
                // mp3 frames concatenate cleanly.
                AudioContainer::Mp3 => audio.extend_from_slice(&bytes),
                // WAV: keep each piece's samples and write one header for the whole file.
                AudioContainer::Wav => wav_pcm.extend_from_slice(wav_data(&bytes)),
            }
        }
        if container == AudioContainer::Wav {
            audio = wav_file(&wav_pcm, PLAYBACK_SAMPLE_RATE);
        }
        Ok(audio)
    }
}

#[cfg(test)]
mod tests;
