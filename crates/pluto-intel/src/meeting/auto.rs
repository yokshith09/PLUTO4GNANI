//! Auto mode for Google Meet: notice a call in the front browser, offer to record it, record until
//! it ends (or the person says stop), then write the decisions, action items and a short summary
//! into the meeting without a review step.
//!
//! Everything here is logic with no operating system in it, so it can be tested: what a reading of
//! the browser's tabs means ([`observe`]), and what to do next ([`AutoMeet`]). Reading the tabs and
//! recording are the shell's job (`src-tauri/src/auto_meeting.rs`).
//!
//! Two rules keep it honest. It only ever *offers*: recording starts when the person says yes, so
//! nobody is recorded by surprise. And a reading that says nothing (no browser in front) never ends
//! a call, because people switch to other apps during a meeting.

/// One tab as the shell read it. `domain` is known only for the tab that is showing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabSeen {
    pub title: Option<String>,
    pub domain: Option<String>,
    pub active: bool,
}

/// What a reading of the browser says about Google Meet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observation {
    /// A call is open; `code` is its meeting code (`abc-defg-hij`).
    InCall { code: String },
    /// Meet is open on its start or "you left" page: a call that was open has ended.
    Left,
    /// The browser could be read and has no Meet call in it.
    NoCall,
    /// Nothing could be read (no browser in front). Says nothing about the call.
    Unknown,
}

/// The meeting code in a tab title: three letters, four, three, joined by dashes.
fn code_in(title: &str) -> Option<String> {
    let bytes = title.as_bytes();
    for start in 0..bytes.len().saturating_sub(11) {
        let w = &bytes[start..start + 12];
        let shape = [3, 4, 3];
        let mut at = 0;
        let mut ok = true;
        for (i, len) in shape.iter().enumerate() {
            if !w[at..at + len].iter().all(u8::is_ascii_lowercase) {
                ok = false;
                break;
            }
            at += len;
            if i < 2 {
                if w[at] != b'-' {
                    ok = false;
                    break;
                }
                at += 1;
            }
        }
        // Not inside a longer word.
        let before = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
        let after = start + 12 >= bytes.len() || !bytes[start + 12].is_ascii_alphanumeric();
        if ok && before && after {
            return Some(String::from_utf8_lossy(w).into_owned());
        }
    }
    None
}

/// Titles Meet shows when no call is open.
fn is_meet_home(title: &str) -> bool {
    let t = title.trim().to_lowercase();
    t == "meet" || t == "google meet" || t.starts_with("meet - google") || t == "meet - video calls"
}

/// What a reading of the tabs means for Google Meet.
pub fn observe(tabs: &[TabSeen]) -> Observation {
    if tabs.is_empty() {
        return Observation::Unknown;
    }
    let on_meet = |t: &TabSeen| t.domain.as_deref() == Some("meet.google.com");
    // A call: a tab titled with a meeting code, or the showing tab on meet.google.com with one.
    for t in tabs {
        let title = t.title.as_deref().unwrap_or("");
        let looks_like_meet = title.to_lowercase().contains("meet") || on_meet(t);
        if looks_like_meet {
            if let Some(code) = code_in(title) {
                return Observation::InCall { code };
            }
        }
    }
    if tabs
        .iter()
        .any(|t| on_meet(t) || t.title.as_deref().is_some_and(is_meet_home))
    {
        return Observation::Left;
    }
    Observation::NoCall
}

/// How the person answered an offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Record,
    NotNow,
}

/// What the shell tells the machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Saw(Observation),
    Answered(Answer),
    /// The Stop button on the pet.
    StopPressed,
    /// Starting the recording did not work (no microphone, no key, ...).
    StartFailed,
}

/// What the shell should do next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Nothing,
    /// Ask whether to record this call.
    Offer {
        code: String,
    },
    /// The call went away before an answer: take the question down.
    WithdrawOffer,
    StartRecording {
        code: String,
    },
    StopRecording,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Offered { code: String },
    Recording { code: String },
}

/// Readings of a call in a row before it is offered (about 6 s at one reading per 3 s).
pub const OFFER_AFTER: u32 = 2;
/// Readings in a row with a browser in front and no call before a recording is ended (about 30 s).
pub const END_AFTER: u32 = 10;
/// The same, when Meet itself shows its start or "you left" page (about 6 s).
pub const END_AFTER_LEFT: u32 = 2;

#[derive(Debug, Clone)]
pub struct AutoMeet {
    phase: Phase,
    /// Readings in a row that saw a call, while idle.
    seen: u32,
    /// Readings in a row that saw the call gone, while offered or recording.
    gone: u32,
    /// A call the person said "not now" to (or that was stopped): not offered again.
    declined: Option<String>,
}

impl Default for AutoMeet {
    fn default() -> Self {
        Self::new()
    }
}

impl AutoMeet {
    pub fn new() -> Self {
        Self {
            phase: Phase::Idle,
            seen: 0,
            gone: 0,
            declined: None,
        }
    }

    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    fn back_to_idle(&mut self) {
        self.phase = Phase::Idle;
        self.seen = 0;
        self.gone = 0;
    }

    pub fn on(&mut self, event: Event) -> Action {
        match event {
            Event::Saw(obs) => self.saw(obs),
            Event::Answered(answer) => match (&self.phase, answer) {
                (Phase::Offered { code }, Answer::Record) => {
                    let code = code.clone();
                    self.phase = Phase::Recording { code: code.clone() };
                    self.gone = 0;
                    Action::StartRecording { code }
                }
                (Phase::Offered { code }, Answer::NotNow) => {
                    self.declined = Some(code.clone());
                    self.back_to_idle();
                    Action::Nothing
                }
                _ => Action::Nothing,
            },
            Event::StopPressed => match &self.phase {
                Phase::Recording { code } => {
                    // The same call is not offered again straight away.
                    self.declined = Some(code.clone());
                    self.back_to_idle();
                    Action::StopRecording
                }
                _ => Action::Nothing,
            },
            Event::StartFailed => {
                if let Phase::Recording { code } = &self.phase {
                    self.declined = Some(code.clone());
                }
                self.back_to_idle();
                Action::Nothing
            }
        }
    }

    fn saw(&mut self, obs: Observation) -> Action {
        // Nothing read says nothing: it neither starts, offers, nor ends anything.
        if obs == Observation::Unknown {
            return Action::Nothing;
        }
        match self.phase.clone() {
            Phase::Idle => match obs {
                Observation::InCall { code } => {
                    if self.declined.as_deref() == Some(code.as_str()) {
                        return Action::Nothing;
                    }
                    self.seen += 1;
                    if self.seen >= OFFER_AFTER {
                        self.phase = Phase::Offered { code: code.clone() };
                        self.gone = 0;
                        return Action::Offer { code };
                    }
                    Action::Nothing
                }
                _ => {
                    // The call is over: the next one may be offered.
                    self.seen = 0;
                    self.declined = None;
                    Action::Nothing
                }
            },
            Phase::Offered { .. } => match obs {
                Observation::InCall { .. } => {
                    self.gone = 0;
                    Action::Nothing
                }
                other => {
                    self.gone += 1;
                    let limit = if other == Observation::Left {
                        END_AFTER_LEFT
                    } else {
                        END_AFTER
                    };
                    if self.gone >= limit {
                        self.back_to_idle();
                        return Action::WithdrawOffer;
                    }
                    Action::Nothing
                }
            },
            Phase::Recording { .. } => match obs {
                Observation::InCall { .. } => {
                    self.gone = 0;
                    Action::Nothing
                }
                other => {
                    self.gone += 1;
                    let limit = if other == Observation::Left {
                        END_AFTER_LEFT
                    } else {
                        END_AFTER
                    };
                    if self.gone >= limit {
                        self.back_to_idle();
                        return Action::StopRecording;
                    }
                    Action::Nothing
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(title: &str, domain: Option<&str>, active: bool) -> TabSeen {
        TabSeen {
            title: Some(title.into()),
            domain: domain.map(Into::into),
            active,
        }
    }
    fn call(code: &str) -> Observation {
        Observation::InCall { code: code.into() }
    }

    #[test]
    fn a_call_is_found_by_its_code_in_any_tab() {
        let tabs = [
            tab("Inbox - Mail", None, false),
            tab("Meet - abc-defg-hij", Some("meet.google.com"), true),
        ];
        assert_eq!(observe(&tabs), call("abc-defg-hij"));
        // A background tab has no domain but its title still says so.
        let bg = [
            tab("Docs", Some("docs.google.com"), true),
            tab("Meet - abc-defg-hij", None, false),
        ];
        assert_eq!(observe(&bg), call("abc-defg-hij"));
        assert_eq!(
            observe(&[tab("Sprint planning – Meet", None, false)]),
            Observation::NoCall
        );
        assert_eq!(
            observe(&[tab("Sprint – Meet abc-defg-hij", None, false)]),
            call("abc-defg-hij")
        );
    }

    #[test]
    fn other_pages_with_code_like_text_are_not_calls() {
        // A code-shaped word on a page that has nothing to do with Meet.
        assert_eq!(
            observe(&[tab("abc-defg-hij notes", Some("notion.so"), true)]),
            Observation::NoCall
        );
        // Not inside a longer word.
        assert_eq!(
            observe(&[tab("Meet - xabc-defg-hijx", Some("meet.google.com"), true)]),
            Observation::Left
        );
    }

    #[test]
    fn meet_without_a_call_means_the_call_is_over() {
        assert_eq!(
            observe(&[tab("Meet", Some("meet.google.com"), true)]),
            Observation::Left
        );
        assert_eq!(
            observe(&[tab("Google Meet", None, false)]),
            Observation::Left
        );
        assert_eq!(
            observe(&[tab("Docs", Some("docs.google.com"), true)]),
            Observation::NoCall
        );
        assert_eq!(observe(&[]), Observation::Unknown, "nothing read");
    }

    #[test]
    fn a_call_is_offered_once_it_has_been_seen_twice_and_recorded_only_on_yes() {
        let mut m = AutoMeet::new();
        assert_eq!(m.on(Event::Saw(call("abc-defg-hij"))), Action::Nothing);
        assert_eq!(
            m.on(Event::Saw(call("abc-defg-hij"))),
            Action::Offer {
                code: "abc-defg-hij".into()
            }
        );
        // Still just offered while it is on screen: nothing is recorded without a yes.
        for _ in 0..30 {
            assert_eq!(m.on(Event::Saw(call("abc-defg-hij"))), Action::Nothing);
        }
        assert_eq!(
            m.on(Event::Answered(Answer::Record)),
            Action::StartRecording {
                code: "abc-defg-hij".into()
            }
        );
        assert!(matches!(m.phase(), Phase::Recording { .. }));
    }

    #[test]
    fn not_now_is_remembered_for_that_call_only() {
        let mut m = AutoMeet::new();
        m.on(Event::Saw(call("aaa-bbbb-ccc")));
        m.on(Event::Saw(call("aaa-bbbb-ccc")));
        assert_eq!(m.on(Event::Answered(Answer::NotNow)), Action::Nothing);
        for _ in 0..10 {
            assert_eq!(
                m.on(Event::Saw(call("aaa-bbbb-ccc"))),
                Action::Nothing,
                "not asked again"
            );
        }
        // A different call is offered; so is the same code after the call has been left.
        m.on(Event::Saw(call("ddd-eeee-fff")));
        assert!(matches!(
            m.on(Event::Saw(call("ddd-eeee-fff"))),
            Action::Offer { .. }
        ));
        let mut m = AutoMeet::new();
        m.on(Event::Saw(call("aaa-bbbb-ccc")));
        m.on(Event::Saw(call("aaa-bbbb-ccc")));
        m.on(Event::Answered(Answer::NotNow));
        m.on(Event::Saw(Observation::Left));
        m.on(Event::Saw(call("aaa-bbbb-ccc")));
        assert!(matches!(
            m.on(Event::Saw(call("aaa-bbbb-ccc"))),
            Action::Offer { .. }
        ));
    }

    #[test]
    fn a_recording_ends_when_the_call_is_gone_for_long_enough_but_not_when_nothing_could_be_read() {
        let mut m = AutoMeet::new();
        m.on(Event::Saw(call("abc-defg-hij")));
        m.on(Event::Saw(call("abc-defg-hij")));
        m.on(Event::Answered(Answer::Record));
        // Switching to another app: nothing read, for as long as it takes.
        for _ in 0..100 {
            assert_eq!(m.on(Event::Saw(Observation::Unknown)), Action::Nothing);
        }
        // Another browser window with no call: only after END_AFTER readings in a row.
        for _ in 0..END_AFTER - 1 {
            assert_eq!(m.on(Event::Saw(Observation::NoCall)), Action::Nothing);
        }
        // The call came back: the count starts over.
        assert_eq!(m.on(Event::Saw(call("abc-defg-hij"))), Action::Nothing);
        for _ in 0..END_AFTER - 1 {
            assert_eq!(m.on(Event::Saw(Observation::NoCall)), Action::Nothing);
        }
        assert_eq!(m.on(Event::Saw(Observation::NoCall)), Action::StopRecording);
        assert_eq!(m.phase(), &Phase::Idle);
    }

    #[test]
    fn leaving_the_call_ends_it_quickly() {
        let mut m = AutoMeet::new();
        m.on(Event::Saw(call("abc-defg-hij")));
        m.on(Event::Saw(call("abc-defg-hij")));
        m.on(Event::Answered(Answer::Record));
        assert_eq!(m.on(Event::Saw(Observation::Left)), Action::Nothing);
        assert_eq!(m.on(Event::Saw(Observation::Left)), Action::StopRecording);
    }

    #[test]
    fn stop_on_the_pet_ends_it_and_the_same_call_is_not_offered_again() {
        let mut m = AutoMeet::new();
        m.on(Event::Saw(call("abc-defg-hij")));
        m.on(Event::Saw(call("abc-defg-hij")));
        m.on(Event::Answered(Answer::Record));
        assert_eq!(m.on(Event::StopPressed), Action::StopRecording);
        for _ in 0..5 {
            assert_eq!(m.on(Event::Saw(call("abc-defg-hij"))), Action::Nothing);
        }
        // Stop with nothing recording does nothing.
        assert_eq!(AutoMeet::new().on(Event::StopPressed), Action::Nothing);
    }

    #[test]
    fn an_offer_is_taken_down_when_the_call_goes_away_and_a_failed_start_is_not_retried() {
        let mut m = AutoMeet::new();
        m.on(Event::Saw(call("abc-defg-hij")));
        m.on(Event::Saw(call("abc-defg-hij")));
        assert_eq!(m.on(Event::Saw(Observation::Left)), Action::Nothing);
        assert_eq!(m.on(Event::Saw(Observation::Left)), Action::WithdrawOffer);
        assert_eq!(m.phase(), &Phase::Idle);

        let mut m = AutoMeet::new();
        m.on(Event::Saw(call("abc-defg-hij")));
        m.on(Event::Saw(call("abc-defg-hij")));
        m.on(Event::Answered(Answer::Record));
        m.on(Event::StartFailed);
        assert_eq!(m.on(Event::Saw(call("abc-defg-hij"))), Action::Nothing);
        assert_eq!(m.phase(), &Phase::Idle);
    }
}
