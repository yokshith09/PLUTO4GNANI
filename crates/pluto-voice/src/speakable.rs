//! Text as it is spoken. The screen shows dates compactly ("Tue, 10 Nov"); a voice should say them
//! in full ("Tuesday, 10 November"). Applied once, to the text Timbre is asked to speak, so every
//! spoken reply (a proposal, an answer, a recap) gets it and the written text stays short.

const WEEKDAYS: [(&str, &str); 7] = [
    ("Mon", "Monday"),
    ("Tue", "Tuesday"),
    ("Wed", "Wednesday"),
    ("Thu", "Thursday"),
    ("Fri", "Friday"),
    ("Sat", "Saturday"),
    ("Sun", "Sunday"),
];
const MONTHS: [(&str, &str); 12] = [
    ("Jan", "January"),
    ("Feb", "February"),
    ("Mar", "March"),
    ("Apr", "April"),
    ("May", "May"),
    ("Jun", "June"),
    ("Jul", "July"),
    ("Aug", "August"),
    ("Sep", "September"),
    ("Oct", "October"),
    ("Nov", "November"),
    ("Dec", "December"),
];

/// The next character that is not a space.
fn next_non_space(rest: &str) -> Option<char> {
    rest.trim_start_matches(' ').chars().next()
}

/// The last character before a word that is not a space.
fn prev_non_space(before: &str) -> Option<char> {
    before.trim_end_matches(' ').chars().next_back()
}

/// `text` with abbreviated weekdays and months written out when they are part of a date: a weekday
/// followed by a comma or a day number ("Tue, 10 Nov"), a month next to a day number ("10 Nov",
/// "Nov 10"). A word that merely looks like one ("Sun-dried", "Mon" with nothing after it) is left.
pub fn speakable(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut i = 0;
    while i < text.len() {
        let c = text[i..].chars().next().unwrap_or(' ');
        if !c.is_ascii_uppercase() {
            out.push(c);
            i += c.len_utf8();
            continue;
        }
        // A capitalised word starts here; take the whole alphabetic run.
        let end = text[i..]
            .find(|ch: char| !ch.is_ascii_alphabetic())
            .map_or(text.len(), |n| i + n);
        let word = &text[i..end];
        let after = &text[end..];
        // Not the start of a longer word ("Monday", "Marketing"): only the exact abbreviations count.
        let before_is_word = text[..i]
            .chars()
            .next_back()
            .is_some_and(|b| b.is_alphanumeric());
        let replacement = if before_is_word {
            None
        } else if let Some((_, full)) = WEEKDAYS.iter().find(|(a, _)| *a == word) {
            let n = next_non_space(after);
            (n == Some(',') && after.starts_with(',') || n.is_some_and(|d| d.is_ascii_digit()))
                .then_some(*full)
        } else if let Some((_, full)) = MONTHS.iter().find(|(a, _)| *a == word && *a != "May") {
            let before_digit = prev_non_space(&text[..i]).is_some_and(|d| d.is_ascii_digit());
            let after_digit = next_non_space(after).is_some_and(|d| d.is_ascii_digit());
            (before_digit || after_digit).then_some(*full)
        } else {
            None
        };
        out.push_str(replacement.unwrap_or(word));
        i = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::speakable;

    #[test]
    fn dates_are_spoken_in_full() {
        assert_eq!(
            speakable("Add “Send the report” for Tue, 10 Nov?"),
            "Add “Send the report” for Tuesday, 10 November?"
        );
        assert_eq!(
            speakable("Due Sat, 7 Nov, 5:00 PM"),
            "Due Saturday, 7 November, 5:00 PM"
        );
        assert_eq!(
            speakable("Meeting on Fri 14 Nov"),
            "Meeting on Friday 14 November"
        );
        assert_eq!(
            speakable("Dec 3 is the deadline"),
            "December 3 is the deadline"
        );
        assert_eq!(speakable("Wed, Nov 12"), "Wednesday, November 12");
        assert_eq!(
            speakable("Sun, 9 Mar and Thu, 1 Sep"),
            "Sunday, 9 March and Thursday, 1 September"
        );
    }

    #[test]
    fn words_that_only_look_like_dates_are_left_alone() {
        for same in [
            "Sun-dried tomatoes",
            "Mon is a short word",
            "Marketing plan for May",
            "Call Mara about the Monday meeting",
            "Opening Meetings.",
            "Ho gaya. Kaam add kar diya.",
            "Tuesday, 10 November",
            "I can't do that.",
        ] {
            assert_eq!(speakable(same), same, "{same}");
        }
    }

    #[test]
    fn no_text_is_lost_and_non_ascii_survives() {
        assert_eq!(speakable(""), "");
        assert_eq!(
            speakable("नया काम: Tue, 10 Nov"),
            "नया काम: Tuesday, 10 November"
        );
    }
}
