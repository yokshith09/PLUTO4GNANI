//! Recall (T-305): "what did we decide about the launch date?" answered only from the user's own
//! transcripts. The words are searched in Rust, the LLM only phrases an answer from the snippets it
//! is given, and the answer is kept only if it names snippets that really were given (grounding,
//! ARCHITECTURE §7). No grounded source: "Not found in your meetings." and no guessing.

use pluto_contracts::error::VoiceResult;
use pluto_contracts::llm::{Llm, LlmMessage, LlmRequest, LlmRole, LlmTask};
use pluto_contracts::meeting::{Citation, RecallAnswer, TranscriptSegment};
use serde_json::{json, Value};

use crate::llm::router::default_timeout_ms;

pub const NOT_FOUND: &str = "Not found in your meetings.";
/// Snippets handed to the LLM.
pub const TOP_K: usize = 8;

const STOP: &[&str] = &[
    "the",
    "and",
    "was",
    "were",
    "what",
    "who",
    "when",
    "where",
    "which",
    "did",
    "does",
    "how",
    "about",
    "for",
    "with",
    "that",
    "this",
    "have",
    "has",
    "had",
    "are",
    "our",
    "you",
    "your",
    "his",
    "her",
    "them",
    "they",
    "said",
    "say",
    "tell",
    "meeting",
    "kya",
    "kab",
    "kaun",
    "kahan",
    "kaise",
    "ne",
    "ka",
    "ki",
    "ke",
    "hai",
    "tha",
    "thi",
    "the",
    "bola",
    "bole",
    "mein",
    "main",
    "se",
    "ko",
    "tak",
    "bhi",
    "aur",
    "yeh",
    "woh",
    "hum",
    "humne",
    "pichli",
    "last",
    "decide",
    "decided",
    "hui",
    "hua",
    "kiya",
    "enna",
    "yaaru",
    "eppo",
    "yenu",
    "yaavaga",
    "yaaru",
    "nirdharisidvi",
];

/// Words worth searching for: letters and digits, at least 3 characters, no filler.
pub fn search_terms(question: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in question
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 3)
    {
        if !STOP.contains(&w) && !out.iter().any(|o| o == w) {
            out.push(w.to_owned());
        }
    }
    out
}

/// One transcript line with where it came from.
#[derive(Debug, Clone)]
pub struct Hit {
    pub segment: TranscriptSegment,
    pub meeting_title: String,
    /// Meeting start (ms) plus the line's offset: when it was said.
    pub said_at: i64,
}

fn schema() -> Value {
    json!({
        "type": "object",
        "required": ["found", "answer", "sources"],
        "properties": {
            "found": {"type": "boolean"},
            "answer": {"type": "string"},
            "sources": {"type": "array", "items": {"type": "string"}}
        }
    })
}

const SYSTEM: &str = "You answer a question about past meetings using ONLY the numbered snippets below. \
If the snippets do not answer it, set found=false. Otherwise write one or two short sentences in the language of the question \
(for example Hindi, Tamil, Kannada or English) and list the numbers of the snippets you used in `sources`, like [\"S1\",\"S3\"]. \
Do not add facts that are not in the snippets.";

fn speaker(s: &TranscriptSegment) -> String {
    s.participant.clone().unwrap_or_else(|| {
        s.speaker_label
            .map_or("Unknown".into(), |n| format!("Speaker {n}"))
    })
}

fn not_found(language: &str) -> RecallAnswer {
    RecallAnswer {
        answer_text: NOT_FOUND.into(),
        language: language.into(),
        citations: vec![],
        proposal: None,
    }
}

/// Ask the LLM to answer from `hits`. Anything it says without a valid source is thrown away.
pub async fn answer(
    llm: &dyn Llm,
    question: &str,
    language: &str,
    hits: &[Hit],
) -> VoiceResult<RecallAnswer> {
    let hits = &hits[..hits.len().min(TOP_K)];
    if hits.is_empty() {
        return Ok(not_found(language));
    }
    let mut prompt = format!("Question: {question}\n\nSnippets:\n");
    for (i, h) in hits.iter().enumerate() {
        prompt.push_str(&format!(
            "[S{}] {} — {}: {}\n",
            i + 1,
            h.meeting_title,
            speaker(&h.segment),
            h.segment.text
        ));
    }
    let resp = llm
        .complete_json(LlmRequest {
            task: LlmTask::RecallAnswer,
            system: SYSTEM.into(),
            messages: vec![LlmMessage {
                role: LlmRole::User,
                content: prompt,
            }],
            json_schema: schema(),
            timeout_ms: default_timeout_ms(LlmTask::RecallAnswer),
        })
        .await?;
    let v = &resp.json;
    let text = v["answer"].as_str().map(str::trim).unwrap_or_default();
    if v["found"].as_bool() != Some(true) || text.is_empty() {
        return Ok(not_found(language));
    }
    let mut citations: Vec<Citation> = Vec::new();
    for src in v["sources"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        let idx = src
            .trim_start_matches(['S', 's'])
            .parse::<usize>()
            .ok()
            .and_then(|n| n.checked_sub(1));
        let Some(h) = idx.and_then(|i| hits.get(i)) else {
            continue;
        };
        if citations.iter().any(|c| c.segment_id == h.segment.id) {
            continue;
        }
        citations.push(Citation {
            meeting_id: h.segment.meeting_id.clone().unwrap_or_default(),
            meeting_title: h.meeting_title.clone(),
            segment_id: h.segment.id.clone(),
            speaker: Some(speaker(&h.segment)).filter(|s| s != "Unknown"),
            quote: h.segment.text.clone(),
            starts_at: h.said_at,
        });
    }
    if citations.is_empty() {
        tracing::info!("dropped a recall answer with no valid source");
        return Ok(not_found(language));
    }
    Ok(RecallAnswer {
        answer_text: text.to_owned(),
        language: language.into(),
        citations,
        proposal: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_terms_drop_filler_and_duplicates() {
        assert_eq!(
            search_terms("What did we decide about the launch date?"),
            ["launch", "date"]
        );
        assert_eq!(
            search_terms("Pichli meeting mein launch date kya decide hui thi"),
            ["launch", "date"]
        );
        assert_eq!(search_terms("launch Launch LAUNCH"), ["launch"]);
        assert!(search_terms("is it ok").is_empty());
    }
}
