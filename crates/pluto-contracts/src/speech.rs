//! Audio and speech types; the `SpeechToText`, `TextToSpeech` and `FrameSink` traits
//! (API_CONTRACTS §0 "Audio formats", §2.1, §2.4, §3.7).
//!
//! Owner: **D1** (T-004). Changes go through a `contract` PR that D2 approves.
//!
//! These types stay in Rust: the UI sees speech only through `voice://` events and the `playback`
//! Channel, so nothing here is exported to TypeScript.

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::error::{VoiceError, VoiceResult};
use crate::voice::SttFormat;

// ---- audio formats (never change without a contract PR) ------------------------------------

/// Mic → Rust: PCM16 little-endian, mono, 16 kHz.
pub const CAPTURE_SAMPLE_RATE: u32 = 16_000;
/// Every frame from `audio_push` is exactly this many bytes (512 samples, Gnani's recommended size).
pub const FRAME_BYTES: usize = 1_024;
/// Rust → speaker: PCM16 little-endian, mono, 24 kHz (Timbre `linear_pcm`).
pub const PLAYBACK_SAMPLE_RATE: u32 = 24_000;

/// One frame from the UI's capture worklet, as received by `audio_push`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioFrame {
    /// The `x-session-id` header: the push-to-talk or recording session.
    pub session_id: String,
    /// 0-based order within the session, assigned by Rust on arrival.
    pub seq: u64,
    /// Exactly [`FRAME_BYTES`] bytes of PCM16 LE mono 16 kHz.
    pub pcm: Vec<u8>,
}

/// Receives every frame `audio_push` accepts (the WAV writer, the Realtime sender, the recorder).
/// Called on the command thread about 31 times a second, so `push` must not block: hand the frame
/// to a channel or buffer. Implemented in `pluto-voice` (WAV writer) and
/// `pluto-intel::meeting::recorder`.
pub trait FrameSink: Send + Sync {
    /// Short name for logs, e.g. `"wav"`, `"realtime"`.
    fn name(&self) -> &'static str;

    /// One frame. An error is logged and reported once; the other sinks still get the frame.
    fn push(&self, frame: &AudioFrame) -> VoiceResult<()>;

    /// The session ended (key released, recording stopped or cancelled). Flush and close.
    fn end(&self, _session_id: &str) -> VoiceResult<()> {
        Ok(())
    }
}

// ---- speech to text (Prisma Realtime) ------------------------------------------------------

/// Handshake settings for one Prisma Realtime session (API_CONTRACTS §2.1). The model and the
/// 16 kHz sample rate are fixed by the client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtConfig {
    /// `indic-auto`, or 1–4 comma-separated codes such as `hi-IN,en-IN`.
    pub language_code: String,
    pub format: SttFormat,
    /// Recognition hints (≤ 200). Non-ASCII terms are JSON-escaped by the client.
    pub keyterms: Vec<String>,
    /// `x-idle-timeout-sec`, 5–900. `None` keeps Gnani's default (60 s).
    pub idle_timeout_sec: Option<u16>,
    /// `x-min-silence-ms`. `None` keeps Gnani's default.
    pub min_silence_ms: Option<u32>,
}

impl Default for RtConfig {
    fn default() -> Self {
        Self {
            language_code: "indic-auto".into(),
            format: SttFormat::Codemix,
            keyterms: Vec::new(),
            idle_timeout_sec: None,
            min_silence_ms: None,
        }
    }
}

/// What the caller sends into an open session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RtInput {
    /// One [`FRAME_BYTES`] PCM frame.
    Frame(Vec<u8>),
    /// Force the current segment to close (`{"type":"commit"}`), sent on hotkey release.
    Commit,
    /// Close the socket normally.
    Close,
}

/// One final transcript from Prisma Realtime (there are no partials). Times are converted from
/// Gnani's seconds to milliseconds by the client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtTranscript {
    pub text: String,
    /// Gnani's `segment_index` within this session.
    pub segment_index: u32,
    /// `detected_language_code`; `None` = unknown.
    pub language: Option<String>,
    pub audio_duration_ms: Option<u32>,
    /// Gnani's reported `latency`, in ms.
    pub latency_ms: Option<u32>,
}

/// What an open session reports, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum RtEvent {
    Connected {
        request_id: Option<String>,
    },
    SpeechStart,
    Processing,
    Transcript(RtTranscript),
    /// The socket dropped or hit the 14-minute rotation; audio is being buffered (up to 10 s).
    Reconnecting {
        attempt: u32,
    },
    /// A failure. Fatal ones are followed by `Closed`.
    Error(VoiceError),
    /// The session is over; no more events follow.
    Closed,
}

/// An open Prisma Realtime session. Dropping `input` closes it; `events` ends with `Closed`.
#[derive(Debug)]
pub struct RtSession {
    /// Our id for logs and the privacy log.
    pub id: String,
    pub input: mpsc::Sender<RtInput>,
    pub events: mpsc::Receiver<RtEvent>,
}

/// Opens Prisma Realtime sessions. Implemented by `pluto-gnani::realtime` (`MockStt` in
/// `pluto-gnani::mock` for tests); used by `pluto-voice::runtime` and the recorder's captions.
#[async_trait]
pub trait SpeechToText: Send + Sync {
    async fn open(&self, config: RtConfig) -> VoiceResult<RtSession>;
}

// ---- text to speech (Timbre) ---------------------------------------------------------------

/// One synthesis request (API_CONTRACTS §2.4). `None` fields use the voice settings.
#[derive(Debug, Clone, PartialEq)]
pub struct TtsRequest {
    /// Up to 500 characters per Timbre call; the client splits longer text at sentence ends.
    pub text: String,
    /// `hi-IN`, `en-IN`, …, `hi-en` or `auto`. Follows the detected language.
    pub language: String,
    pub voice: Option<String>,
    pub model: Option<String>,
    /// 0.85–1.15.
    pub speed: Option<f32>,
}

/// File formats for [`TextToSpeech::synthesize`]. Streaming is always raw PCM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioContainer {
    Mp3,
    Wav,
}

/// Where streamed PCM16 24 kHz chunks go. `src-tauri` forwards them to the `playback` Channel.
/// A closed receiver means "stop" (barge-in): the stream ends early without an error.
pub type PcmSink = mpsc::Sender<Vec<u8>>;

/// Speaks text. Implemented by `pluto-gnani::tts` (`MockTts` in `pluto-gnani::mock`); used by
/// `pluto-voice::assistant` and `pluto-intel::meeting` (recap).
#[async_trait]
pub trait TextToSpeech: Send + Sync {
    /// Streams PCM16 24 kHz mono chunks into `out` as they arrive; returns when the last chunk is
    /// sent or `out` is closed.
    async fn stream(&self, request: TtsRequest, out: PcmSink) -> VoiceResult<()>;

    /// Synthesizes a whole file (recap export).
    async fn synthesize(
        &self,
        request: TtsRequest,
        container: AudioContainer,
    ) -> VoiceResult<Vec<u8>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_is_512_samples_and_32_ms() {
        assert_eq!(FRAME_BYTES / 2, 512);
        assert_eq!(FRAME_BYTES as u32 * 1000 / (CAPTURE_SAMPLE_RATE * 2), 32);
    }

    #[test]
    fn realtime_defaults_are_auto_language_and_codemix() {
        let c = RtConfig::default();
        assert_eq!(c.language_code, "indic-auto");
        assert_eq!(c.format, SttFormat::Codemix);
        assert!(c.keyterms.is_empty());
    }
}
