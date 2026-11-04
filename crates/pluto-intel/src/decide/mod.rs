//! Rules table, then the intent LLM, producing an `AssistantDecision` with resolved arguments.
//!
//! Owner: **D2** (T-302). `rules`: zero-cost answers. This file: the LLM path, per-tool argument
//! checks and date/time resolution (the LLM only ever returns phrases; Rust makes the dates).

pub mod rules;

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use chrono::{Duration, NaiveDate, NaiveTime, TimeZone};
use chrono_tz::Tz;
use pluto_contracts::decide::{AssistantDecision, DecideContext, Decider, DecisionSource, Intent};
use pluto_contracts::error::{VoiceError, VoiceErrorCode, VoiceResult};
use pluto_contracts::llm::{Llm, LlmMessage, LlmRequest, LlmRole, LlmTask};
use serde_json::{json, Map, Value};

use crate::dates::resolve_date;
use crate::llm::router::default_timeout_ms;

/// The tools the assistant may propose or run (API_CONTRACTS §3.5) and the intent each belongs to.
const TOOLS: [(&str, Intent); 14] = [
    ("tasks_list", Intent::Task),
    ("task_create", Intent::Task),
    ("task_complete", Intent::Task),
    ("task_update", Intent::Task),
    ("reminders_list", Intent::Reminder),
    ("reminder_create", Intent::Reminder),
    ("reminder_done", Intent::Reminder),
    ("meeting_search", Intent::MeetingQuery),
    ("commitments_list", Intent::MeetingQuery),
    ("notes_search", Intent::Note),
    ("note_create", Intent::Note),
    ("focus_start", Intent::Focus),
    ("focus_stop", Intent::Focus),
    ("projects_list", Intent::Project),
];

fn intent_name(i: Intent) -> &'static str {
    match i {
        Intent::MeetingQuery => "meeting_query",
        Intent::Task => "task",
        Intent::Reminder => "reminder",
        Intent::Note => "note",
        Intent::Focus => "focus",
        Intent::Project => "project",
        Intent::Approval => "approval",
        Intent::Reject => "reject",
        Intent::Cancel => "cancel",
        Intent::Noise => "noise",
    }
}

fn parse_intent(s: &str) -> Option<Intent> {
    [
        Intent::MeetingQuery,
        Intent::Task,
        Intent::Reminder,
        Intent::Note,
        Intent::Focus,
        Intent::Project,
        Intent::Approval,
        Intent::Reject,
        Intent::Cancel,
        Intent::Noise,
    ]
    .into_iter()
    .find(|i| intent_name(*i) == s)
}

fn schema() -> Value {
    json!({
        "type": "object",
        "required": ["intent", "args"],
        "properties": {
            "intent": {"type": "string", "enum": ["meeting_query","task","reminder","note","focus","project","approval","reject","cancel","noise"]},
            "tool": {"type": "string", "nullable": true},
            "args": {"type": "object"},
            "confidence": {"type": "number", "nullable": true}
        }
    })
}

const SYSTEM: &str = "You route one spoken command (Hindi, Tamil, Kannada or English, often mixed) to a tool for a personal productivity app. \
Reply with JSON {intent, tool, args, confidence}. Use only these tools and arguments:\n\
- tasks_list {view: \"today\"|\"overdue\"|\"all\"}\n\
- task_create {title, due_phrase?}\n\
- task_complete {match}  (match = words from the task title)\n\
- task_update {match, due_phrase}\n\
- reminders_list {}\n\
- reminder_create {title, when_phrase, time?: \"HH:MM\" 24-hour, in_minutes?: number}\n\
- reminder_done {match}\n\
- meeting_search {query}  (questions about what was said or decided in past meetings)\n\
- commitments_list {owner?, status?: \"open\"|\"overdue\"|\"done\"}\n\
- notes_search {query}; note_create {body}\n\
- focus_start {minutes}; focus_stop {}\n\
- projects_list {}\n\
Intents: meeting_query, task, reminder, note, focus, project, or noise for anything that is not a command (set tool null). \
Titles and queries are short English. Copy time words exactly into due_phrase / when_phrase (for example \"kal tak\", \"Friday\"); \
never compute dates. For \"remind me in two hours\" set in_minutes. \"time\" is the clock time if one was spoken (\"5 pm\" -> \"17:00\").";

/// Local date and time for a decision.
struct Clock {
    tz: Tz,
    now_ms: i64,
}

impl Clock {
    fn new(ctx: &DecideContext) -> Self {
        Self {
            tz: ctx.timezone.parse().unwrap_or(chrono_tz::Asia::Kolkata),
            now_ms: ctx.now_ms,
        }
    }
    fn now(&self) -> chrono::DateTime<Tz> {
        self.tz
            .timestamp_millis_opt(self.now_ms)
            .single()
            .unwrap_or_else(|| self.tz.timestamp_millis_opt(0).unwrap())
    }
    fn today(&self) -> NaiveDate {
        self.now().date_naive()
    }
    fn at(&self, date: NaiveDate, time: NaiveTime) -> Option<i64> {
        self.tz
            .from_local_datetime(&date.and_time(time))
            .earliest()
            .map(|d| d.timestamp_millis())
    }
}

fn bad_output() -> VoiceError {
    VoiceError::new(
        VoiceErrorCode::LlmInvalidOutput,
        "The language service gave an answer Pluto couldn't use.",
    )
}

fn text(args: &Map<String, Value>, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn parse_hhmm(s: &str) -> Option<NaiveTime> {
    let (h, m) = s.trim().split_once(':')?;
    NaiveTime::from_hms_opt(h.trim().parse().ok()?, m.trim().parse().ok()?, 0)
}

/// Check and complete the arguments for one tool. Dates and times are resolved here.
fn resolve_args(tool: &str, raw: &Map<String, Value>, clock: &Clock) -> Option<Value> {
    let today = clock.today();
    let with_due = |mut out: Map<String, Value>| {
        if let Some(phrase) = text(raw, "due_phrase") {
            if let Some(d) = resolve_date(&phrase, today) {
                out.insert("due_date".into(), json!(d.to_string()));
            }
            out.insert("due_phrase".into(), json!(phrase));
        }
        Value::Object(out)
    };
    Some(match tool {
        "tasks_list" => {
            let view = text(raw, "view")
                .filter(|v| ["today", "overdue", "all"].contains(&v.as_str()))
                .unwrap_or_else(|| "today".into());
            json!({"view": view})
        }
        "task_create" => with_due(Map::from_iter([(
            "title".to_owned(),
            json!(text(raw, "title")?),
        )])),
        "task_complete" | "reminder_done" => {
            json!({"match": text(raw, "match").or_else(|| text(raw, "title"))?})
        }
        "task_update" => {
            let out = with_due(Map::from_iter([(
                "match".to_owned(),
                json!(text(raw, "match").or_else(|| text(raw, "title"))?),
            )]));
            out.get("due_phrase")?;
            out
        }
        "reminders_list" | "focus_stop" | "projects_list" => json!({}),
        "reminder_create" => {
            let title = text(raw, "title")?;
            let when = text(raw, "when_phrase");
            let remind_at = if let Some(mins) = raw
                .get("in_minutes")
                .and_then(Value::as_i64)
                .filter(|m| (1..=7 * 24 * 60).contains(m))
            {
                Some(clock.now_ms + Duration::minutes(mins).num_milliseconds())
            } else {
                let time = text(raw, "time").as_deref().and_then(parse_hhmm);
                let date = when.as_deref().and_then(|p| resolve_date(p, today));
                match (date, time) {
                    (Some(d), Some(t)) => clock.at(d, t),
                    // A time with no day: today if it is still ahead, otherwise tomorrow.
                    (None, Some(t)) => {
                        let ms = clock.at(today, t)?;
                        Some(if ms > clock.now_ms {
                            ms
                        } else {
                            clock.at(today + Duration::days(1), t)?
                        })
                    }
                    // A day with no time: ask. `remind_at: null` means "needs a time".
                    _ => None,
                }
            };
            json!({"title": title, "remind_at": remind_at, "when_phrase": when})
        }
        "meeting_search" | "notes_search" => json!({"query": text(raw, "query")?}),
        "commitments_list" => {
            let mut out = Map::new();
            if let Some(o) = text(raw, "owner") {
                out.insert("owner".into(), json!(o));
            }
            if let Some(s) =
                text(raw, "status").filter(|s| ["open", "overdue", "done"].contains(&s.as_str()))
            {
                out.insert("status".into(), json!(s));
            }
            Value::Object(out)
        }
        "note_create" => json!({"body": text(raw, "body")?}),
        "focus_start" => {
            let m = raw
                .get("minutes")
                .and_then(Value::as_u64)
                .filter(|m| (1..=240).contains(m))?;
            json!({"minutes": m})
        }
        _ => return None,
    })
}

fn user_prompt(text: &str, ctx: &DecideContext, clock: &Clock) -> String {
    let now = clock.now();
    let mut out = format!(
        "Now: {} ({}), timezone {}\n",
        now.format("%A %Y-%m-%d %H:%M"),
        now.format("%z"),
        ctx.timezone
    );
    if !ctx.open_task_titles.is_empty() {
        out.push_str(&format!(
            "Open tasks: {}\n",
            ctx.open_task_titles.join("; ")
        ));
    }
    if !ctx.participants.is_empty() {
        out.push_str(&format!(
            "People in recent meetings: {}\n",
            ctx.participants.join(", ")
        ));
    }
    for turn in ctx
        .recent_turns
        .iter()
        .rev()
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        out.push_str(&format!("Earlier: {turn}\n"));
    }
    out.push_str(&format!("\nCommand: {text}"));
    out
}

fn ms(start: Instant) -> u32 {
    u32::try_from(start.elapsed().as_millis()).unwrap_or(u32::MAX)
}

/// Rules first, then one intent call. Never touches the database: it only decides.
pub struct DefaultDecider {
    llm: Arc<dyn Llm>,
}

impl DefaultDecider {
    pub fn new(llm: Arc<dyn Llm>) -> Self {
        Self { llm }
    }
}

#[async_trait]
impl Decider for DefaultDecider {
    async fn decide(&self, text: &str, ctx: &DecideContext) -> VoiceResult<AssistantDecision> {
        let started = Instant::now();
        if let Some(hit) = rules::apply(text) {
            return Ok(AssistantDecision {
                intent: hit.intent,
                tool: hit.tool.map(str::to_owned),
                args: hit.args,
                confidence: Some(1.0),
                source: DecisionSource::Rules,
                latency_ms: ms(started),
            });
        }
        let clock = Clock::new(ctx);
        let resp = self
            .llm
            .complete_json(LlmRequest {
                task: LlmTask::Intent,
                system: SYSTEM.into(),
                messages: vec![LlmMessage {
                    role: LlmRole::User,
                    content: user_prompt(text, ctx, &clock),
                }],
                json_schema: schema(),
                timeout_ms: default_timeout_ms(LlmTask::Intent),
            })
            .await?;
        let v = &resp.json;
        let intent = v["intent"]
            .as_str()
            .and_then(parse_intent)
            .ok_or_else(bad_output)?;
        let confidence = v["confidence"].as_f64().map(|c| c.clamp(0.0, 1.0) as f32);
        let latency_ms = ms(started);
        let done = |intent, tool: Option<String>, args| AssistantDecision {
            intent,
            tool,
            args,
            confidence,
            source: DecisionSource::Llm,
            latency_ms,
        };

        // Not a tool call: noise, or a spoken answer the rules did not catch.
        let Some(tool) = v["tool"].as_str().filter(|t| !t.is_empty()) else {
            let intent = if matches!(
                intent,
                Intent::Approval | Intent::Reject | Intent::Cancel | Intent::Noise
            ) {
                intent
            } else {
                Intent::Noise
            };
            return Ok(done(intent, None, json!({})));
        };
        // The tool must exist and belong to the stated intent; otherwise do nothing.
        let Some((_, tool_intent)) = TOOLS.iter().find(|(name, _)| *name == tool) else {
            tracing::info!("the model named a tool that does not exist");
            return Ok(done(Intent::Noise, None, json!({})));
        };
        let empty = Map::new();
        let raw = v["args"].as_object().unwrap_or(&empty);
        match resolve_args(tool, raw, &clock) {
            Some(args) => Ok(done(*tool_intent, Some(tool.to_owned()), args)),
            None => {
                tracing::info!(tool, "the model's arguments were incomplete");
                Ok(done(Intent::Noise, None, json!({})))
            }
        }
    }
}
