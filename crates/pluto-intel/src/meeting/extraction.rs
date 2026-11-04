//! Extraction (T-203): transcript segments in, validated decisions, action items, questions and
//! risks out. The LLM proposes; Rust checks every item (API_CONTRACTS §3.3, ARCHITECTURE §7):
//!
//! - the source segment must exist, otherwise the item is dropped (A3.9);
//! - the quote shown to the user is always real text from that segment;
//! - the owner must be a participant, otherwise it is `None` and a warning is raised;
//! - the due phrase is resolved to a date here, never by the LLM;
//! - items that look like an open task are flagged (`duplicate_of_task_id`), not dropped.

use std::collections::HashSet;

use chrono::NaiveDate;
use pluto_contracts::error::VoiceResult;
use pluto_contracts::llm::{Llm, LlmMessage, LlmRequest, LlmRole, LlmTask};
use pluto_contracts::meeting::{
    ExtractedItem, ExtractionResult, QualityWarning, Relevance, TranscriptSegment,
};
use pluto_core::ids::new_id;
use pluto_core::tasks::Priority;
use serde_json::{json, Value};

use crate::dates::resolve_date;
use crate::llm::router::default_timeout_ms;

/// Segments per LLM call.
pub const BATCH: usize = 40;
const QUOTE_MAX: usize = 1_000;

#[derive(Debug, Clone)]
pub struct OpenTask {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct ExtractInput {
    pub meeting_id: String,
    pub meeting_date: NaiveDate,
    pub participants: Vec<String>,
    pub segments: Vec<TranscriptSegment>,
    pub open_tasks: Vec<OpenTask>,
}

fn schema() -> Value {
    json!({
        "type": "object",
        "required": ["items"],
        "properties": {"items": {"type": "array", "items": {
            "type": "object",
            "required": ["kind", "body", "segment_id", "quote"],
            "properties": {
                "kind": {"type": "string", "enum": ["decision", "action_item", "question", "risk"]},
                "body": {"type": "string"},
                "owner": {"type": "string", "nullable": true},
                "due_phrase": {"type": "string", "nullable": true},
                "priority": {"type": "string", "enum": ["LOW", "MEDIUM", "HIGH"], "nullable": true},
                "segment_id": {"type": "string"},
                "quote": {"type": "string"}
            }
        }}}
    })
}

const SYSTEM: &str = "You read a meeting transcript and list its decisions, action items, open questions and risks. \
Rules: use only what was said; do not invent items; skip small talk. `body` is one short English sentence. \
`owner` is the person who must do an action item, exactly as written in the participant list, or null. \
`due_phrase` is the deadline words exactly as spoken (for example \"Friday tak\"), or null; never convert it to a date. \
`segment_id` is the id in square brackets of the line the item comes from. \
`quote` is copied word for word from that line. The meeting may mix Hindi, Tamil, Kannada and English.";

fn speaker(s: &TranscriptSegment) -> String {
    match (&s.participant, s.speaker_label) {
        (Some(p), _) => p.clone(),
        (None, Some(n)) => format!("Speaker {n}"),
        (None, None) => "Unknown".into(),
    }
}

fn user_prompt(input: &ExtractInput, batch: &[TranscriptSegment]) -> String {
    let mut out = format!(
        "Meeting date: {}\nParticipants: {}\n\nTranscript:\n",
        input.meeting_date,
        if input.participants.is_empty() {
            "unknown".into()
        } else {
            input.participants.join(", ")
        }
    );
    for s in batch {
        out.push_str(&format!("[{}] {}: {}\n", s.id, speaker(s), s.text));
    }
    out
}

fn norm(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn words(s: &str) -> HashSet<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .map(str::to_owned)
        .collect()
}

/// The open task this item most likely repeats (at least 60% word overlap), if any.
fn duplicate_of(body: &str, tasks: &[OpenTask]) -> Option<String> {
    let b = words(body);
    if b.is_empty() {
        return None;
    }
    tasks
        .iter()
        .map(|t| {
            let w = words(&t.title);
            let shared = b.intersection(&w).count() as f32;
            (t, shared / b.len().min(w.len().max(1)) as f32)
        })
        .filter(|(_, score)| *score >= 0.6)
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(t, _)| t.id.clone())
}

fn kind_of(s: &str) -> Option<Relevance> {
    match s {
        "decision" => Some(Relevance::Decision),
        "action_item" => Some(Relevance::ActionItem),
        "question" => Some(Relevance::Question),
        "risk" => Some(Relevance::Risk),
        _ => None,
    }
}

fn priority_of(s: Option<&str>) -> Option<Priority> {
    match s {
        Some("LOW") => Some(Priority::Low),
        Some("MEDIUM") => Some(Priority::Medium),
        Some("HIGH") => Some(Priority::High),
        _ => None,
    }
}

fn clip(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// Turn one raw LLM item into a checked [`ExtractedItem`], or drop it.
fn validate(raw: &Value, input: &ExtractInput) -> Option<ExtractedItem> {
    let kind = kind_of(raw["kind"].as_str()?)?;
    let body = raw["body"].as_str()?.trim();
    if body.is_empty() {
        return None;
    }
    let segment = input
        .segments
        .iter()
        .find(|s| Some(s.id.as_str()) == raw["segment_id"].as_str());
    let Some(segment) = segment else {
        tracing::info!("dropped an extracted item with no matching segment");
        return None;
    };
    // The quote must be real text from the segment; otherwise show the segment itself.
    let quote = raw["quote"]
        .as_str()
        .map(str::trim)
        .filter(|q| !q.is_empty() && norm(&segment.text).contains(&norm(q)))
        .map_or_else(|| segment.text.clone(), str::to_owned);

    let owner = raw["owner"]
        .as_str()
        .map(str::trim)
        .filter(|o| !o.is_empty())
        .and_then(|o| {
            input
                .participants
                .iter()
                .find(|p| p.eq_ignore_ascii_case(o))
                .cloned()
        });
    let due_phrase = raw["due_phrase"]
        .as_str()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_owned);
    let due_date = due_phrase
        .as_deref()
        .and_then(|p| resolve_date(p, input.meeting_date))
        .map(|d| d.to_string());
    let is_action = kind == Relevance::ActionItem;

    Some(ExtractedItem {
        id: new_id(),
        kind,
        body: clip(body, 300),
        owner: if is_action { owner } else { None },
        due_phrase: if is_action { due_phrase } else { None },
        due_date: if is_action { due_date } else { None },
        priority: if is_action {
            priority_of(raw["priority"].as_str())
        } else {
            None
        },
        source_segment_id: segment.id.clone(),
        quote: clip(&quote, QUOTE_MAX),
        duplicate_of_task_id: if is_action {
            duplicate_of(body, &input.open_tasks)
        } else {
            None
        },
    })
}

pub fn warnings(items: &[ExtractedItem]) -> Vec<QualityWarning> {
    let mut w = Vec::new();
    if !items.iter().any(|i| i.kind == Relevance::Decision) {
        w.push(QualityWarning::NoDecisions);
    }
    for i in items.iter().filter(|i| i.kind == Relevance::ActionItem) {
        if i.owner.is_none() {
            w.push(QualityWarning::ActionItemWithoutOwner {
                item_id: i.id.clone(),
            });
        }
        if i.due_phrase.is_some() && i.due_date.is_none() {
            w.push(QualityWarning::UnresolvedDueDate {
                item_id: i.id.clone(),
            });
        }
    }
    w
}

/// Run extraction over the whole transcript (batches of [`BATCH`] segments).
pub async fn extract(llm: &dyn Llm, input: &ExtractInput) -> VoiceResult<ExtractionResult> {
    let mut items: Vec<ExtractedItem> = Vec::new();
    for batch in input.segments.chunks(BATCH) {
        let resp = llm
            .complete_json(LlmRequest {
                task: LlmTask::Extract,
                system: SYSTEM.into(),
                messages: vec![LlmMessage {
                    role: LlmRole::User,
                    content: user_prompt(input, batch),
                }],
                json_schema: schema(),
                timeout_ms: default_timeout_ms(LlmTask::Extract),
            })
            .await?;
        // Batches overlap in nothing, but the model may repeat an item; keep the first of each.
        for raw in resp.json["items"].as_array().into_iter().flatten() {
            if let Some(item) = validate(raw, input) {
                let again = items
                    .iter()
                    .any(|i| i.kind == item.kind && norm(&i.body) == norm(&item.body));
                if !again {
                    items.push(item);
                }
            }
        }
    }
    let dominant_language = dominant_language(&input.segments);
    let warnings = warnings(&items);
    Ok(ExtractionResult {
        meeting_id: input.meeting_id.clone(),
        items,
        warnings,
        dominant_language,
    })
}

fn dominant_language(segments: &[TranscriptSegment]) -> Option<String> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for l in segments.iter().filter_map(|s| s.language.as_deref()) {
        match counts.iter_mut().find(|(k, _)| k == l) {
            Some((_, n)) => *n += 1,
            None => counts.push((l.to_owned(), 1)),
        }
    }
    counts.into_iter().max_by_key(|(_, n)| *n).map(|(l, _)| l)
}
