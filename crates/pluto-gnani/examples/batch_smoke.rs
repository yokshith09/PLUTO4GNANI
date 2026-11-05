//! Live check of Prisma Batch (T-201, INTEGRATION_CONTRACT step 3). Needs a Gnani key and the
//! network.
//!
//!   cargo run -p pluto-gnani --example batch_smoke -- test-data/audio/as1.wav [Priya,Rahul,Asha]
//!
//! Uploads the file (≤ 10 MB), starts the job, polls until it ends, then prints every segment with
//! its speaker and times (transcript text included: this is a local developer tool, not the app).
//! The key is read from `GNANI_API_KEY`, or from the git-ignored `.env` at the repository root.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use pluto_gnani::batch::{
    clock_text, wait_for, BatchApi, BatchClient, BatchConfig, BatchFile, PollSchedule,
};
use pluto_gnani::http::{GnaniHttp, NoLog, StaticKey, DEFAULT_BASE_URL};

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

fn fail(msg: impl std::fmt::Display) -> ! {
    eprintln!("{msg}");
    std::process::exit(1);
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: cargo run -p pluto-gnani --example batch_smoke -- file.wav [Name,Name]");
        std::process::exit(2);
    };
    let participants: Vec<String> = args
        .next()
        .map(|s| s.split(',').map(str::to_string).collect())
        .unwrap_or_default();
    let Some(key) = api_key() else {
        eprintln!("No Gnani key: set GNANI_API_KEY or fill it in .env at the repository root.");
        std::process::exit(2);
    };
    let bytes = std::fs::read(&path).unwrap_or_else(|e| fail(format!("can't read {path}: {e}")));
    let name = Path::new(&path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "audio.wav".into());

    let http = GnaniHttp::new(
        DEFAULT_BASE_URL,
        Arc::new(StaticKey(Some(key))),
        Arc::new(NoLog),
    )
    .unwrap_or_else(|e| fail(e.message));
    let client =
        BatchClient::new(Arc::new(http), Arc::new(NoLog)).unwrap_or_else(|e| fail(e.message));
    let config = BatchConfig::for_meeting(&participants, &[], None);
    println!(
        "uploading {name} ({:.1} MB), config {}",
        bytes.len() as f64 / 1_048_576.0,
        serde_json::to_string(&config).unwrap_or_default()
    );

    let started = Instant::now();
    let job = client
        .create(&[BatchFile { name, bytes }], &config, "smoke")
        .await
        .unwrap_or_else(|e| fail(format!("create failed: {:?}: {}", e.code, e.message)));
    println!("job {job} created");
    let st = client
        .start(&job)
        .await
        .unwrap_or_else(|e| fail(format!("start failed: {:?}: {}", e.code, e.message)));
    println!("start → {st:?}");
    let end = wait_for(&client, &job, &PollSchedule::default(), |s| {
        println!(
            "{:6.0}s  {:?}  {}/{} done, {} failed",
            started.elapsed().as_secs_f64(),
            s.status,
            s.completed_files,
            s.total_files,
            s.failed_files
        );
    })
    .await
    .unwrap_or_else(|e| fail(format!("polling failed: {:?}: {}", e.code, e.message)));

    let files = client
        .files(&job)
        .await
        .unwrap_or_else(|e| fail(format!("files failed: {:?}: {}", e.code, e.message)));
    let mut total = 0;
    for f in &files {
        println!("file {} → {}", f.original_path, f.status);
        let Some(url) = f.transcript_url.as_deref().filter(|_| f.is_completed()) else {
            println!("  {:?} {:?}", f.error_code, f.error_message);
            continue;
        };
        let segs = client
            .transcript(url, "smoke")
            .await
            .unwrap_or_else(|e| fail(format!("transcript failed: {:?}: {}", e.code, e.message)));
        for s in &segs {
            println!(
                "  {}–{}  spk {:?}  [{}]  {}",
                clock_text(s.start_ms),
                clock_text(s.end_ms),
                s.speaker,
                s.language.as_deref().unwrap_or("?"),
                s.text
            );
        }
        total += segs.len();
    }
    println!(
        "done: {:?}, {total} segment(s) in {:.0} s",
        end.status,
        started.elapsed().as_secs_f64()
    );
    std::process::exit(if total > 0 { 0 } else { 1 });
}
