//! Live check of one push-to-talk turn through the voice runtime (T-301) with the real Prisma
//! Realtime, without the UI: press, push a 16 kHz mono WAV as 1,024-byte frames at real-time pace
//! (as `audio_push` would), release, and print every status and the words the assistant gets.
//!
//!   cargo run -p pluto-voice --example ptt_smoke -- clip16k.wav
//!
//! The key is read from `GNANI_API_KEY`, or from the git-ignored `.env` at the repository root.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use pluto_contracts::error::{VoiceError, VoiceResult};
use pluto_contracts::speech::{RtTranscript, FRAME_BYTES};
use pluto_contracts::voice::VoiceStatus;
use pluto_gnani::http::{NoLog, StaticKey};
use pluto_gnani::realtime::{RealtimeClient, DEFAULT_URL};
use pluto_voice::audio::AudioRouter;
use pluto_voice::runtime::{Heard, RuntimeEvents, Turn, TurnHandler, VoiceRuntime};

fn api_key() -> Option<String> {
    if let Ok(k) = std::env::var("GNANI_API_KEY") {
        if !k.trim().is_empty() {
            return Some(k.trim().to_string());
        }
    }
    let env = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.env");
    let text = std::fs::read_to_string(env).ok()?;
    text.lines()
        .filter_map(|l| l.trim().strip_prefix("GNANI_API_KEY="))
        .map(|v| v.trim().trim_matches('"').to_string())
        .find(|v| !v.is_empty())
}

struct Print(Instant);

impl RuntimeEvents for Print {
    fn status(&self, s: &VoiceStatus) {
        println!(
            "{:6.2}s  status {:?} {}",
            self.0.elapsed().as_secs_f64(),
            s.state,
            s.detail.clone().unwrap_or_default()
        );
    }
    fn transcript(&self, _: &str, t: &RtTranscript) {
        println!(
            "{:6.2}s  transcript {}",
            self.0.elapsed().as_secs_f64(),
            t.text
        );
    }
    fn error(&self, e: &VoiceError) {
        println!(
            "{:6.2}s  error {:?}: {}",
            self.0.elapsed().as_secs_f64(),
            e.code,
            e.message
        );
    }
}

struct Echo;

#[async_trait]
impl TurnHandler for Echo {
    async fn turn(&self, heard: Heard) -> VoiceResult<Turn> {
        println!(
            "HEARD [{}] {}",
            heard.language.unwrap_or_default(),
            heard.text
        );
        Ok(Turn::Done)
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: cargo run -p pluto-voice --example ptt_smoke -- clip16k.wav");
        std::process::exit(2);
    });
    let key = api_key().unwrap_or_else(|| {
        eprintln!("No Gnani key in GNANI_API_KEY or .env");
        std::process::exit(2);
    });
    let mut reader = hound::WavReader::open(&path).expect("a 16 kHz mono 16-bit WAV");
    let pcm: Vec<u8> = reader
        .samples::<i16>()
        .flat_map(|s| s.unwrap_or(0).to_le_bytes())
        .collect();

    let router = Arc::new(AudioRouter::new());
    let stt = Arc::new(RealtimeClient::new(
        DEFAULT_URL,
        Arc::new(StaticKey(Some(key))),
        Arc::new(NoLog),
    ));
    let rt = Arc::new(VoiceRuntime::new(
        stt,
        router.clone(),
        Arc::new(Print(Instant::now())),
        Arc::new(Echo),
        true,
    ));
    let session = rt.ptt_down().expect("press");
    let mut tick = tokio::time::interval(Duration::from_millis(32));
    for chunk in pcm.chunks(FRAME_BYTES) {
        tick.tick().await;
        let mut frame = chunk.to_vec();
        frame.resize(FRAME_BYTES, 0);
        router.push(&session, frame).expect("push");
    }
    rt.ptt_up();
    tokio::time::sleep(Duration::from_secs(5)).await;
}
