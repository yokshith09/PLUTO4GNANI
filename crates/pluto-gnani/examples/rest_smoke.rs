//! Live check of Prisma REST (T-GN-01 / T-010). Needs a Gnani key and the network.
//!
//!   cargo run -p pluto-gnani --example rest_smoke -- clip.wav [language_code]
//!
//! Sends one clip (≤ 30 s) to `POST /stt/v3` through the shared HTTP base and prints the answer's
//! transcript, language and timings (transcript text included: a local developer tool, not the
//! app). The key is read from `GNANI_API_KEY`, or from the git-ignored `.env` at the repository
//! root. The comparison client itself is D2's `rest.rs` (T-504); this only proves the key and path.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use pluto_gnani::http::{Call, Family, GnaniHttp, NoLog, StaticKey, DEFAULT_BASE_URL};
use reqwest::multipart::{Form, Part};

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
    let Some(path) = args.next() else {
        eprintln!(
            "usage: cargo run -p pluto-gnani --example rest_smoke -- clip.wav [language_code]"
        );
        std::process::exit(2);
    };
    let language = args.next().unwrap_or_else(|| "indic-auto".into());
    let Some(key) = api_key() else {
        eprintln!("No Gnani key: set GNANI_API_KEY or fill it in .env at the repository root.");
        std::process::exit(2);
    };
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("can't read {path}: {e}");
            std::process::exit(2);
        }
    };
    let http = match GnaniHttp::new(
        DEFAULT_BASE_URL,
        Arc::new(StaticKey(Some(key))),
        Arc::new(NoLog),
    ) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("{}", e.message);
            std::process::exit(1);
        }
    };
    let url = http.url("/stt/v3");
    let size = bytes.len() as u64;
    println!(
        "sending {path} ({:.0} KB, language_code={language})",
        size as f64 / 1024.0
    );
    let started = Instant::now();
    let result: Result<serde_json::Value, _> = http
        .execute_json(Call::new(Family::SttRest, "clip", size), |c| {
            let form = Form::new()
                .part(
                    "audio_file",
                    Part::bytes(bytes.clone())
                        .file_name("clip.wav")
                        .mime_str("audio/wav")
                        .expect("static mime"),
                )
                .text("model", "prisma-v3.0")
                .text("language_code", language.clone())
                .text("format", "codemix");
            c.post(&url).multipart(form)
        })
        .await;
    match result {
        Ok(v) => {
            println!("answer in {:?}", started.elapsed());
            for k in [
                "success",
                "transcript",
                "detected_language_code",
                "processing_time",
                "end_to_end_latency",
                "model",
            ] {
                println!("  {k}: {}", v.get(k).cloned().unwrap_or_default());
            }
            let ok = v.get("success").and_then(|s| s.as_bool()) != Some(false)
                && v.get("transcript")
                    .and_then(|t| t.as_str())
                    .is_some_and(|t| !t.is_empty());
            std::process::exit(if ok { 0 } else { 1 });
        }
        Err(e) => {
            eprintln!("failed: {:?}: {}", e.code, e.message);
            std::process::exit(1);
        }
    }
}
