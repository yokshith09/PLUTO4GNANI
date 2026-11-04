//! Recap (T-205): a 60–90 word summary of what was decided and who does what, written from the
//! applied items only, in the chosen language. Reading it aloud goes through `TextToSpeech`.

use chrono::NaiveDate;
use pluto_contracts::error::{VoiceError, VoiceErrorCode, VoiceResult};
use pluto_contracts::llm::{Llm, LlmMessage, LlmRequest, LlmRole, LlmTask};
use pluto_contracts::speech::{AudioContainer, TextToSpeech, TtsRequest};
use serde_json::json;

use crate::llm::router::default_timeout_ms;

/// Longer than this and the recap is cut at a sentence end.
pub const MAX_WORDS: usize = 110;

#[derive(Debug, Clone)]
pub struct RecapAction {
    pub body: String,
    pub owner: Option<String>,
    pub due_date: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RecapInput {
    pub title: String,
    pub date: NaiveDate,
    pub participants: Vec<String>,
    pub decisions: Vec<String>,
    pub actions: Vec<RecapAction>,
}

/// The language name the prompt uses for a code like `hi-IN`; unknown codes get English.
pub fn language_name(code: &str) -> &'static str {
    match code.split('-').next().unwrap_or("en") {
        "hi" => "Hindi",
        "ta" => "Tamil",
        "kn" => "Kannada",
        "te" => "Telugu",
        "ml" => "Malayalam",
        "mr" => "Marathi",
        "pa" => "Punjabi",
        "bn" => "Bengali",
        "gu" => "Gujarati",
        _ => "English",
    }
}

const SYSTEM: &str = "You write a short spoken recap of a meeting for the people who were in it. \
Use ONLY the decisions and action items given; do not add anything. About 60 to 90 words, plain sentences that sound natural \
read aloud, no lists or symbols. Say who does what and by when. Names stay as written. Write in the requested language.";

fn prompt(input: &RecapInput, language: &str) -> String {
    let mut out = format!(
        "Language: {}\nMeeting: {} on {}\nPeople: {}\n\n",
        language_name(language),
        input.title,
        input.date,
        if input.participants.is_empty() {
            "not listed".into()
        } else {
            input.participants.join(", ")
        }
    );
    for d in &input.decisions {
        out.push_str(&format!("Decision: {d}\n"));
    }
    for a in &input.actions {
        out.push_str(&format!(
            "Action: {} — {}{}\n",
            a.owner.as_deref().unwrap_or("someone"),
            a.body,
            a.due_date
                .as_deref()
                .map(|d| format!(" (by {d})"))
                .unwrap_or_default()
        ));
    }
    out
}

/// Cut over-long text at the last sentence end within [`MAX_WORDS`] words.
pub fn cap(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= MAX_WORDS {
        return words.join(" ");
    }
    let head = words[..MAX_WORDS].join(" ");
    match head.rfind(['.', '।', '!', '?']) {
        Some(i) => head[..=i].to_owned(),
        None => head,
    }
}

pub async fn generate(llm: &dyn Llm, input: &RecapInput, language: &str) -> VoiceResult<String> {
    if input.decisions.is_empty() && input.actions.is_empty() {
        return Err(VoiceError::new(
            VoiceErrorCode::Core,
            "Apply the review first: there is nothing to summarise yet.",
        ));
    }
    let resp = llm
        .complete_json(LlmRequest {
            task: LlmTask::Recap,
            system: SYSTEM.into(),
            messages: vec![LlmMessage { role: LlmRole::User, content: prompt(input, language) }],
            json_schema: json!({"type":"object","required":["recap"],"properties":{"recap":{"type":"string"}}}),
            timeout_ms: default_timeout_ms(LlmTask::Recap),
        })
        .await?;
    let text = cap(resp.json["recap"].as_str().unwrap_or_default());
    if !text.chars().any(char::is_alphabetic) {
        return Err(VoiceError::new(
            VoiceErrorCode::LlmInvalidOutput,
            "The language service gave an answer Pluto couldn't use.",
        ));
    }
    Ok(text)
}

/// The recap as an mp3 (for the export).
pub async fn to_mp3(tts: &dyn TextToSpeech, text: &str, language: &str) -> VoiceResult<Vec<u8>> {
    tts.synthesize(
        TtsRequest {
            text: text.to_owned(),
            language: language.to_owned(),
            voice: None,
            model: None,
            speed: None,
        },
        AudioContainer::Mp3,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_names() {
        assert_eq!(language_name("hi-IN"), "Hindi");
        assert_eq!(language_name("kn-IN"), "Kannada");
        assert_eq!(language_name("en-IN"), "English");
        assert_eq!(language_name("hi-en"), "Hindi");
        assert_eq!(language_name("xx"), "English");
    }

    #[test]
    fn long_text_is_cut_at_a_sentence_end() {
        let sentence = "We agreed to launch on the twentieth of November. ";
        let long = sentence.repeat(30);
        let out = cap(&long);
        assert!(out.split_whitespace().count() <= MAX_WORDS);
        assert!(out.ends_with('.'));
        assert_eq!(cap("  short   text "), "short text");
    }
}
