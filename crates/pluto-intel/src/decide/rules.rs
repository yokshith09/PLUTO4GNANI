//! Zero-cost rules (ARCHITECTURE §8): short answers to a proposal (yes / no / cancel) and fixed
//! focus commands, in English, Hindi, Tamil and Kannada. Anything else returns `None` and goes to
//! the LLM. The rules only fire on short, unambiguous phrases, so a long sentence that merely
//! contains "yes" is never read as an approval.

use pluto_contracts::decide::Intent;
use serde_json::{json, Value};

/// What a rule decided.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleHit {
    pub intent: Intent,
    pub tool: Option<&'static str>,
    pub args: Value,
}

const YES: &[&str] = &[
    "yes", "yeah", "yep", "ok", "okay", "sure", "go", "ahead", "confirm", "do", "it", "please",
    "haan", "han", "ha", "ji", "kar", "do", "theek", "hai", "thik", "bilkul", "sahi", "aama",
    "ama", "sari", "sariye", "pannunga", "pannu", "seri", "houdu", "houdu", "haudu", "maadi",
    "madi", "sare",
];
const NO: &[&str] = &[
    "no", "nope", "nahi", "nahin", "na", "mat", "karo", "rehne", "rahne", "do", "don't", "dont",
    "venam", "vendam", "illa", "beda", "bedi", "vyartha",
];
const CANCEL: &[&str] = &[
    "cancel", "stop", "abort", "roko", "nillisu", "nirutthu", "nillu", "radd",
];
const NEVER_MIND: [&str; 2] = ["never mind", "nevermind"];

const FOCUS_WORDS: &[&str] = &["focus", "pomodoro"];
const STOP_WORDS: &[&str] = &[
    "stop", "end", "band", "khatam", "nillu", "nirutthu", "nillisu", "mudi",
];
const START_WORDS: &[&str] = &[
    "start",
    "begin",
    "shuru",
    "aarambh",
    "aarambhisi",
    "thodangu",
    "podu",
    "maadi",
    "karo",
];
const MINUTE_WORDS: &[&str] = &[
    "minute", "minutes", "min", "mins", "nimisha", "nimidam", "nimisham",
];

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .replace('’', "'")
        .split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

fn all_in(ws: &[String], set: &[&str]) -> bool {
    !ws.is_empty() && ws.iter().all(|w| set.contains(&w.as_str()))
}

fn any_in(ws: &[String], set: &[&str]) -> bool {
    ws.iter().any(|w| set.contains(&w.as_str()))
}

/// Spoken numbers for the focus length (digits are handled first).
fn number_word(w: &str) -> Option<u32> {
    Some(match w {
        "five" | "paanch" | "panch" | "aidhu" | "aydu" => 5,
        "ten" | "das" | "pathu" | "hattu" => 10,
        "fifteen" | "pandrah" | "pathinaindhu" | "hadinaidu" => 15,
        "twenty" | "bees" | "irubadhu" | "ippattu" => 20,
        "pachchis" | "pachees" | "pacchis" => 25,
        "thirty" | "tees" | "muppadhu" | "muvattu" => 30,
        "forty" | "chalis" | "naarpadhu" | "naalvattu" => 40,
        "pachaas" | "fifty" | "aimbadhu" | "aivattu" => 50,
        "sixty" | "saath" | "arubadhu" | "aravattu" => 60,
        _ => return None,
    })
}

fn minutes_in(ws: &[String]) -> Option<u32> {
    let mut found = None;
    for (i, w) in ws.iter().enumerate() {
        let n = w.parse::<u32>().ok().or_else(|| number_word(w));
        if let Some(n) = n {
            // "forty five" -> 45
            let n = match ws
                .get(i + 1)
                .and_then(|next| number_word(next).or_else(|| next.parse().ok()))
            {
                Some(extra) if n % 10 == 0 && n >= 20 && (1..=9).contains(&extra) => n + extra,
                _ => n,
            };
            found = Some(n);
            break;
        }
    }
    found.filter(|m| (1..=240).contains(m))
}

/// Try the rules. `None` means "ask the LLM".
pub fn apply(text: &str) -> Option<RuleHit> {
    let lower = text.to_lowercase();
    let ws = words(text);
    if ws.is_empty() {
        return Some(RuleHit {
            intent: Intent::Noise,
            tool: None,
            args: json!({}),
        });
    }

    // Focus commands (any length: "start a 45 minute focus session").
    if any_in(&ws, FOCUS_WORDS) {
        if any_in(&ws, STOP_WORDS) && minutes_in(&ws).is_none() {
            return Some(RuleHit {
                intent: Intent::Focus,
                tool: Some("focus_stop"),
                args: json!({}),
            });
        }
        if let Some(minutes) = minutes_in(&ws).filter(|_| any_in(&ws, MINUTE_WORDS)) {
            if any_in(&ws, START_WORDS) || !any_in(&ws, STOP_WORDS) {
                return Some(RuleHit {
                    intent: Intent::Focus,
                    tool: Some("focus_start"),
                    args: json!({"minutes": minutes}),
                });
            }
        }
    }

    // Short answers only: at most four words.
    if ws.len() <= 4 {
        let never_mind = NEVER_MIND.iter().any(|p| lower.contains(p));
        if any_in(&ws, CANCEL) || never_mind {
            return Some(RuleHit {
                intent: Intent::Cancel,
                tool: None,
                args: json!({}),
            });
        }
        // "no" must be present for a rejection; "kar do" alone is a yes.
        let has_no = any_in(
            &ws,
            &[
                "no", "nope", "nahi", "nahin", "na", "mat", "venam", "vendam", "illa", "beda",
                "bedi", "don't", "dont",
            ],
        );
        if has_no && all_in(&ws, NO) {
            return Some(RuleHit {
                intent: Intent::Reject,
                tool: None,
                args: json!({}),
            });
        }
        if !has_no && all_in(&ws, YES) {
            return Some(RuleHit {
                intent: Intent::Approval,
                tool: None,
                args: json!({}),
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intent(s: &str) -> Option<Intent> {
        apply(s).map(|h| h.intent)
    }

    #[test]
    fn short_answers_in_four_languages() {
        for s in [
            "Yes go ahead",
            "haan kar do",
            "Aama pannunga",
            "Houdu",
            "okay",
            "theek hai",
        ] {
            assert_eq!(intent(s), Some(Intent::Approval), "{s}");
        }
        for s in ["no", "Nahi rehne do", "venam", "beda"] {
            assert_eq!(intent(s), Some(Intent::Reject), "{s}");
        }
        for s in ["cancel", "No cancel that", "stop", "never mind"] {
            assert_eq!(intent(s), Some(Intent::Cancel), "{s}");
        }
        assert_eq!(intent("   "), Some(Intent::Noise));
    }

    #[test]
    fn long_sentences_are_not_answers() {
        for s in [
            "yes I think the meeting went fine",
            "Hmm okay never mind the weather is nice",
            "kar do aaj ke tasks dikhao",
        ] {
            assert_eq!(intent(s), None, "{s}");
        }
    }

    #[test]
    fn focus_commands() {
        let start = |s: &str| apply(s).map(|h| (h.tool, h.args["minutes"].as_u64()));
        assert_eq!(
            start("Pachchis minute ka focus shuru karo"),
            Some((Some("focus_start"), Some(25)))
        );
        assert_eq!(
            start("Start a 45 minute focus session"),
            Some((Some("focus_start"), Some(45)))
        );
        assert_eq!(
            start("Start a forty five minute focus session"),
            Some((Some("focus_start"), Some(45)))
        );
        assert_eq!(
            start("Hattu nimisha focus shuru maadi"),
            Some((Some("focus_start"), Some(10)))
        );
        assert_eq!(
            start("Stop the focus session"),
            Some((Some("focus_stop"), None))
        );
        assert_eq!(start("Focus band karo"), Some((Some("focus_stop"), None)));
        assert_eq!(apply("how is my focus score"), None);
    }
}
