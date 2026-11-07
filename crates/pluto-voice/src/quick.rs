//! Instant answers for the commonest commands, decided on this computer without the LLM.
//!
//! Owner: **D1**. Opening a page and reading the task or reminder lists are a few fixed phrases
//! in English and Hinglish; matching them here saves the LLM round trip (about 1–1.5 s,
//! `docs/results/d2-llm-check.md`). Only a whole command matches, after polite filler ("can
//! you", "please") is dropped; anything else, or anything longer, goes to the decider as before.

use pluto_contracts::decide::Intent;
use pluto_contracts::voice::{pages, tools};
use serde_json::{json, Value};

/// Lower case, letters and digits only, single spaces, polite filler removed.
fn normalise(text: &str) -> String {
    let lower = text.to_lowercase().replace(['\u{2019}', '\''], "");
    let words: Vec<&str> = lower
        .split(|c: char| !(c.is_alphanumeric()))
        .filter(|w| !w.is_empty())
        .collect();
    let mut s = words.join(" ");
    let fillers_front = [
        "can you please ",
        "could you please ",
        "can you ",
        "could you ",
        "will you ",
        "would you ",
        "please ",
        "hey ",
        "okay ",
        "ok ",
        "just ",
        "zara ",
    ];
    let fillers_back = [" please", " for me", " now", " na", " zara"];
    loop {
        let before = s.clone();
        for f in fillers_front {
            if let Some(rest) = s.strip_prefix(f) {
                s = rest.to_string();
            }
        }
        for f in fillers_back {
            if let Some(rest) = s.strip_suffix(f) {
                s = rest.to_string();
            }
        }
        if s == before {
            return s;
        }
    }
}

/// "notes", "the notes tab", "my meetings page" → the page.
fn page(words: &str) -> Option<&'static str> {
    let w = words.trim();
    let w = w.strip_prefix("my ").unwrap_or(w);
    let w = w.strip_prefix("the ").unwrap_or(w);
    pages::named(w)
}

fn navigation(s: &str) -> Option<&'static str> {
    const BEFORE: [&str; 11] = [
        "open up ",
        "open ",
        "go to ",
        "goto ",
        "take me to ",
        "switch to ",
        "navigate to ",
        "jump to ",
        "show me ",
        "show ",
        "move to ",
    ];
    const AFTER: [&str; 10] = [
        " kholo",
        " khol do",
        " kholiye",
        " open karo",
        " open kar do",
        " dikhao",
        " pe jao",
        " par jao",
        " pe chalo",
        " par chalo",
    ];
    BEFORE
        .iter()
        .find_map(|v| s.strip_prefix(v).and_then(page))
        .or_else(|| AFTER.iter().find_map(|v| s.strip_suffix(v).and_then(page)))
}

fn task_list(s: &str) -> Option<&'static str> {
    const TODAY: [&str; 13] = [
        "aaj ke kaam batao",
        "aaj ke kaam dikhao",
        "aaj ke kaam",
        "aaj kya kaam hai",
        "aaj kya karna hai",
        "what are my tasks today",
        "what are my tasks for today",
        "what are todays tasks",
        "todays tasks",
        "my tasks for today",
        "tasks for today",
        "what do i have today",
        "whats on today",
    ];
    const OVERDUE: [&str; 7] = [
        "whats overdue",
        "what is overdue",
        "overdue tasks",
        "my overdue tasks",
        "what tasks are overdue",
        "kaun se kaam late hain",
        "late kaam batao",
    ];
    const ALL: [&str; 5] = [
        "all my tasks",
        "list all my tasks",
        "list all tasks",
        "what are all my tasks",
        "sab kaam batao",
    ];
    if TODAY.contains(&s) {
        Some("today")
    } else if OVERDUE.contains(&s) {
        Some("overdue")
    } else if ALL.contains(&s) {
        Some("all")
    } else {
        None
    }
}

fn reminder_list(s: &str) -> bool {
    [
        "what are my reminders",
        "my reminders",
        "list my reminders",
        "reminders batao",
        "mere reminders batao",
        "kaun se reminders hain",
    ]
    .contains(&s)
}

/// The decision for a fixed command: `(intent, tool, args)`, or `None` for the decider.
pub fn decide(text: &str) -> Option<(Intent, &'static str, Value)> {
    let s = normalise(text);
    if s.split(' ').count() > 8 {
        return None;
    }
    if let Some(view) = task_list(&s) {
        return Some((Intent::Task, tools::TASKS_LIST, json!({"view": view})));
    }
    if reminder_list(&s) {
        return Some((Intent::Reminder, tools::REMINDERS_LIST, json!({})));
    }
    navigation(&s).map(|p| (Intent::Navigate, tools::OPEN_PAGE, json!({"page": p})))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page_of(text: &str) -> Option<String> {
        decide(text)
            .filter(|(_, tool, _)| *tool == tools::OPEN_PAGE)
            .map(|(_, _, a)| a["page"].as_str().unwrap().to_string())
    }

    #[test]
    fn spoken_page_requests_open_the_page_at_once() {
        // From the user's screen recording (Prisma transcripts), and the usual forms.
        for (said, page) in [
            ("Can you open notes tab?", "notes"),
            ("Open Task Tab.", "tasks"),
            ("Can you go to character tab?", "character"),
            ("meetings kholo", "meetings"),
            ("Settings khol do", "settings"),
            ("go to the settings page please", "settings"),
            ("take me to my notes", "notes"),
            ("show me the bin", "bin"),
            ("home pe jao", "today"),
        ] {
            assert_eq!(page_of(said).as_deref(), Some(page), "{said}");
        }
    }

    #[test]
    fn task_and_reminder_lists_are_read_at_once() {
        for (said, view) in [
            ("Aaj ke kaam batao", "today"),
            ("What are my tasks for today?", "today"),
            ("What's overdue?", "overdue"),
            ("list all my tasks", "all"),
        ] {
            let (_, tool, args) = decide(said).unwrap();
            assert_eq!(
                (tool, args["view"].as_str()),
                (tools::TASKS_LIST, Some(view)),
                "{said}"
            );
        }
        assert_eq!(
            decide("What are my reminders?").unwrap().1,
            tools::REMINDERS_LIST
        );
    }

    #[test]
    fn anything_else_goes_to_the_decider() {
        for said in [
            "add a task to call Ravi tomorrow",
            "open a new note about the budget",
            "show me what Priya said about the login fix",
            "go to the kitchen",
            "remind me at 5 pm",
            "haan",
            "usse Friday kar do",
            "can you open notes tab and add a task to call mom tomorrow evening please",
        ] {
            assert!(decide(said).is_none(), "{said}");
        }
    }
}
