//! Assistant (T-303): tool registry over core functions, proposals (30 s), approval by voice or
//! click, 6-turn context, interaction log. **The only place writes are approved.**
//!
//! Owner: **D1**. One push-to-talk turn ([`TurnHandler::turn`]):
//!
//! 1. The words are logged (`voice_interactions`) and decided by D2's [`Decider`] (rules, then
//!    the intent LLM) with the last 6 turns as context, so "usse Friday kar do" refers to the
//!    task of the previous turn.
//! 2. **Reads** (`tasks_list`, `reminders_list`, `meeting_search`) run at once and the answer is
//!    spoken (lists: at most 5 items, then "and N more"). `open_page` ("meetings kholo") also
//!    runs at once: it switches the dashboard page and changes no data.
//! 3. **Writes** (`task_create`, `task_complete`, `task_update`, `reminder_create`,
//!    `reminder_done`) become a [`Proposal`]: stored, shown and spoken, waiting 30 s. Nothing
//!    runs until [`Assistant::respond`] gets an approval, by click (`voice_proposal_respond`) or
//!    by voice ("haan", "yes"). Several matching tasks → the proposal lists up to 3 and
//!    "pehla / doosra / first / second" picks one. The gate itself is the single conditional
//!    UPDATE in `voice_store::decide_proposal`, so a proposal runs at most once (T-AS-05).
//!
//! Replies are in Hindi (romanised, as Timbre speaks it) or English, following the detected
//! language; Tamil and Kannada fall back to English for now.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, Weak};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{NaiveDate, TimeZone};
use pluto_contracts::decide::{AssistantDecision, DecideContext, Decider, DecisionSource, Intent};
use pluto_contracts::error::{VoiceError, VoiceErrorCode, VoiceResult};
use pluto_contracts::meeting::MeetingKnowledge;
use pluto_contracts::voice::{
    pages, tools, AssistantTurn, Proposal, ProposalRequest, ProposalResponse, ToolExecutor,
    ToolResult,
};
use pluto_core::clock::Clock;
use pluto_core::db::Database;
use pluto_core::ids::new_id;
use pluto_core::meeting_ai_store;
use pluto_core::meetings;
use pluto_core::reminders::{self, Reminder, ReminderInput};
use pluto_core::settings_service::SettingsService;
use pluto_core::tasks::{self, TaskFilters, TaskInput, TaskPatch, TaskStatus, TaskView};
use pluto_core::voice_store::{
    self, Answer, Decided, InteractionRow, InteractionUpdate, ProposalRow,
};
use serde_json::{json, Value};

use crate::runtime::{Heard, Turn, TurnHandler, VoiceRuntime};

/// How long a proposal waits for an answer.
pub const PROPOSAL_WINDOW: Duration = Duration::from_secs(30);
/// Turns of context given to the decider.
pub const CONTEXT_TURNS: usize = 6;
/// Items read out of a list before "and N more".
pub const LIST_SPOKEN: usize = 5;
/// Matching tasks offered when a command is ambiguous.
pub const CANDIDATES: usize = 3;
const HISTORY: usize = 50;

/// Where the assistant reports (`voice://turn`, `voice://proposal`).
pub trait AssistantEvents: Send + Sync {
    fn turn(&self, turn: &AssistantTurn);
    fn proposal(&self, proposal: &Proposal);
    /// Switch the dashboard to `page` (one of `pages::ALL`), bringing the window up.
    fn navigate(&self, _page: &str) {}
}

// ---- language --------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    Hi,
    En,
}

impl Lang {
    fn of(code: Option<&str>) -> Self {
        match code.map(str::to_ascii_lowercase) {
            Some(c) if c.starts_with("hi") || c.starts_with("mr") => Self::Hi,
            _ => Self::En,
        }
    }

    fn code(self) -> &'static str {
        match self {
            Self::Hi => "hi-IN",
            Self::En => "en-IN",
        }
    }

    fn pick(self, hi: impl Into<String>, en: impl Into<String>) -> String {
        match self {
            Self::Hi => hi.into(),
            Self::En => en.into(),
        }
    }
}

/// "A, B and 3 more" (at most [`LIST_SPOKEN`] read out).
fn spoken_list(items: &[String], lang: Lang) -> String {
    let shown: Vec<&str> = items.iter().take(LIST_SPOKEN).map(String::as_str).collect();
    let mut out = shown.join(", ");
    let more = items.len().saturating_sub(LIST_SPOKEN);
    if more > 0 {
        out.push_str(&lang.pick(format!(", aur {more} aur"), format!(", and {more} more")));
    }
    out
}

/// The page's name as the sidebar shows it.
fn page_title(page: &str) -> &'static str {
    match page {
        "today" => "Today",
        "tasks" => "Tasks",
        "time" => "Time",
        "notes" => "Notes",
        "projects" => "Projects",
        "meetings" => "Meetings",
        "voice" => "Voice",
        "archive" => "Archive",
        "bin" => "Bin",
        "character" => "Character",
        "mcp" => "MCP",
        "settings" => "Settings",
        _ => "That",
    }
}

/// "Sat, 7 Nov" for `2026-11-07` (as it is read out), or the text itself if it isn't a date.
pub fn friendly_date(d: &str) -> String {
    NaiveDate::parse_from_str(d, "%Y-%m-%d")
        .map(|d| d.format("%a, %-d %b").to_string())
        .unwrap_or_else(|_| d.to_string())
}

fn capitalise(s: &str) -> String {
    let mut c = s.trim().chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// A clock time in a phrase: "5 pm", "5:30 PM", "6 a.m.", "17:00", "5 baje", "five pm".
/// `None` when the phrase has no time.
pub fn clock_time(phrase: &str) -> Option<(u32, u32)> {
    let p = phrase
        .to_lowercase()
        .replace("a.m.", "am")
        .replace("p.m.", "pm");
    const NUMBERS: [&str; 12] = [
        "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven",
        "twelve",
    ];
    let tokens: Vec<String> = p
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect();
    for (i, tok) in tokens.iter().enumerate() {
        // Split "5pm" / "5:30pm" into number and suffix.
        let (num, mut suffix) = match tok.find(|c: char| c.is_ascii_alphabetic()) {
            Some(at) if at > 0 => (tok[..at].to_string(), tok[at..].to_string()),
            _ => (tok.clone(), String::new()),
        };
        let (h, m) = if let Some((h, m)) = num.split_once(':') {
            (h.parse::<u32>().ok(), m.parse::<u32>().ok())
        } else if let Ok(h) = num.parse::<u32>() {
            (Some(h), Some(0))
        } else if let Some(n) = NUMBERS.iter().position(|w| *w == num) {
            (Some(n as u32 + 1), Some(0))
        } else {
            (None, None)
        };
        let (Some(mut h), Some(m)) = (h, m) else {
            continue;
        };
        if suffix.is_empty() {
            suffix = tokens.get(i + 1).cloned().unwrap_or_default();
        }
        let has_colon = num.contains(':');
        match suffix.as_str() {
            "pm" | "p" | "shaam" | "raat" if (1..=12).contains(&h) => {
                if h != 12 {
                    h += 12;
                }
            }
            "am" | "a" | "subah" if (1..=12).contains(&h) => {
                if h == 12 {
                    h = 0;
                }
            }
            "baje" | "o'clock" | "oclock" | "o" if (1..=12).contains(&h) => {
                // "5 baje" with no am/pm: a working-hours guess, 1–7 → afternoon/evening.
                if (1..=7).contains(&h) {
                    h += 12;
                }
            }
            _ if has_colon && h < 24 => {}
            _ => continue,
        }
        if h < 24 && m < 60 {
            return Some((h, m));
        }
    }
    None
}

/// "pehla", "doosra", "first", "2", … → 0-based choice.
pub fn ordinal(text: &str) -> Option<usize> {
    let t = text.trim().to_lowercase();
    let t = t.trim_end_matches(['.', '!', '?', '।']);
    let words: Vec<&str> = t.split_whitespace().collect();
    let first = *words.first()?;
    // Only short answers ("pehla", "pehla wala", "the second one") count as a choice.
    if words.len() > 3 {
        return None;
    }
    let n = match first {
        "pehla" | "pehle" | "pehli" | "first" | "1" | "one" | "ek" | "पहला" | "पहले" => {
            0
        }
        "doosra" | "dusra" | "doosre" | "dusre" | "doosri" | "second" | "2" | "two" | "do"
        | "दूसरा" => 1,
        "teesra" | "tisra" | "teesre" | "third" | "3" | "three" | "teen" | "तीसरा" => 2,
        "the" => return ordinal(&words[1..].join(" ")),
        _ => return None,
    };
    Some(n)
}

// ---- remembered context ------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Remembered {
    transcript: String,
    tool: Option<String>,
    /// The task this turn was about (for "usse", "it").
    task: Option<(String, String)>,
    reminder: Option<(String, String)>,
}

impl Remembered {
    fn line(&self) -> String {
        let mut s = format!("User: {}", self.transcript);
        if let Some(t) = &self.tool {
            s.push_str(&format!(" → {t}"));
        }
        if let Some((_, title)) = &self.task {
            s.push_str(&format!(" (task: \"{title}\")"));
        }
        if let Some((_, title)) = &self.reminder {
            s.push_str(&format!(" (reminder: \"{title}\")"));
        }
        s
    }
}

/// Words that point back at the previous turn's item.
fn is_reference(m: &str) -> bool {
    let m = m.trim().to_lowercase();
    m.is_empty()
        || [
            "usse",
            "use",
            "usko",
            "isse",
            "ise",
            "isko",
            "woh",
            "wo",
            "vo",
            "it",
            "that",
            "this",
            "that one",
            "this one",
            "same",
            "the same",
            "wahi",
            "vahi",
            "उसे",
            "इसे",
            "वो",
        ]
        .contains(&m.as_str())
}

// ---- matching ----------------------------------------------------------------------------------

fn words(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() > 1)
        .map(str::to_string)
        .collect()
}

/// The best matches for `query` among `(id, title)` items: every item sharing the most words
/// with the query (case-insensitive), best first. Empty when nothing shares a word.
fn best_matches(query: &str, items: &[(String, String)]) -> Vec<(String, String)> {
    let q = words(query);
    if q.is_empty() {
        return vec![];
    }
    let mut scored: Vec<(usize, &(String, String))> = items
        .iter()
        .map(|it| {
            let t = words(&it.1);
            let hits = q
                .iter()
                .filter(|w| {
                    t.iter()
                        .any(|tw| tw.starts_with(w.as_str()) || w.starts_with(tw.as_str()))
                })
                .count();
            (hits, it)
        })
        .filter(|(h, _)| *h > 0)
        .collect();
    let Some(best) = scored.iter().map(|(h, _)| *h).max() else {
        return vec![];
    };
    scored.retain(|(h, _)| *h == best);
    scored.into_iter().map(|(_, it)| it.clone()).collect()
}

// ---- the assistant -----------------------------------------------------------------------------

struct Pending {
    proposal: Proposal,
    /// Choices when the command matched several items ("pehla / doosra").
    candidates: Vec<(String, String)>,
}

#[derive(Default)]
struct State {
    context: VecDeque<Remembered>,
    pending: Option<Pending>,
    history: VecDeque<AssistantTurn>,
}

pub struct Assistant {
    db: Arc<Database>,
    clock: Arc<dyn Clock>,
    decider: Arc<dyn Decider>,
    events: Arc<dyn AssistantEvents>,
    window: Duration,
    runtime: OnceLock<Weak<VoiceRuntime>>,
    state: Mutex<State>,
    /// Grounded meeting answers (T-305); without it `meeting_search` reads out matching quotes.
    knowledge: Option<Arc<dyn MeetingKnowledge>>,
}

impl Assistant {
    pub fn new(
        db: Arc<Database>,
        clock: Arc<dyn Clock>,
        decider: Arc<dyn Decider>,
        events: Arc<dyn AssistantEvents>,
    ) -> Self {
        Self {
            db,
            clock,
            decider,
            events,
            window: PROPOSAL_WINDOW,
            runtime: OnceLock::new(),
            state: Mutex::default(),
            knowledge: None,
        }
    }

    /// Lets spoken questions about meetings use the grounded recall (T-305).
    pub fn with_knowledge(mut self, knowledge: Arc<dyn MeetingKnowledge>) -> Self {
        self.knowledge = Some(knowledge);
        self
    }

    pub fn with_window(mut self, window: Duration) -> Self {
        self.window = window;
        self
    }

    /// Connects the runtime, so a click approval or an expiry moves it on (AwaitingApproval →
    /// Executing → Idle). Called once by the shell after both exist.
    pub fn attach(&self, runtime: &Arc<VoiceRuntime>) {
        let _ = self.runtime.set(Arc::downgrade(runtime));
    }

    fn rt(&self) -> Option<Arc<VoiceRuntime>> {
        self.runtime.get().and_then(Weak::upgrade)
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The newest turns first (`voice_history`). Kept in memory for this run of the app; every
    /// turn is also in `voice_interactions`.
    pub fn history(&self, limit: usize) -> Vec<AssistantTurn> {
        self.lock().history.iter().take(limit).cloned().collect()
    }

    /// The proposal waiting for an answer, if any.
    pub fn pending(&self) -> Option<Proposal> {
        self.lock().pending.as_ref().map(|p| p.proposal.clone())
    }

    fn record(&self, turn: &AssistantTurn) {
        {
            let mut st = self.lock();
            st.history
                .retain(|t| t.interaction_id != turn.interaction_id);
            st.history.push_front(turn.clone());
            st.history.truncate(HISTORY);
        }
        self.events.turn(turn);
    }

    fn remember(&self, r: Remembered) {
        let mut st = self.lock();
        st.context.push_back(r);
        while st.context.len() > CONTEXT_TURNS {
            st.context.pop_front();
        }
    }

    fn last_item(&self, reminder: bool) -> Option<(String, String)> {
        self.lock().context.iter().rev().find_map(|r| {
            if reminder {
                r.reminder.clone()
            } else {
                r.task.clone()
            }
        })
    }

    async fn log_update(&self, interaction_id: &str, update: InteractionUpdate) {
        let id = interaction_id.to_string();
        let written = self
            .db
            .write(move |tx| {
                voice_store::update_interaction(tx, &id, &update)?;
                Ok(((), vec![]))
            })
            .await;
        if let Err(e) = written {
            tracing::warn!(error = %e.message, "couldn't update the interaction log");
        }
    }

    async fn context(&self) -> DecideContext {
        let recent_turns = self.lock().context.iter().map(Remembered::line).collect();
        let open_task_titles = self
            .open_tasks()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|(_, t)| t)
            .take(20)
            .collect();
        DecideContext {
            recent_turns,
            open_task_titles,
            participants: vec![],
            timezone: self.clock.tz().name().to_string(),
            now_ms: self.clock.now_ms(),
        }
    }

    async fn open_tasks(&self) -> VoiceResult<Vec<(String, String)>> {
        let rows = tasks::query(
            &self.db,
            &*self.clock,
            TaskView::All,
            TaskFilters::default(),
        )
        .await?;
        Ok(rows
            .into_iter()
            .filter(|r| !matches!(r.task.status, TaskStatus::Completed | TaskStatus::Cancelled))
            .map(|r| (r.task.id, r.task.title))
            .collect())
    }

    async fn open_reminders(&self) -> VoiceResult<Vec<Reminder>> {
        Ok(reminders::list(&self.db, false)
            .await?
            .into_iter()
            .filter(|r| r.done_at.is_none())
            .collect())
    }

    /// `day` at `h:m` local time, in ms.
    fn local_ms(&self, day: NaiveDate, h: u32, m: u32) -> Option<i64> {
        let t = day.and_hms_opt(h, m, 0)?;
        self.clock
            .tz()
            .from_local_datetime(&t)
            .earliest()
            .map(|d| d.timestamp_millis())
    }

    /// The demo view (T-402) hides some pages; voice doesn't open them either.
    async fn demo_pages_only(&self) -> bool {
        SettingsService::new(self.db.clone(), self.clock.clone())
            .get_all()
            .await
            .ok()
            .and_then(|all| all.get("advanced.demo_pages_only").and_then(Value::as_bool))
            .unwrap_or(false)
    }

    fn time_text(&self, ms: i64) -> String {
        let tz = self.clock.tz();
        let Some(at) = tz.timestamp_millis_opt(ms).single() else {
            return String::new();
        };
        let today = self.clock.local_date(self.clock.now_ms());
        if at.date_naive() == today {
            at.format("%-I:%M %p").to_string()
        } else {
            at.format("%a, %-d %b, %-I:%M %p").to_string()
        }
    }

    /// The recall answer for a spoken question: only when it cites a real segment (T-305).
    async fn grounded_answer(&self, question: &str, lang: Lang) -> Option<String> {
        let knowledge = self.knowledge.as_ref()?;
        let answer = knowledge.recall(question, Some(lang.code())).await.ok()?;
        (!answer.citations.is_empty() && !answer.answer_text.trim().is_empty())
            .then(|| answer.answer_text.trim().to_owned())
    }

    /// Keyword fallback: read out the best matching quotes.
    async fn quote_search(&self, query: &str, lang: Lang) -> VoiceResult<String> {
        let terms: Vec<String> = words(query)
            .into_iter()
            .filter(|w| w.chars().count() > 2)
            .collect();
        let hits = self
            .db
            .read(move |c| meeting_ai_store::search_segments_any(c, &terms, 3))
            .await?;
        if hits.is_empty() {
            return Ok(lang.pick(
                "Meetings mein iske baare mein kuch nahi mila.",
                "I couldn't find that in your meetings.",
            ));
        }
        let mut parts = Vec::new();
        for h in &hits {
            let title = meetings::get(&self.db, &h.meeting_id)
                .await
                .map(|m| m.title)
                .unwrap_or_default();
            let quote: String = h.text.chars().take(160).collect();
            parts.push(format!("{title}: “{quote}”"));
        }
        Ok(parts.join(" "))
    }

    /// Open commitments, read out (T-305).
    async fn commitments_text(&self, lang: Lang) -> VoiceResult<String> {
        let Some(knowledge) = &self.knowledge else {
            return Err(not_yet(lang));
        };
        let open = knowledge.open_commitments().await?;
        if open.is_empty() {
            return Ok(lang.pick(
                "Koi open commitment nahi hai.",
                "You have no open commitments.",
            ));
        }
        let lines: Vec<String> = open
            .iter()
            .map(|c| match (&c.owner, &c.due_date) {
                (Some(o), Some(d)) => format!("{o}: {} ({})", c.body, friendly_date(d)),
                (Some(o), None) => format!("{o}: {}", c.body),
                (None, _) => c.body.clone(),
            })
            .collect();
        Ok(lang.pick(
            format!(
                "{} open commitments: {}.",
                open.len(),
                spoken_list(&lines, lang)
            ),
            format!(
                "{} open commitments: {}.",
                open.len(),
                spoken_list(&lines, lang)
            ),
        ))
    }

    // ---- reads ---------------------------------------------------------------------------------

    async fn read(
        &self,
        tool: &str,
        args: &Value,
        lang: Lang,
    ) -> VoiceResult<(ToolResult, Remembered)> {
        let mut seen = Remembered {
            transcript: String::new(),
            tool: Some(tool.to_string()),
            task: None,
            reminder: None,
        };
        let message = match tool {
            tools::TASKS_LIST => {
                let view = args.get("view").and_then(Value::as_str).unwrap_or("today");
                let rows = tasks::query(
                    &self.db,
                    &*self.clock,
                    if view == "all" {
                        TaskView::All
                    } else {
                        TaskView::Today
                    },
                    TaskFilters::default(),
                )
                .await?;
                let rows: Vec<_> = rows
                    .into_iter()
                    .filter(|r| {
                        !matches!(r.task.status, TaskStatus::Completed | TaskStatus::Cancelled)
                    })
                    .filter(|r| view != "overdue" || r.overdue)
                    .collect();
                if let [only] = rows.as_slice() {
                    seen.task = Some((only.task.id.clone(), only.task.title.clone()));
                }
                let titles: Vec<String> = rows.iter().map(|r| r.task.title.clone()).collect();
                match (titles.len(), view) {
                    (0, "overdue") => lang.pick("Koi kaam late nahi hai.", "Nothing is overdue."),
                    (0, _) => lang.pick("Aaj koi kaam nahi hai.", "You have no tasks today."),
                    (n, "overdue") => lang.pick(
                        format!("{n} kaam late hain: {}.", spoken_list(&titles, lang)),
                        format!("{n} overdue: {}.", spoken_list(&titles, lang)),
                    ),
                    (n, "all") => lang.pick(
                        format!("Kul {n} kaam: {}.", spoken_list(&titles, lang)),
                        format!("{n} open tasks: {}.", spoken_list(&titles, lang)),
                    ),
                    (n, _) => lang.pick(
                        format!("Aaj ke {n} kaam: {}.", spoken_list(&titles, lang)),
                        format!("You have {n} tasks today: {}.", spoken_list(&titles, lang)),
                    ),
                }
            }
            tools::REMINDERS_LIST => {
                let mut list = self.open_reminders().await?;
                list.sort_by_key(|r| r.remind_at);
                if let [only] = list.as_slice() {
                    seen.reminder = Some((only.id.clone(), only.title.clone()));
                }
                let items: Vec<String> = list
                    .iter()
                    .map(|r| format!("{} ({})", r.title, self.time_text(r.remind_at)))
                    .collect();
                if items.is_empty() {
                    lang.pick("Koi reminder nahi hai.", "You have no reminders.")
                } else {
                    lang.pick(
                        format!("{} reminder: {}.", items.len(), spoken_list(&items, lang)),
                        format!("{} reminders: {}.", items.len(), spoken_list(&items, lang)),
                    )
                }
            }
            tools::MEETING_SEARCH => {
                let query = args
                    .get("query")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                match self.grounded_answer(query, lang).await {
                    Some(text) => text,
                    None => self.quote_search(query, lang).await?,
                }
            }
            tools::OPEN_PAGE => {
                let page = args
                    .get("page")
                    .and_then(Value::as_str)
                    .and_then(pages::named)
                    .ok_or_else(|| {
                        VoiceError::new(
                            VoiceErrorCode::ToolFailed,
                            lang.pick("Kaunsa page? Phir se boliye.", "Which page? Say it again."),
                        )
                    })?;
                if pages::DEMO_HIDDEN.contains(&page) && self.demo_pages_only().await {
                    return Err(VoiceError::new(
                        VoiceErrorCode::ToolFailed,
                        lang.pick(
                            format!("{} page abhi chhupa hua hai.", page_title(page)),
                            format!("The {} page is hidden in this view.", page_title(page)),
                        ),
                    ));
                }
                self.events.navigate(page);
                lang.pick(
                    format!("{} khol raha hoon.", page_title(page)),
                    format!("Opening {}.", page_title(page)),
                )
            }
            tools::COMMITMENTS_LIST => self.commitments_text(lang).await?,
            tools::NOTES_SEARCH | tools::PROJECTS_LIST => {
                return Err(not_yet(lang));
            }
            _ => {
                return Err(VoiceError::new(
                    VoiceErrorCode::ToolFailed,
                    "That isn't something Pluto can look up.",
                ))
            }
        };
        Ok((
            ToolResult {
                tool: tool.to_string(),
                ok: true,
                message,
                record_id: None,
            },
            seen,
        ))
    }

    // ---- writes: proposal ----------------------------------------------------------------------

    /// Prepares a write: finds its target, writes the summary. `Err(reply)` when it can't be
    /// proposed (nothing matches, a missing date, …) with what to say instead.
    async fn prepare(
        &self,
        tool: &str,
        args: &Value,
        lang: Lang,
    ) -> Result<(Value, String, Vec<(String, String)>), String> {
        let s = |k: &str| {
            args.get(k)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_string)
        };
        match tool {
            tools::TASK_CREATE => {
                let title = s("title").ok_or_else(|| {
                    lang.pick(
                        "Kaunsa kaam? Phir se boliye.",
                        "What's the task? Say it again.",
                    )
                })?;
                let title = capitalise(&title);
                let due = s("due_date");
                // "meeting tomorrow at 6 am": tasks hold only a date, so the time becomes a
                // reminder for the task.
                let at = s("due_phrase")
                    .as_deref()
                    .and_then(clock_time)
                    .and_then(|(h, m)| {
                        let day = due
                            .as_deref()
                            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
                            .unwrap_or_else(|| self.clock.local_date(self.clock.now_ms()));
                        self.local_ms(day, h, m)
                    })
                    .filter(|at| *at > self.clock.now_ms());
                let day = due.as_deref().map(friendly_date).unwrap_or_default();
                let summary = match (&due, at) {
                    (Some(_), Some(at)) => {
                        let t = self.time_text(at);
                        lang.pick(
                            format!(
                                "Naya kaam “{title}”, {day}, {t} par reminder ke saath. Bana doon?"
                            ),
                            format!("Add “{title}” for {day}, with a reminder at {t}?"),
                        )
                    }
                    (Some(_), None) => lang.pick(
                        format!("Naya kaam “{title}”, {day} tak. Bana doon?"),
                        format!("Add “{title}”, due {day}?"),
                    ),
                    (None, Some(at)) => {
                        let t = self.time_text(at);
                        lang.pick(
                            format!("Naya kaam “{title}”, {t} par reminder ke saath. Bana doon?"),
                            format!("Add “{title}”, with a reminder at {t}?"),
                        )
                    }
                    (None, None) => lang.pick(
                        format!("Naya kaam “{title}”. Bana doon?"),
                        format!("Add “{title}”?"),
                    ),
                };
                Ok((
                    json!({"title": title, "due_date": due, "due_phrase": s("due_phrase"), "remind_at": at}),
                    summary,
                    vec![],
                ))
            }
            tools::TASK_COMPLETE | tools::TASK_UPDATE => {
                if tool == tools::TASK_UPDATE && s("due_date").is_none() {
                    return Err(lang.pick(
                        "Kaunsi date? Jaise “Friday tak”.",
                        "Which date? For example “by Friday”.",
                    ));
                }
                let m = s("match").unwrap_or_default();
                let (target, candidates) =
                    self.target_task(&m).await.map_err(|_| unavailable(lang))?;
                let due = s("due_date");
                let mut out = json!({"match": m, "due_date": due, "due_phrase": s("due_phrase")});
                match (target, candidates.len()) {
                    (Some((id, title)), _) => {
                        out["task_id"] = json!(id);
                        out["task_title"] = json!(title);
                        let summary = if tool == tools::TASK_COMPLETE {
                            lang.pick(
                                format!("“{title}” ko done kar doon?"),
                                format!("Mark “{title}” as done?"),
                            )
                        } else {
                            let d = due.clone().unwrap_or_default();
                            lang.pick(
                                format!("“{title}” ki date {d} kar doon?"),
                                format!("Move “{title}” to {d}?"),
                            )
                        };
                        Ok((out, summary, vec![]))
                    }
                    (None, 0) => Err(lang.pick(
                        format!("“{m}” jaisa koi kaam nahi mila."),
                        format!("I couldn't find a task like “{m}”."),
                    )),
                    (None, _) => {
                        let names: Vec<String> = candidates
                            .iter()
                            .enumerate()
                            .map(|(i, (_, t))| format!("{}) {t}", i + 1))
                            .collect();
                        out["candidates"] = json!(candidates
                            .iter()
                            .map(|(id, t)| json!({"id": id, "title": t}))
                            .collect::<Vec<_>>());
                        let summary = lang.pick(
                            format!(
                                "Kaunsa? {}. Pehla, doosra ya teesra boliye.",
                                names.join(", ")
                            ),
                            format!(
                                "Which one? {}. Say first, second or third.",
                                names.join(", ")
                            ),
                        );
                        Ok((out, summary, candidates))
                    }
                }
            }
            tools::REMINDER_CREATE => {
                let title = s("title")
                    .ok_or_else(|| lang.pick("Kis cheez ka reminder?", "Remind you about what?"))?;
                // The decider sometimes leaves the time in `when_phrase` ("5 pm"): read it here.
                let parsed = s("when_phrase").as_deref().and_then(|p| {
                    let (h, m) = clock_time(p)?;
                    let today = self.clock.local_date(self.clock.now_ms());
                    let tomorrow = today.succ_opt()?;
                    let lower = p.to_lowercase();
                    let named_tomorrow = lower.contains("tomorrow") || lower.contains("kal");
                    let first = if named_tomorrow { tomorrow } else { today };
                    let at = self.local_ms(first, h, m)?;
                    if at > self.clock.now_ms() {
                        Some(at)
                    } else {
                        self.local_ms(tomorrow, h, m)
                    }
                });
                // A title that is only the time ("five PM") isn't a title.
                let title = if clock_time(&title).is_some() && words(&title).len() <= 2 {
                    lang.pick("Reminder", "Reminder")
                } else {
                    capitalise(&title)
                };
                let Some(at) = args.get("remind_at").and_then(Value::as_i64).or(parsed) else {
                    return Err(lang.pick(
                        "Kab yaad dilaoon? Time boliye, jaise “5 baje”.",
                        "When should I remind you? Say a time, like “at 5”.",
                    ));
                };
                if at <= self.clock.now_ms() {
                    return Err(
                        lang.pick("Woh time nikal chuka hai.", "That time has already passed.")
                    );
                }
                let when = self.time_text(at);
                Ok((
                    json!({"title": title, "remind_at": at}),
                    lang.pick(
                        format!("“{title}” ka reminder {when} par. Laga doon?"),
                        format!("Remind you to “{title}” at {when}?"),
                    ),
                    vec![],
                ))
            }
            tools::REMINDER_DONE => {
                let m = s("match").unwrap_or_default();
                let list = self.open_reminders().await.map_err(|_| unavailable(lang))?;
                let items: Vec<(String, String)> = list
                    .iter()
                    .map(|r| (r.id.clone(), r.title.clone()))
                    .collect();
                let matched = if is_reference(&m) {
                    self.last_item(true).into_iter().collect()
                } else {
                    best_matches(&m, &items)
                };
                match matched.as_slice() {
                    [] => Err(lang.pick(
                        format!("“{m}” jaisa koi reminder nahi mila."),
                        format!("I couldn't find a reminder like “{m}”."),
                    )),
                    [(id, title)] => Ok((
                        json!({"match": m, "reminder_id": id, "reminder_title": title}),
                        lang.pick(
                            format!("“{title}” reminder done kar doon?"),
                            format!("Mark the reminder “{title}” as done?"),
                        ),
                        vec![],
                    )),
                    many => {
                        let first: Vec<(String, String)> =
                            many.iter().take(CANDIDATES).cloned().collect();
                        let names: Vec<String> = first
                            .iter()
                            .enumerate()
                            .map(|(i, (_, t))| format!("{}) {t}", i + 1))
                            .collect();
                        Ok((
                            json!({"match": m, "candidates": first.iter().map(|(id, t)| json!({"id": id, "title": t})).collect::<Vec<_>>()}),
                            lang.pick(
                                format!("Kaunsa? {}.", names.join(", ")),
                                format!("Which one? {}.", names.join(", ")),
                            ),
                            first,
                        ))
                    }
                }
            }
            tools::NOTE_CREATE | tools::FOCUS_START | tools::FOCUS_STOP => {
                Err(not_yet(lang).message)
            }
            _ => Err(lang.pick("Yeh main nahi kar sakta.", "I can't do that.")),
        }
    }

    /// The task `m` refers to: the previous turn's task for "usse" / "it", else the best title
    /// match. `(Some, _)` = one task; `(None, candidates)` = ambiguous (up to 3) or none.
    async fn target_task(
        &self,
        m: &str,
    ) -> VoiceResult<(Option<(String, String)>, Vec<(String, String)>)> {
        let open = self.open_tasks().await?;
        if is_reference(m) {
            let last = self
                .last_item(false)
                .filter(|(id, _)| open.iter().any(|(o, _)| o == id));
            return Ok((last, vec![]));
        }
        let matched = best_matches(m, &open);
        Ok(match matched.len() {
            0 => {
                // The decider may copy the pronoun into a longer phrase; fall back to context.
                (
                    self.last_item(false)
                        .filter(|_| words(m).iter().any(|w| is_reference(w))),
                    vec![],
                )
            }
            1 => (matched.into_iter().next(), vec![]),
            _ => (None, matched.into_iter().take(CANDIDATES).collect()),
        })
    }

    async fn store_proposal(
        &self,
        interaction_id: &str,
        tool: &str,
        args: Value,
        summary: String,
        lang: Lang,
        candidates: Vec<(String, String)>,
    ) -> VoiceResult<Proposal> {
        let now = self.clock.now_ms();
        let window = i64::try_from(self.window.as_millis()).unwrap_or(30_000);
        let proposal = Proposal {
            id: new_id(),
            interaction_id: interaction_id.to_string(),
            tool: tool.to_string(),
            args,
            summary_text: summary,
            language: lang.code().to_string(),
            expires_at: now + window,
        };
        let row = ProposalRow {
            id: proposal.id.clone(),
            interaction_id: proposal.interaction_id.clone(),
            tool: proposal.tool.clone(),
            args_json: proposal.args.to_string(),
            summary_text: proposal.summary_text.clone(),
            status: "pending".into(),
            created_at: now,
            expires_at: proposal.expires_at,
            decided_at: None,
        };
        self.db
            .write(move |tx| {
                // A new proposal replaces any older one still waiting.
                for old in voice_store::pending_proposals(tx)? {
                    voice_store::decide_proposal(tx, &old.id, Answer::Reject, now)?;
                }
                voice_store::insert_proposal(tx, &row)?;
                Ok(((), vec![]))
            })
            .await?;
        self.lock().pending = Some(Pending {
            proposal: proposal.clone(),
            candidates,
        });
        self.events.proposal(&proposal);
        Ok(proposal)
    }

    /// Expires the proposal when its window ends (if nobody answered).
    fn arm_expiry(self: &Arc<Self>, proposal: &Proposal) {
        let me = Arc::clone(self);
        let (id, expires_at, window) = (proposal.id.clone(), proposal.expires_at, self.window);
        tokio::spawn(async move {
            tokio::time::sleep(window + Duration::from_millis(50)).await;
            let pid = id.clone();
            let expired = me
                .db
                .write(move |tx| {
                    let row = voice_store::get_proposal(tx, &pid)?;
                    if row.as_ref().is_some_and(|r| r.status == "pending") {
                        // The timer, not the wall clock, decides here: the window has passed.
                        voice_store::decide_proposal(tx, &pid, Answer::Approve, expires_at + 1)?;
                        return Ok((row, vec![]));
                    }
                    Ok((None, vec![]))
                })
                .await;
            let Ok(Some(row)) = expired else { return };
            let was_current = {
                let mut st = me.lock();
                let current = st.pending.as_ref().is_some_and(|p| p.proposal.id == id);
                if current {
                    st.pending = None;
                }
                current
            };
            me.log_update(
                &row.interaction_id,
                InteractionUpdate {
                    outcome: Some("expired".into()),
                    ..Default::default()
                },
            )
            .await;
            if was_current {
                if let Some(rt) = me.rt() {
                    rt.proposal_resolved(false);
                }
            }
        });
    }

    // ---- writes: execution ---------------------------------------------------------------------

    /// The user's answer to a proposal (click or voice). Approve / Edit run the tool **once**,
    /// through the store's gate; Reject and an expired proposal run nothing.
    pub async fn respond(
        self: &Arc<Self>,
        proposal_id: &str,
        response: ProposalResponse,
    ) -> VoiceResult<ToolResult> {
        let row = {
            let id = proposal_id.to_string();
            self.db
                .read(move |c| voice_store::get_proposal(c, &id))
                .await?
        }
        .ok_or_else(|| {
            VoiceError::new(
                VoiceErrorCode::ProposalExpired,
                "That request is no longer waiting.",
            )
        })?;
        let lang = {
            let st = self.lock();
            st.pending
                .as_ref()
                .filter(|p| p.proposal.id == proposal_id)
                .map(|p| Lang::of(Some(&p.proposal.language)))
                .unwrap_or(Lang::En)
        };
        let stored: Value = serde_json::from_str(&row.args_json).unwrap_or(Value::Null);
        let (answer, args) = match response {
            ProposalResponse::Approve => (Answer::Approve, stored.clone()),
            ProposalResponse::Reject => (Answer::Reject, stored.clone()),
            ProposalResponse::Edit { args } => (Answer::Approve, merge_edit(&stored, &args)),
        };
        let now = self.clock.now_ms();
        let pid = proposal_id.to_string();
        let decided = self
            .db
            .write(move |tx| Ok((voice_store::decide_proposal(tx, &pid, answer, now)?, vec![])))
            .await?;
        {
            let mut st = self.lock();
            if st
                .pending
                .as_ref()
                .is_some_and(|p| p.proposal.id == proposal_id)
            {
                st.pending = None;
            }
        }
        let rt = self.rt();
        let result = match decided {
            Decided::Approved => {
                if let Some(rt) = &rt {
                    rt.proposal_resolved(true);
                }
                let outcome = self.execute(&row.tool, &args, lang).await;
                if let Some(rt) = &rt {
                    rt.execution_done();
                }
                match outcome {
                    Ok((result, item)) => {
                        self.log_update(
                            &row.interaction_id,
                            InteractionUpdate {
                                outcome: Some("executed".into()),
                                args_json: Some(args.to_string()),
                                ..Default::default()
                            },
                        )
                        .await;
                        self.remember(Remembered {
                            transcript: format!("(approved) {}", row.summary_text),
                            tool: Some(row.tool.clone()),
                            task: item.clone().filter(|_| row.tool.starts_with("task")),
                            reminder: item.filter(|_| row.tool.starts_with("reminder")),
                        });
                        result
                    }
                    Err(e) => {
                        let pid = proposal_id.to_string();
                        let _ = self
                            .db
                            .write(move |tx| {
                                Ok((voice_store::mark_proposal_failed(tx, &pid)?, vec![]))
                            })
                            .await;
                        self.log_update(
                            &row.interaction_id,
                            InteractionUpdate {
                                outcome: Some("failed".into()),
                                ..Default::default()
                            },
                        )
                        .await;
                        ToolResult {
                            tool: row.tool.clone(),
                            ok: false,
                            message: e.message,
                            record_id: None,
                        }
                    }
                }
            }
            Decided::Rejected => {
                if let Some(rt) = &rt {
                    rt.proposal_resolved(false);
                }
                self.log_update(
                    &row.interaction_id,
                    InteractionUpdate {
                        outcome: Some("rejected".into()),
                        ..Default::default()
                    },
                )
                .await;
                ToolResult {
                    tool: row.tool.clone(),
                    ok: true,
                    message: lang.pick(
                        "Theek hai, kuch nahi badla.",
                        "Okay, I didn't change anything.",
                    ),
                    record_id: None,
                }
            }
            Decided::Expired => {
                if let Some(rt) = &rt {
                    rt.proposal_resolved(false);
                }
                self.log_update(
                    &row.interaction_id,
                    InteractionUpdate {
                        outcome: Some("expired".into()),
                        ..Default::default()
                    },
                )
                .await;
                return Err(VoiceError::new(
                    VoiceErrorCode::ProposalExpired,
                    "That request timed out. Ask again.",
                ));
            }
            Decided::AlreadyDecided(_) | Decided::NotFound => {
                return Err(VoiceError::new(
                    VoiceErrorCode::ProposalExpired,
                    "That request was already answered.",
                ));
            }
        };
        // The turn now carries its result.
        let turn = {
            let st = self.lock();
            st.history
                .iter()
                .find(|t| t.interaction_id == row.interaction_id)
                .cloned()
        };
        if let Some(mut turn) = turn {
            turn.result = Some(result.clone());
            self.record(&turn);
        }
        Ok(result)
    }

    /// Runs an approved write. Only [`respond`](Self::respond) calls this, after the gate.
    async fn execute(
        &self,
        tool: &str,
        args: &Value,
        lang: Lang,
    ) -> VoiceResult<(ToolResult, Option<(String, String)>)> {
        let s = |k: &str| {
            args.get(k)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_string)
        };
        let date = |k: &str| -> VoiceResult<Option<NaiveDate>> {
            match s(k) {
                None => Ok(None),
                Some(d) => NaiveDate::parse_from_str(&d, "%Y-%m-%d")
                    .map(Some)
                    .map_err(|_| {
                        VoiceError::new(
                            VoiceErrorCode::ToolFailed,
                            "That date isn't valid (use YYYY-MM-DD).",
                        )
                    }),
            }
        };
        let done = |message: String, id: &str, title: &str| {
            Ok((
                ToolResult {
                    tool: tool.to_string(),
                    ok: true,
                    message,
                    record_id: Some(id.to_string()),
                },
                Some((id.to_string(), title.to_string())),
            ))
        };
        match tool {
            tools::TASK_CREATE => {
                let title = s("title").ok_or_else(|| {
                    VoiceError::new(VoiceErrorCode::ToolFailed, "The task needs a title.")
                })?;
                let task = tasks::create(
                    &self.db,
                    &*self.clock,
                    TaskInput {
                        title,
                        due_date: date("due_date")?,
                        ..Default::default()
                    },
                )
                .await?;
                if let Some(at) = args.get("remind_at").and_then(Value::as_i64) {
                    reminders::create(
                        &self.db,
                        &*self.clock,
                        ReminderInput {
                            title: task.title.clone(),
                            remind_at: at,
                            task_id: Some(task.id.clone()),
                            ..Default::default()
                        },
                    )
                    .await?;
                    return done(
                        lang.pick(
                            "Ho gaya. Kaam aur reminder dono laga diye.",
                            "Done. Task added, with its reminder.",
                        ),
                        &task.id,
                        &task.title,
                    );
                }
                done(
                    lang.pick("Ho gaya. Kaam add kar diya.", "Done. Task added."),
                    &task.id,
                    &task.title,
                )
            }
            tools::TASK_COMPLETE | tools::TASK_UPDATE => {
                let id = self.resolve_edit_target(args).await?;
                if tool == tools::TASK_COMPLETE {
                    let task =
                        tasks::transition(&self.db, &*self.clock, &id, TaskStatus::Completed)
                            .await?;
                    done(
                        lang.pick("Ho gaya. Kaam done.", "Done. Marked complete."),
                        &task.id,
                        &task.title,
                    )
                } else {
                    let due = date("due_date")?.ok_or_else(|| {
                        VoiceError::new(VoiceErrorCode::ToolFailed, "Which date?")
                    })?;
                    let task = tasks::update(
                        &self.db,
                        &*self.clock,
                        &id,
                        TaskPatch {
                            due_date: Some(Some(due)),
                            ..Default::default()
                        },
                    )
                    .await?;
                    done(
                        lang.pick(
                            format!("Ho gaya. Ab {due} tak."),
                            format!("Done. It's now due {due}."),
                        ),
                        &task.id,
                        &task.title,
                    )
                }
            }
            tools::REMINDER_CREATE => {
                let title = s("title").ok_or_else(|| {
                    VoiceError::new(VoiceErrorCode::ToolFailed, "The reminder needs a title.")
                })?;
                let at = args
                    .get("remind_at")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| VoiceError::new(VoiceErrorCode::ToolFailed, "When?"))?;
                let r = reminders::create(
                    &self.db,
                    &*self.clock,
                    ReminderInput {
                        title,
                        remind_at: at,
                        ..Default::default()
                    },
                )
                .await?;
                done(
                    lang.pick("Ho gaya. Reminder laga diya.", "Done. Reminder set."),
                    &r.id,
                    &r.title,
                )
            }
            tools::REMINDER_DONE => {
                let id = s("reminder_id").ok_or_else(|| {
                    VoiceError::new(VoiceErrorCode::ToolFailed, "Which reminder?")
                })?;
                let r = reminders::set_done(&self.db, &*self.clock, &id, true).await?;
                done(lang.pick("Ho gaya.", "Done."), &r.id, &r.title)
            }
            _ => Err(not_yet(lang)),
        }
    }

    /// The task an approved `task_complete` / `task_update` acts on: `task_id`, or the `match`
    /// again when an edit changed it (it must then match exactly one task).
    async fn resolve_edit_target(&self, args: &Value) -> VoiceResult<String> {
        if let Some(id) = args.get("task_id").and_then(Value::as_str) {
            return Ok(id.to_string());
        }
        let m = args
            .get("match")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match self.target_task(m).await? {
            (Some((id, _)), _) => Ok(id),
            (None, c) if c.len() > 1 => Err(VoiceError::new(
                VoiceErrorCode::ToolFailed,
                "Several tasks match. Say which one.",
            )),
            _ => Err(VoiceError::new(
                VoiceErrorCode::ToolFailed,
                "I couldn't find that task.",
            )),
        }
    }

    // ---- one turn ------------------------------------------------------------------------------

    async fn handle(self: &Arc<Self>, mut heard: Heard) -> VoiceResult<Turn> {
        // "Pluto, add a task": the wake word (T-705) isn't part of the command.
        heard.text = crate::wake::strip_wake_word(&heard.text);
        if heard.text.trim().is_empty() {
            return Ok(Turn::Done);
        }
        let lang = Lang::of(heard.language.as_deref());
        let interaction_id = new_id();
        let row = InteractionRow {
            id: interaction_id.clone(),
            started_at: self.clock.now_ms(),
            transcript: heard.text.clone(),
            language: heard.language.clone(),
            ..Default::default()
        };
        self.db
            .write(move |tx| {
                voice_store::insert_interaction(tx, &row)?;
                Ok(((), vec![]))
            })
            .await?;

        // A choice for a waiting "which one?" proposal.
        let choice = {
            let st = self.lock();
            st.pending
                .as_ref()
                .filter(|p| !p.candidates.is_empty())
                .and_then(|p| {
                    ordinal(&heard.text)
                        .and_then(|n| p.candidates.get(n))
                        .map(|c| (p.proposal.clone(), c.clone()))
                })
        };
        if let Some((proposal, (id, title))) = choice {
            let key = if proposal.tool.starts_with("reminder") {
                "reminder_id"
            } else {
                "task_id"
            };
            let mut args = proposal.args.clone();
            args[key] = json!(id);
            args[if key == "task_id" {
                "task_title"
            } else {
                "reminder_title"
            }] = json!(title);
            let decision = AssistantDecision {
                intent: Intent::Approval,
                tool: Some(proposal.tool.clone()),
                args: args.clone(),
                confidence: Some(1.0),
                source: DecisionSource::Rules,
                latency_ms: 0,
            };
            self.log_update(
                &interaction_id,
                InteractionUpdate {
                    intent: Some("approval".into()),
                    decision_source: Some("rules".into()),
                    ..Default::default()
                },
            )
            .await;
            let result = self
                .respond(&proposal.id, ProposalResponse::Edit { args })
                .await?;
            self.record(&AssistantTurn {
                interaction_id,
                transcript: heard.text,
                language: heard.language,
                decision,
                reply_text: Some(result.message.clone()),
                proposal: None,
                result: Some(result),
            });
            return Ok(Turn::Reply);
        }

        // Fixed commands ("meetings kholo", "aaj ke kaam batao") are decided here at once;
        // everything else goes to the decider (rules, then the LLM).
        let started = std::time::Instant::now();
        let decision = match crate::quick::decide(&heard.text) {
            Some((intent, tool, args)) => AssistantDecision {
                intent,
                tool: Some(tool.to_string()),
                args,
                confidence: Some(1.0),
                source: DecisionSource::Rules,
                latency_ms: 0,
            },
            None => {
                let ctx = self.context().await;
                self.decider.decide(&heard.text, &ctx).await?
            }
        };
        if cfg!(debug_assertions) {
            eprintln!(
                "[voice] timing: decided in {:.2} s ({})",
                started.elapsed().as_secs_f32(),
                match decision.source {
                    DecisionSource::Rules => "rules, no LLM",
                    DecisionSource::Llm => "LLM",
                }
            );
        }
        self.log_update(
            &interaction_id,
            InteractionUpdate {
                intent: serde_json::to_value(decision.intent)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string)),
                decision_source: Some(
                    match decision.source {
                        DecisionSource::Rules => "rules",
                        DecisionSource::Llm => "llm",
                    }
                    .into(),
                ),
                tool: decision.tool.clone(),
                args_json: Some(decision.args.to_string()),
                llm_latency_ms: (decision.source == DecisionSource::Llm)
                    .then_some(i64::from(decision.latency_ms)),
                ..Default::default()
            },
        )
        .await;

        let mut turn = AssistantTurn {
            interaction_id: interaction_id.clone(),
            transcript: heard.text.clone(),
            language: heard.language.clone(),
            decision: decision.clone(),
            reply_text: None,
            proposal: None,
            result: None,
        };
        let mut seen = Remembered {
            transcript: heard.text.clone(),
            tool: decision.tool.clone(),
            task: None,
            reminder: None,
        };

        // Spoken answers to a waiting proposal.
        if matches!(
            decision.intent,
            Intent::Approval | Intent::Reject | Intent::Cancel
        ) {
            let pending = self.pending();
            let reply = match (pending, decision.intent) {
                (Some(p), Intent::Approval) => {
                    let r = self.respond(&p.id, ProposalResponse::Approve).await?;
                    turn.result = Some(r.clone());
                    r.message
                }
                (Some(p), _) => {
                    let r = self.respond(&p.id, ProposalResponse::Reject).await?;
                    turn.result = Some(r.clone());
                    r.message
                }
                (None, Intent::Approval) => lang.pick(
                    "Abhi kuch approve karne ko nahi hai.",
                    "There's nothing waiting for approval.",
                ),
                (None, _) => {
                    self.log_update(
                        &interaction_id,
                        InteractionUpdate {
                            outcome: Some("cancelled".into()),
                            ..Default::default()
                        },
                    )
                    .await;
                    self.remember(seen);
                    return Ok(Turn::Done);
                }
            };
            turn.reply_text = Some(reply);
            self.record(&turn);
            self.remember(seen);
            return Ok(Turn::Reply);
        }

        let Some(tool) = decision
            .tool
            .clone()
            .filter(|_| decision.intent != Intent::Noise)
        else {
            self.log_update(
                &interaction_id,
                InteractionUpdate {
                    outcome: Some("cancelled".into()),
                    ..Default::default()
                },
            )
            .await;
            return Ok(Turn::Done);
        };

        if tools::READ.contains(&tool.as_str()) {
            let reply = match self.read(&tool, &decision.args, lang).await {
                Ok((result, read_seen)) => {
                    seen.task = read_seen.task;
                    seen.reminder = read_seen.reminder;
                    let msg = result.message.clone();
                    turn.result = Some(result);
                    self.log_update(
                        &interaction_id,
                        InteractionUpdate {
                            outcome: Some("answered".into()),
                            ..Default::default()
                        },
                    )
                    .await;
                    msg
                }
                Err(e) => {
                    self.log_update(
                        &interaction_id,
                        InteractionUpdate {
                            outcome: Some("failed".into()),
                            ..Default::default()
                        },
                    )
                    .await;
                    e.message
                }
            };
            turn.reply_text = Some(reply);
            self.record(&turn);
            self.remember(seen);
            return Ok(Turn::Reply);
        }

        if tools::WRITE.contains(&tool.as_str()) {
            match self.prepare(&tool, &decision.args, lang).await {
                Ok((args, summary, candidates)) => {
                    if let Some(id) = args.get("task_id").and_then(Value::as_str) {
                        let title = args
                            .get("task_title")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        seen.task = Some((id.to_string(), title.to_string()));
                    }
                    if let Some(id) = args.get("reminder_id").and_then(Value::as_str) {
                        let title = args
                            .get("reminder_title")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        seen.reminder = Some((id.to_string(), title.to_string()));
                    }
                    let proposal = self
                        .store_proposal(&interaction_id, &tool, args, summary, lang, candidates)
                        .await?;
                    self.arm_expiry(&proposal);
                    turn.reply_text = Some(proposal.summary_text.clone());
                    turn.proposal = Some(proposal);
                    self.record(&turn);
                    self.remember(seen);
                    return Ok(Turn::Proposal);
                }
                Err(reply) => {
                    self.log_update(
                        &interaction_id,
                        InteractionUpdate {
                            outcome: Some("answered".into()),
                            ..Default::default()
                        },
                    )
                    .await;
                    turn.reply_text = Some(reply);
                    self.record(&turn);
                    self.remember(seen);
                    return Ok(Turn::Reply);
                }
            }
        }

        self.log_update(
            &interaction_id,
            InteractionUpdate {
                outcome: Some("failed".into()),
                ..Default::default()
            },
        )
        .await;
        Ok(Turn::Done)
    }
}

/// Edited arguments from the proposal card: the edited fields win. A changed `match` drops the
/// resolved target, so it is found again from the new words.
fn merge_edit(stored: &Value, edited: &Value) -> Value {
    let mut out = stored.clone();
    let (Some(o), Some(e)) = (out.as_object_mut(), edited.as_object()) else {
        return stored.clone();
    };
    let match_changed = e
        .get("match")
        .is_some_and(|m| Some(m) != stored.get("match"));
    for (k, v) in e {
        if ["task_id", "reminder_id"].contains(&k.as_str())
            && !stored.get("candidates").is_some_and(Value::is_array)
        {
            continue; // a target can only be chosen from the offered candidates
        }
        o.insert(k.clone(), v.clone());
    }
    if match_changed && !e.contains_key("task_id") {
        o.remove("task_id");
        o.remove("task_title");
    }
    if let Some(id) = e.get("task_id").and_then(Value::as_str) {
        let offered = stored
            .get("candidates")
            .and_then(Value::as_array)
            .is_some_and(|c| {
                c.iter()
                    .any(|x| x.get("id").and_then(Value::as_str) == Some(id))
            });
        if !offered {
            o.remove("task_id");
        }
    }
    out
}

fn not_yet(lang: Lang) -> VoiceError {
    VoiceError::new(
        VoiceErrorCode::ToolFailed,
        lang.pick(
            "Yeh abhi voice se nahi hota. App mein kar lijiye.",
            "I can't do that by voice yet. Use the app for now.",
        ),
    )
}

fn unavailable(lang: Lang) -> String {
    lang.pick(
        "Abhi tasks nahi khul rahe. Thodi der mein try kijiye.",
        "I can't reach your tasks right now. Try again shortly.",
    )
}

#[async_trait]
impl TurnHandler for Arc<Assistant> {
    async fn turn(&self, heard: Heard) -> VoiceResult<Turn> {
        self.handle(heard).await
    }
}

/// The assistant as a [`ToolExecutor`] for callers outside it (D2's recall follow-ups): reads
/// run, writes only propose.
pub struct AssistantTools(pub Arc<Assistant>);

impl std::ops::Deref for AssistantTools {
    type Target = Arc<Assistant>;
    fn deref(&self) -> &Arc<Assistant> {
        &self.0
    }
}

#[async_trait]
impl ToolExecutor for AssistantTools {
    async fn run_read(&self, tool: &str, args: &Value) -> VoiceResult<ToolResult> {
        if !tools::READ.contains(&tool) {
            return Err(VoiceError::new(
                VoiceErrorCode::ToolFailed,
                "That tool changes data; it needs a proposal.",
            ));
        }
        Ok(self.read(tool, args, Lang::En).await?.0)
    }

    async fn propose(&self, request: ProposalRequest) -> VoiceResult<Proposal> {
        if !tools::WRITE.contains(&request.tool.as_str()) {
            return Err(VoiceError::new(
                VoiceErrorCode::ToolFailed,
                "Only changes need a proposal.",
            ));
        }
        let lang = Lang::of(Some(&request.language));
        let interaction_id = match request.interaction_id {
            Some(id) => id,
            None => {
                let id = new_id();
                let row = InteractionRow {
                    id: id.clone(),
                    started_at: self.clock.now_ms(),
                    transcript: String::new(),
                    tool: Some(request.tool.clone()),
                    ..Default::default()
                };
                self.db
                    .write(move |tx| Ok((voice_store::insert_interaction(tx, &row)?, vec![])))
                    .await?;
                id
            }
        };
        let proposal = self
            .store_proposal(
                &interaction_id,
                &request.tool,
                request.args,
                request.summary_text,
                lang,
                vec![],
            )
            .await?;
        self.arm_expiry(&proposal);
        Ok(proposal)
    }
}

#[cfg(test)]
mod tests;
