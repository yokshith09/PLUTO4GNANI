//! Live check of extraction (T-203 / T-ML-02 groundwork): runs the AS-1 fixture transcript through
//! the real Gemini/Groq router and prints what came out. Keys come from `.env` (never printed).
//!
//!   cargo run -p pluto-intel --example extract_smoke
//!   cargo run -p pluto-intel --example extract_smoke -- path/to/segments.json

use std::sync::Arc;

use chrono::NaiveDate;
use pluto_contracts::meeting::TranscriptSegment;
use pluto_core::secrets::{Account, MemorySecrets, SecretStore};
use pluto_gnani::http::NoLog;
use pluto_intel::llm::gemini::{self, GeminiClient};
use pluto_intel::llm::groq::{self, GroqClient};
use pluto_intel::llm::router::Router;
use pluto_intel::meeting::extraction::{extract, ExtractInput};

#[tokio::main]
async fn main() {
    if let Ok(text) = std::fs::read_to_string(".env") {
        for (k, v) in text.lines().filter_map(|l| l.split_once('=')) {
            if !k.starts_with('#') && !v.trim().is_empty() && std::env::var(k.trim()).is_err() {
                std::env::set_var(k.trim(), v.trim().trim_matches('"'));
            }
        }
    }
    let secrets = Arc::new(MemorySecrets::new());
    for (acct, var) in [
        (Account::Gemini, "GEMINI_API_KEY"),
        (Account::Groq, "GROQ_API_KEY"),
    ] {
        if let Ok(k) = std::env::var(var) {
            secrets.set(acct, &k).expect("a valid key");
        }
    }
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "test-data/fixtures/meetings/transcript_segments.as1.json".into());
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("segments file"))
            .expect("JSON");
    let segments: Vec<TranscriptSegment> =
        serde_json::from_value(doc.get("data").cloned().unwrap_or(doc)).expect("segments");

    let gemini = GeminiClient::new(
        std::env::var("GEMINI_MODEL").unwrap_or(gemini::DEFAULT_MODEL.into()),
        secrets.clone(),
        Arc::new(NoLog),
    );
    let groq = GroqClient::new(
        std::env::var("GROQ_MODEL").unwrap_or(groq::DEFAULT_MODEL.into()),
        secrets,
        Arc::new(NoLog),
    );
    let router = Router::new(Arc::new(gemini), Arc::new(groq));

    let input = ExtractInput {
        meeting_id: "m-as1".into(),
        meeting_date: NaiveDate::from_ymd_opt(2026, 11, 2).unwrap(),
        participants: vec!["Asha".into(), "Ravi".into(), "Meena".into()],
        segments,
        open_tasks: vec![],
    };
    match extract(&router, &input).await {
        Ok(r) => {
            for i in &r.items {
                println!(
                    "{:?} | {} | owner {:?} | due {:?} ({:?}) | \"{}\"",
                    i.kind, i.body, i.owner, i.due_date, i.due_phrase, i.quote
                );
            }
            println!("warnings: {:?}", r.warnings);
        }
        Err(e) => println!("failed: {} ({:?})", e.message, e.code),
    }
}
