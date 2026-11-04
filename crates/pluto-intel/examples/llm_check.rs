//! T-009 / T-LLM-01: do Gemini and Groq return schema-valid JSON for the extraction task?
//!
//! Run from the repo root with keys in `.env` (copy `.env.example`):
//!   cargo run -p pluto-intel --example llm_check
//! Optional: GEMINI_MODEL, GROQ_MODEL to try other model IDs. Prints a table; never prints keys.
//! Uses synthetic meeting text only. Paste the output into `docs/results/d2-llm-check.md`.

use std::time::Instant;

use serde_json::{json, Value};

const KINDS: [&str; 5] = ["decision", "action_item", "question", "risk", "chitchat"];

const PROMPTS: [&str; 5] = [
    "Asha: Let's ship the pricing page on Friday. Ravi: I'll finish the copy by Thursday. Asha: Great, decision made, we go with the annual plan first.",
    "Meena: Budget approval is still pending. Karan: Who owns the vendor contract? Meena: Nobody yet. Risk: if legal is late we miss the launch.",
    "Suresh: Kal tak report bhej dena, Priya. Priya: Theek hai, main subah tak bhej dungi. Suresh: Aur server migration next week karenge, yeh final hai.",
    "Divya: How's the weather? Arun: Hot. Divya: Anyway, Arun please book the venue for the offsite by 20 Nov. We agreed to keep it under 50 thousand.",
    "Team: We discussed the roadmap. No one is sure about Q1 hiring. Lakshmi will draft the hiring plan and share it on Monday. Open question: do we need a second designer?",
];

const SYSTEM: &str = "You extract decisions, action items, questions and risks from a meeting excerpt. \
Return JSON {\"items\":[{\"kind\":\"decision|action_item|question|risk|chitchat\",\"body\":string,\"owner\":string|null,\"quote\":string}]}. \
`quote` must be copied word for word from the excerpt. Do not invent items.";

fn load_env() {
    if let Ok(text) = std::fs::read_to_string(".env") {
        for line in text.lines() {
            if let Some((k, v)) = line.split_once('=') {
                let (k, v) = (k.trim(), v.trim().trim_matches('"'));
                if !k.starts_with('#') && !v.is_empty() && std::env::var(k).is_err() {
                    std::env::set_var(k, v);
                }
            }
        }
    }
}

/// (schema-valid, every quote found in the source)
fn check(json: &Value, source: &str) -> (bool, bool) {
    let Some(items) = json.get("items").and_then(Value::as_array) else {
        return (false, false);
    };
    let mut valid = true;
    let mut grounded = true;
    for it in items {
        let kind_ok = it
            .get("kind")
            .and_then(Value::as_str)
            .is_some_and(|k| KINDS.contains(&k));
        let body_ok = it
            .get("body")
            .and_then(Value::as_str)
            .is_some_and(|b| !b.is_empty());
        let quote = it.get("quote").and_then(Value::as_str);
        valid &= kind_ok && body_ok && quote.is_some();
        grounded &= quote.is_some_and(|q| source.contains(q));
    }
    (valid, grounded)
}

fn gemini_schema() -> Value {
    json!({"type":"OBJECT","properties":{"items":{"type":"ARRAY","items":{"type":"OBJECT","properties":{
        "kind":{"type":"STRING","enum":KINDS},"body":{"type":"STRING"},
        "owner":{"type":"STRING","nullable":true},"quote":{"type":"STRING"}},
        "required":["kind","body","quote"]}}},"required":["items"]})
}

async fn gemini(
    c: &reqwest::Client,
    key: &str,
    model: &str,
    text: &str,
) -> Result<(Value, String), String> {
    let url =
        format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent");
    let body = json!({
        "systemInstruction":{"parts":[{"text":SYSTEM}]},
        "contents":[{"role":"user","parts":[{"text":text}]}],
        "generationConfig":{"responseMimeType":"application/json","responseSchema":gemini_schema(),"temperature":0}
    });
    let r = c
        .post(url)
        .header("x-goog-api-key", key)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = r.status();
    let v: Value = r.json().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!(
            "{status} {}",
            v["error"]["message"].as_str().unwrap_or("")
        ));
    }
    let t = v["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .ok_or("no text")?;
    Ok((
        serde_json::from_str(t).map_err(|e| format!("bad JSON: {e}"))?,
        String::new(),
    ))
}

async fn groq(
    c: &reqwest::Client,
    key: &str,
    model: &str,
    text: &str,
) -> Result<(Value, String), String> {
    let body = json!({"model":model,"temperature":0,"response_format":{"type":"json_object"},
        "messages":[{"role":"system","content":SYSTEM},{"role":"user","content":text}]});
    let r = c
        .post("https://api.groq.com/openai/v1/chat/completions")
        .bearer_auth(key)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = r.status();
    let limits: String = [
        "x-ratelimit-limit-requests",
        "x-ratelimit-remaining-requests",
        "x-ratelimit-limit-tokens",
        "x-ratelimit-remaining-tokens",
    ]
    .iter()
    .filter_map(|h| {
        r.headers()
            .get(*h)
            .and_then(|v| v.to_str().ok())
            .map(|v| format!("{h}={v}"))
    })
    .collect::<Vec<_>>()
    .join(" ");
    let v: Value = r.json().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!(
            "{status} {}",
            v["error"]["message"].as_str().unwrap_or("")
        ));
    }
    let t = v["choices"][0]["message"]["content"]
        .as_str()
        .ok_or("no text")?;
    Ok((
        serde_json::from_str(t).map_err(|e| format!("bad JSON: {e}"))?,
        limits,
    ))
}

#[tokio::main]
async fn main() {
    load_env();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .unwrap();
    for (name, key_var, model_var, default_model) in [
        (
            "Gemini",
            "GEMINI_API_KEY",
            "GEMINI_MODEL",
            "gemini-2.5-flash",
        ),
        (
            "Groq",
            "GROQ_API_KEY",
            "GROQ_MODEL",
            "llama-3.3-70b-versatile",
        ),
    ] {
        let Ok(key) = std::env::var(key_var) else {
            println!("{name}: {key_var} is not set in .env, skipped");
            continue;
        };
        let model = std::env::var(model_var).unwrap_or_else(|_| default_model.into());
        println!("\n== {name} ({model}) ==");
        let (mut valid, mut grounded) = (0, 0);
        for (i, p) in PROMPTS.iter().enumerate() {
            let t = Instant::now();
            let res = if name == "Gemini" {
                gemini(&client, &key, &model, p).await
            } else {
                groq(&client, &key, &model, p).await
            };
            let ms = t.elapsed().as_millis();
            match res {
                Ok((json, limits)) => {
                    let (v, g) = check(&json, p);
                    valid += usize::from(v);
                    grounded += usize::from(g);
                    let n = json["items"].as_array().map_or(0, Vec::len);
                    println!("prompt {} | {ms} ms | items {n} | schema_valid {v} | quotes_grounded {g} {limits}", i + 1);
                }
                Err(e) => println!("prompt {} | {ms} ms | ERROR {e}", i + 1),
            }
        }
        println!(
            "{name}: schema-valid {valid}/5, quotes grounded {grounded}/5 (target: 5/5 valid)"
        );
    }
    println!("\nAlso note each provider's free-tier limits from its dashboard (requests/min, per day, tokens).");
}
