//! Makes a synthetic test recording from a script with Gnani's Timbre voices (T-008), so the real
//! pipeline (import, Gnani Batch, speakers, extraction) can be tried without recording people.
//! The audio is generated speech on made-up content; say so wherever it is shown.
//!
//!   cargo run -p pluto-intel --example make_test_audio -- AS1 Asha=Shivani Ravi=<voice> Meena=<voice>
//!
//! - First argument: AS1, AS2 or AS3 (which script).
//! - Then `Name=Voice` pairs. Different people need different voices or the transcription will find
//!   one speaker. Voice names are in Gnani's Timbre documentation (docs.gnani.ai).
//! - Writes `test-data/audio/<id>.wav` (16 kHz mono, a few MB). Needs `GNANI_API_KEY` in `.env`.
//!   A line is spoken per request, so it uses a few hundred characters of your TTS credits.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use pluto_contracts::speech::{TextToSpeech, TtsRequest};
use pluto_gnani::http::{GnaniHttp, NoLog, StaticKey, DEFAULT_BASE_URL};
use pluto_gnani::tts::{TimbreClient, DEFAULT_WS_URL};
use pluto_intel::testaudio::{
    parse_script, pcm_bytes_to_samples, resample_24k_to_16k, silence_16k, wav_16k,
};

fn key() -> Option<String> {
    if let Ok(k) = std::env::var("GNANI_API_KEY") {
        if !k.trim().is_empty() {
            return Some(k.trim().to_owned());
        }
    }
    let text =
        std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.env"))
            .ok()?;
    text.lines()
        .filter_map(|l| l.trim().strip_prefix("GNANI_API_KEY="))
        .map(|v| v.trim().trim_matches('"').to_owned())
        .find(|v| !v.is_empty())
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let mut args = std::env::args().skip(1);
    let which = args.next().unwrap_or_else(|| "AS1".into()).to_uppercase();
    let voices: HashMap<String, String> = args
        .filter_map(|a| a.split_once('=').map(|(n, v)| (n.to_owned(), v.to_owned())))
        .collect();
    let (file, language) = match which.as_str() {
        "AS1" => ("as1-hinglish-launch-sync.md", "hi-en"),
        "AS2" => ("as2-tanglish-sprint-review.md", "auto"),
        "AS3" => ("as3-kannada-english-office-move.md", "auto"),
        other => {
            eprintln!("Unknown set {other}: use AS1, AS2 or AS3.");
            std::process::exit(2);
        }
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let script =
        std::fs::read_to_string(root.join("test-data/audio-scripts").join(file)).expect("script");
    let lines = parse_script(&script);
    let Some(key) = key() else {
        eprintln!("No Gnani key: set GNANI_API_KEY or fill it in .env at the repository root.");
        std::process::exit(2);
    };
    let keys = Arc::new(StaticKey(Some(key)));
    let http = Arc::new(
        GnaniHttp::new(DEFAULT_BASE_URL, keys.clone(), Arc::new(NoLog)).expect("http client"),
    );
    let client = TimbreClient::new(DEFAULT_WS_URL, http, keys, Arc::new(NoLog));

    let mut people: Vec<&str> = lines.iter().map(|l| l.speaker.as_str()).collect();
    people.sort_unstable();
    people.dedup();
    if voices.len() < people.len() {
        eprintln!("Warning: {} speakers but {} voices given. Speakers without one use the default voice, and the transcription will probably merge them into one speaker.", people.len(), voices.len());
    }

    let mut samples = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
        let collector = tokio::spawn(async move {
            let mut pcm = Vec::new();
            while let Some(c) = rx.recv().await {
                pcm.extend_from_slice(&c);
            }
            pcm
        });
        let request = TtsRequest {
            text: line.text.clone(),
            language: language.into(),
            voice: voices.get(&line.speaker).cloned(),
            model: None,
            speed: None,
        };
        if let Err(e) = client.stream(request, tx).await {
            eprintln!(
                "Line {} ({}) failed: {:?}: {}",
                i + 1,
                line.speaker,
                e.code,
                e.message
            );
            std::process::exit(1);
        }
        let pcm = collector.await.unwrap_or_default();
        println!(
            "{:>2}/{} {}: {} ({:.1} s)",
            i + 1,
            lines.len(),
            line.speaker,
            line.text.chars().take(40).collect::<String>(),
            pcm.len() as f64 / 48_000.0
        );
        samples.extend(resample_24k_to_16k(&pcm_bytes_to_samples(&pcm)));
        samples.extend(silence_16k(450));
    }
    let out = root
        .join("test-data/audio")
        .join(format!("{}.wav", which.to_lowercase()));
    std::fs::create_dir_all(out.parent().expect("dir")).expect("create test-data/audio");
    std::fs::write(&out, wav_16k(&samples)).expect("write wav");
    println!(
        "Saved {} ({:.1} s, {:.1} MB). Import it in Meetings to try the real transcription.",
        out.display(),
        samples.len() as f64 / 16_000.0,
        (samples.len() * 2) as f64 / 1_048_576.0
    );
}
