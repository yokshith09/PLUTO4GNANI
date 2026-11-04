//! `MockStt` and `MockTts`: test doubles for `SpeechToText` and `TextToSpeech` (feature `mock`).
//!
//! Owner: **D1** (T-004). Used by `pluto-voice` and `pluto-intel` tests so neither needs a Gnani
//! key. Behaviour follows INTEGRATION_CONTRACT §3: fixture transcripts with delays and errors
//! (including a simulated close code 1001), and a 1 s sine wave.

use std::f32::consts::PI;
use std::time::Duration;

use async_trait::async_trait;
use pluto_contracts::error::{VoiceError, VoiceResult};
use pluto_contracts::speech::{
    AudioContainer, PcmSink, RtConfig, RtEvent, RtInput, RtSession, RtTranscript, SpeechToText,
    TextToSpeech, TtsRequest, PLAYBACK_SAMPLE_RATE,
};
use tokio::sync::mpsc;

// ---- speech to text ------------------------------------------------------------------------

/// Scripted Prisma Realtime. Each `Commit` produces the next scripted transcript (after `delay`);
/// when the script runs out, a commit produces nothing (like silence).
#[derive(Debug, Clone, Default)]
pub struct MockStt {
    transcripts: Vec<String>,
    language: Option<String>,
    delay: Duration,
    open_error: Option<VoiceError>,
    reconnect_once: bool,
}

impl MockStt {
    /// One transcript per commit, in order.
    pub fn new<S: Into<String>>(transcripts: impl IntoIterator<Item = S>) -> Self {
        Self {
            transcripts: transcripts.into_iter().map(Into::into).collect(),
            ..Self::default()
        }
    }

    /// The `detected_language_code` reported with every transcript.
    pub fn language(mut self, code: &str) -> Self {
        self.language = Some(code.to_string());
        self
    }

    /// Wait this long between a commit and its transcript.
    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    /// `open` fails with this error.
    pub fn failing(mut self, error: VoiceError) -> Self {
        self.open_error = Some(error);
        self
    }

    /// Right after connecting, simulate close code 1001 and a reconnect.
    pub fn reconnect_once(mut self) -> Self {
        self.reconnect_once = true;
        self
    }
}

#[async_trait]
impl SpeechToText for MockStt {
    async fn open(&self, _config: RtConfig) -> VoiceResult<RtSession> {
        if let Some(e) = &self.open_error {
            return Err(e.clone());
        }
        let (input_tx, mut input_rx) = mpsc::channel::<RtInput>(64);
        let (event_tx, event_rx) = mpsc::channel::<RtEvent>(64);
        let script = self.clone();
        let id = pluto_core::ids::new_id();
        let request_id = format!("mock-{id}");
        tokio::spawn(async move {
            let send = |e: RtEvent| {
                let tx = event_tx.clone();
                async move { tx.send(e).await.is_ok() }
            };
            if !send(RtEvent::Connected {
                request_id: Some(request_id.clone()),
            })
            .await
            {
                return;
            }
            if script.reconnect_once {
                if !send(RtEvent::Reconnecting { attempt: 1 }).await {
                    return;
                }
                if !send(RtEvent::Connected {
                    request_id: Some(request_id),
                })
                .await
                {
                    return;
                }
            }
            let mut next = 0usize;
            let mut speaking = false;
            while let Some(input) = input_rx.recv().await {
                match input {
                    RtInput::Frame(_) => {
                        if !speaking {
                            speaking = true;
                            if !send(RtEvent::SpeechStart).await {
                                return;
                            }
                        }
                    }
                    RtInput::Commit => {
                        speaking = false;
                        let Some(text) = script.transcripts.get(next).cloned() else {
                            continue;
                        };
                        if !send(RtEvent::Processing).await {
                            return;
                        }
                        if !script.delay.is_zero() {
                            tokio::time::sleep(script.delay).await;
                        }
                        let event = RtEvent::Transcript(RtTranscript {
                            text,
                            segment_index: u32::try_from(next).unwrap_or(u32::MAX),
                            language: script.language.clone(),
                            audio_duration_ms: None,
                            latency_ms: Some(
                                u32::try_from(script.delay.as_millis()).unwrap_or(u32::MAX),
                            ),
                        });
                        next += 1;
                        if !send(event).await {
                            return;
                        }
                    }
                    RtInput::Close => break,
                }
            }
            let _ = send(RtEvent::Closed).await;
        });
        Ok(RtSession {
            id,
            input: input_tx,
            events: event_rx,
        })
    }
}

// ---- text to speech ------------------------------------------------------------------------

/// Samples per streamed chunk: 200 ms at 24 kHz.
const CHUNK_SAMPLES: usize = 4_800;

/// Speaks a 1 s 440 Hz sine wave (PCM16 24 kHz mono) for any text, in 200 ms chunks.
#[derive(Debug, Clone, Default)]
pub struct MockTts {
    error: Option<VoiceError>,
}

impl MockTts {
    pub fn new() -> Self {
        Self::default()
    }

    /// Every call fails with this error.
    pub fn failing(error: VoiceError) -> Self {
        Self { error: Some(error) }
    }
}

/// One second of a 440 Hz sine at 24 kHz, PCM16 little-endian.
pub fn sine_pcm() -> Vec<u8> {
    let n = PLAYBACK_SAMPLE_RATE as usize;
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        let t = i as f32 / PLAYBACK_SAMPLE_RATE as f32;
        // Quarter amplitude: audible, never clipping.
        let s = ((2.0 * PI * 440.0 * t).sin() * f32::from(i16::MAX) * 0.25) as i16;
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// A minimal RIFF/WAVE header + data for PCM16 mono at `rate`.
fn wav(pcm: &[u8], rate: u32) -> Vec<u8> {
    let data_len = u32::try_from(pcm.len()).unwrap_or(u32::MAX);
    let mut out = Vec::with_capacity(44 + pcm.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend_from_slice(pcm);
    out
}

#[async_trait]
impl TextToSpeech for MockTts {
    async fn stream(&self, _request: TtsRequest, out: PcmSink) -> VoiceResult<()> {
        if let Some(e) = &self.error {
            return Err(e.clone());
        }
        for chunk in sine_pcm().chunks(CHUNK_SAMPLES * 2) {
            if out.send(chunk.to_vec()).await.is_err() {
                // Receiver closed: barge-in. Not an error.
                return Ok(());
            }
        }
        Ok(())
    }

    async fn synthesize(
        &self,
        _request: TtsRequest,
        container: AudioContainer,
    ) -> VoiceResult<Vec<u8>> {
        if let Some(e) = &self.error {
            return Err(e.clone());
        }
        Ok(match container {
            AudioContainer::Wav => wav(&sine_pcm(), PLAYBACK_SAMPLE_RATE),
            // Not real mp3 data: an ID3 tag marks it for tests that only check the bytes arrive.
            AudioContainer::Mp3 => b"ID3mock".to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pluto_contracts::error::VoiceErrorCode;
    use pluto_contracts::speech::FRAME_BYTES;

    fn tts_request() -> TtsRequest {
        TtsRequest {
            text: "Ho gaya.".into(),
            language: "hi-IN".into(),
            voice: None,
            model: None,
            speed: None,
        }
    }

    async fn next_transcript(s: &mut RtSession) -> Option<RtTranscript> {
        while let Some(e) = s.events.recv().await {
            match e {
                RtEvent::Transcript(t) => return Some(t),
                RtEvent::Closed => return None,
                _ => {}
            }
        }
        None
    }

    #[tokio::test]
    async fn each_commit_gives_the_next_scripted_transcript() {
        let stt = MockStt::new(["kal 5 baje reminder lagao", "haan"]).language("hi-IN");
        let mut s = stt.open(RtConfig::default()).await.unwrap();
        s.input
            .send(RtInput::Frame(vec![0; FRAME_BYTES]))
            .await
            .unwrap();
        s.input.send(RtInput::Commit).await.unwrap();
        let t = next_transcript(&mut s).await.unwrap();
        assert_eq!(t.text, "kal 5 baje reminder lagao");
        assert_eq!(t.language.as_deref(), Some("hi-IN"));
        assert_eq!(t.segment_index, 0);
        s.input.send(RtInput::Commit).await.unwrap();
        assert_eq!(next_transcript(&mut s).await.unwrap().text, "haan");
        s.input.send(RtInput::Close).await.unwrap();
        assert!(next_transcript(&mut s).await.is_none(), "ends with Closed");
    }

    #[tokio::test]
    async fn the_event_order_is_connected_speech_processing_transcript() {
        let stt = MockStt::new(["x"]);
        let mut s = stt.open(RtConfig::default()).await.unwrap();
        s.input
            .send(RtInput::Frame(vec![0; FRAME_BYTES]))
            .await
            .unwrap();
        s.input.send(RtInput::Commit).await.unwrap();
        s.input.send(RtInput::Close).await.unwrap();
        let mut kinds = Vec::new();
        while let Some(e) = s.events.recv().await {
            kinds.push(match e {
                RtEvent::Connected { .. } => "connected",
                RtEvent::SpeechStart => "speech_start",
                RtEvent::Processing => "processing",
                RtEvent::Transcript(_) => "transcript",
                RtEvent::Reconnecting { .. } => "reconnecting",
                RtEvent::Error(_) => "error",
                RtEvent::Closed => "closed",
            });
        }
        assert_eq!(
            kinds,
            [
                "connected",
                "speech_start",
                "processing",
                "transcript",
                "closed"
            ]
        );
    }

    #[tokio::test]
    async fn a_simulated_1001_reconnects_before_any_transcript() {
        let mut s = MockStt::new(["x"])
            .reconnect_once()
            .open(RtConfig::default())
            .await
            .unwrap();
        assert!(matches!(
            s.events.recv().await,
            Some(RtEvent::Connected { .. })
        ));
        assert_eq!(
            s.events.recv().await,
            Some(RtEvent::Reconnecting { attempt: 1 })
        );
        assert!(matches!(
            s.events.recv().await,
            Some(RtEvent::Connected { .. })
        ));
    }

    #[tokio::test]
    async fn open_can_fail() {
        let err = MockStt::default()
            .failing(VoiceError::new(VoiceErrorCode::InvalidApiKey, "bad key"))
            .open(RtConfig::default())
            .await
            .unwrap_err();
        assert_eq!(err.code, VoiceErrorCode::InvalidApiKey);
    }

    #[tokio::test]
    async fn tts_streams_one_second_of_pcm_in_chunks() {
        let (tx, mut rx) = mpsc::channel(16);
        MockTts::new().stream(tts_request(), tx).await.unwrap();
        let mut total = 0;
        let mut chunks = 0;
        while let Some(c) = rx.recv().await {
            total += c.len();
            chunks += 1;
        }
        assert_eq!(total, PLAYBACK_SAMPLE_RATE as usize * 2);
        assert_eq!(chunks, 5);
    }

    #[tokio::test]
    async fn a_closed_receiver_stops_the_stream_without_error() {
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        MockTts::new().stream(tts_request(), tx).await.unwrap();
    }

    #[tokio::test]
    async fn synthesize_returns_a_wav_with_the_right_header() {
        let bytes = MockTts::new()
            .synthesize(tts_request(), AudioContainer::Wav)
            .await
            .unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(bytes.len(), 44 + PLAYBACK_SAMPLE_RATE as usize * 2);
        let err = MockTts::failing(VoiceError::new(VoiceErrorCode::GnaniUpstream, "down"))
            .synthesize(tts_request(), AudioContainer::Mp3)
            .await
            .unwrap_err();
        assert!(err.retryable);
    }
}
