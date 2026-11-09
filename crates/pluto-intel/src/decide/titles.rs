//! A task title must say what the task is at a glance. The model is asked for a clear phrase; when
//! it still hands back a single word for a longer command ("send" for "add a task to send the
//! budget report to Ravi"), the spoken words themselves are the better title.

/// Words that only address the assistant or introduce the command, not the task.
const LEADS: &[&str] = &[
    "hey pluto",
    "ok pluto",
    "okay pluto",
    "pluto",
    "please",
    "can you",
    "could you",
    "would you",
    "add a new task to",
    "add a new task",
    "add a task to",
    "add a task that",
    "add a task for",
    "add a task",
    "add task to",
    "add task",
    "create a task to",
    "create a task",
    "create task",
    "make a task to",
    "make a task",
    "new task to",
    "new task",
    "put a task to",
    "remind me to",
    "ek task add karo",
    "ek naya task add karo",
    "ek task",
    "task add karo",
    "task bana do",
];
/// Hinglish command endings that are not part of the task.
const TAILS: &[&str] = &[
    "kar do",
    "karo",
    "add karo",
    "add kar do",
    "bana do",
    "please",
];

fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}

/// The command with the assistant-directed words removed and the first letter in capitals; `None`
/// when nothing meaningful is left.
fn clean(spoken: &str) -> Option<String> {
    let mut s = spoken
        .trim()
        .trim_end_matches(['.', '!', '?'])
        .trim()
        .to_owned();
    // Strip leading and trailing filler repeatedly, longest match first ("hey pluto please add a task").
    loop {
        let lower = s.to_lowercase();
        let lead = LEADS
            .iter()
            .filter(|l| lower.starts_with(*l) && boundary(&lower, l.len()))
            .max_by_key(|l| l.len());
        if let Some(l) = lead {
            s = s[l.len()..]
                .trim_start_matches([' ', ',', ':', '-'])
                .to_owned();
            continue;
        }
        let tail = TAILS
            .iter()
            .filter(|t| lower.ends_with(*t) && lower.len() > t.len())
            .max_by_key(|t| t.len());
        if let Some(t) = tail {
            s = s[..s.len() - t.len()]
                .trim_end_matches([' ', ','])
                .to_owned();
            continue;
        }
        break;
    }
    if word_count(&s) < 2 {
        return None;
    }
    let mut chars = s.chars();
    let first = chars.next()?.to_uppercase().collect::<String>();
    let mut out = first + chars.as_str();
    if out.chars().count() > 100 {
        out = out.chars().take(100).collect::<String>();
        if let Some(cut) = out.rfind(' ') {
            out.truncate(cut);
        }
    }
    Some(out)
}

/// A filler phrase ends at a word edge: "ek task" must not eat the front of "ek taskbar".
fn boundary(lower: &str, len: usize) -> bool {
    lower[len..]
        .chars()
        .next()
        .is_none_or(|c| !c.is_alphanumeric())
}

/// The title to use: the model's if it is a real phrase, otherwise the cleaned spoken command.
pub fn clearer_title(title: &str, spoken: &str) -> String {
    if word_count(title) >= 2 || word_count(spoken) < 4 {
        return title.to_owned();
    }
    clean(spoken).unwrap_or_else(|| title.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_real_phrase_from_the_model_is_kept() {
        assert_eq!(
            clearer_title(
                "Send the budget report to Ravi",
                "Pluto add a task to send the budget report to Ravi"
            ),
            "Send the budget report to Ravi"
        );
    }

    #[test]
    fn a_one_word_title_becomes_what_was_said() {
        assert_eq!(
            clearer_title("send", "Pluto add a task to send the budget report to Ravi"),
            "Send the budget report to Ravi"
        );
        assert_eq!(
            clearer_title(
                "Call",
                "hey pluto, please add a task to call the dentist tomorrow"
            ),
            "Call the dentist tomorrow"
        );
        assert_eq!(
            clearer_title("report", "ek task add karo budget report bhejna hai"),
            "Budget report bhejna hai"
        );
        assert_eq!(
            clearer_title("budget", "budget report bhejna hai kar do"),
            "Budget report bhejna hai"
        );
    }

    #[test]
    fn nothing_better_to_offer_keeps_the_model_title() {
        // A short command, or one that is only the command words, has no better title.
        assert_eq!(clearer_title("Milk", "buy milk"), "Milk");
        assert_eq!(clearer_title("task", "pluto add a task please"), "task");
        // "ek taskbar" is not the filler "ek task".
        assert_eq!(
            clearer_title("fix", "ek taskbar fix karna hai aaj"),
            "Ek taskbar fix karna hai aaj"
        );
    }

    #[test]
    fn a_very_long_command_is_cut_at_a_word() {
        let spoken = format!("pluto add a task to {}", "word ".repeat(40));
        let t = clearer_title("word", &spoken);
        assert!(t.chars().count() <= 100, "{t}");
        assert!(!t.ends_with(' '));
    }
}
