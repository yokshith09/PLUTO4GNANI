//! Live check of Prisma Realtime (T-GN-01 / T-101). Needs a Gnani key and the network.
//!
//!   cargo run -p pluto-gnani --example rt_smoke -- path/to/clip.wav [language_code]
//!
//! The clip must be 16 kHz, mono, 16-bit PCM WAV (e.g. `ffmpeg -i in.m4a -ar 16000 -ac 1 out.wav`).
//! The key is read from `GNANI_API_KEY`, or from the git-ignored `.env` at the repository root.
//! Audio is streamed at real-time pace in 1,024-byte frames, then committed; every event is printed
//! (transcript text included: this is a local developer tool, not the app).

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pluto_contracts::speech::{
    RtConfig, RtEvent, RtInput, SpeechToText, CAPTURE_SAMPLE_RATE, FRAME_BYTES,
};
use pluto_gnani::http::{NoLog, StaticKey};
use pluto_gnani::realtime::{RealtimeClient, DEFAULT_URL};

/// `GNANI_API_KEY` from the environment, else from `.env` in the repository root.
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

fn read_wav(path: &str) -> Result<Vec<u8>, String> {
    let mut reader = hound::WavReader::open(path).map_err(|e| format!("can't open {path}: {e}"))?;
    let spec = reader.spec();
    if spec.sample_rate != CAPTURE_SAMPLE_RATE || spec.channels != 1 || spec.bits_per_sample != 16 {
        return Err(format!(
            "{path} is {} Hz, {} channel(s), {}-bit; it must be 16000 Hz mono 16-bit \
             (ffmpeg -i in -ar 16000 -ac 1 out.wav)",
            spec.sample_rate, spec.channels, spec.bits_per_sample
        ));
    }
    let mut pcm = Vec::new();
    for s in reader.samples::<i16>() {
        pcm.extend_from_slice(&s.map_err(|e| e.to_string())?.to_le_bytes());
    }
    Ok(pcm)
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: cargo run -p pluto-gnani --example rt_smoke -- clip.wav [language_code]");
        std::process::exit(2);
    };
    let language = args.next().unwrap_or_else(|| "indic-auto".into());
    let Some(key) = api_key() else {
        eprintln!("No Gnani key: set GNANI_API_KEY or fill it in .env at the repository root.");
        std::process::exit(2);
    };
    let pcm = match read_wav(&path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let seconds = pcm.len() as f64 / f64::from(CAPTURE_SAMPLE_RATE * 2);
    println!("streaming {seconds:.1} s of {path} (language_code={language})");

    let client = RealtimeClient::new(DEFAULT_URL, Arc::new(StaticKey(Some(key))), Arc::new(NoLog));
    let config = RtConfig {
        language_code: language,
        ..RtConfig::default()
    };
    let started = Instant::now();
    let mut session = match client.open(config).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("open failed: {:?}: {}", e.code, e.message);
            std::process::exit(1);
        }
    };

    let input = session.input.clone();
    let sender = tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(32));
        for chunk in pcm.chunks(FRAME_BYTES) {
            tick.tick().await;
            let mut frame = chunk.to_vec();
            frame.resize(FRAME_BYTES, 0); // pad the last frame with silence
            if input.send(RtInput::Frame(frame)).await.is_err() {
                return;
            }
        }
        let _ = input.send(RtInput::Commit).await;
    });

    let mut transcripts = 0;
    let mut sent_all_at: Option<Instant> = None;
    loop {
        let wait = match sent_all_at {
            // After the commit, give the last transcript up to 5 s.
            Some(at) => Duration::from_secs(5).saturating_sub(at.elapsed()),
            None => Duration::from_secs(60),
        };
        let event = match tokio::time::timeout(wait, session.events.recv()).await {
            Ok(Some(e)) => e,
            Ok(None) => break,
            Err(_) => break,
        };
        let t = started.elapsed().as_secs_f64();
        match &event {
            RtEvent::Transcript(tr) => {
                transcripts += 1;
                println!(
                    "{t:6.2}s  transcript #{}  [{}]  latency={:?} ms  {}",
                    tr.segment_index,
                    tr.language.as_deref().unwrap_or("?"),
                    tr.latency_ms,
                    tr.text
                );
            }
            RtEvent::Error(e) => println!("{t:6.2}s  error {:?}: {}", e.code, e.message),
            RtEvent::Closed => {
                println!("{t:6.2}s  closed");
                break;
            }
            other => println!("{t:6.2}s  {other:?}"),
        }
        if sent_all_at.is_none() && sender.is_finished() {
            sent_all_at = Some(Instant::now());
        }
    }
    let _ = session.input.send(RtInput::Close).await;
    println!(
        "done: {transcripts} transcript(s) in {:.1} s",
        started.elapsed().as_secs_f64()
    );
    std::process::exit(if transcripts > 0 { 0 } else { 1 });
}
