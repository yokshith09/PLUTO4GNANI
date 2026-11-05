//! Live check of Timbre (T-GN-01 / T-102). Needs a Gnani key and the network.
//!
//!   cargo run -p pluto-gnani --example tts_smoke -- "Ho gaya" hi-IN [voice]
//!
//! Streams the text over the Timbre WebSocket, prints the time to the first audio chunk and the
//! chunk count, and saves what arrived as `target/tts_smoke.wav` (PCM16 24 kHz mono) to play back.
//! The key is read from `GNANI_API_KEY`, or from the git-ignored `.env` at the repository root.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use pluto_contracts::speech::{TextToSpeech, TtsRequest, PLAYBACK_SAMPLE_RATE};
use pluto_gnani::http::{GnaniHttp, NoLog, StaticKey, DEFAULT_BASE_URL};
use pluto_gnani::tts::{TimbreClient, DEFAULT_WS_URL};

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

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let mut args = std::env::args().skip(1);
    let text = args.next().unwrap_or_else(|| "Ho gaya.".into());
    let language = args.next().unwrap_or_else(|| "hi-IN".into());
    let voice = args.next();
    let Some(key) = api_key() else {
        eprintln!("No Gnani key: set GNANI_API_KEY or fill it in .env at the repository root.");
        std::process::exit(2);
    };
    let keys = Arc::new(StaticKey(Some(key)));
    let http = match GnaniHttp::new(DEFAULT_BASE_URL, keys.clone(), Arc::new(NoLog)) {
        Ok(h) => Arc::new(h),
        Err(e) => {
            eprintln!("{}", e.message);
            std::process::exit(1);
        }
    };
    let client = TimbreClient::new(DEFAULT_WS_URL, http, keys, Arc::new(NoLog));
    println!("speaking {text:?} in {language}");

    let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
    let started = Instant::now();
    let collector = tokio::spawn(async move {
        let mut first = None;
        let mut pcm = Vec::new();
        let mut chunks = 0;
        while let Some(chunk) = rx.recv().await {
            first.get_or_insert_with(|| started.elapsed());
            chunks += 1;
            pcm.extend_from_slice(&chunk);
        }
        (first, chunks, pcm)
    });
    let request = TtsRequest {
        text,
        language,
        voice,
        model: None,
        speed: None,
    };
    let result = client.stream(request, tx).await;
    let (first, chunks, pcm) = collector.await.unwrap_or_default();
    if let Err(e) = result {
        eprintln!("stream failed: {:?}: {}", e.code, e.message);
        std::process::exit(1);
    }
    let seconds = pcm.len() as f64 / f64::from(PLAYBACK_SAMPLE_RATE * 2);
    println!(
        "first audio after {:?}; {chunks} chunk(s), {seconds:.2} s of audio in {:?}",
        first.unwrap_or_default(),
        started.elapsed()
    );

    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/tts_smoke.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: PLAYBACK_SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    match hound::WavWriter::create(&out, spec) {
        Ok(mut w) => {
            for pair in pcm.chunks_exact(2) {
                let _ = w.write_sample(i16::from_le_bytes([pair[0], pair[1]]));
            }
            let _ = w.finalize();
            println!("saved {}", out.display());
        }
        Err(e) => eprintln!("couldn't save the WAV: {e}"),
    }
    std::process::exit(if chunks > 0 { 0 } else { 1 });
}
