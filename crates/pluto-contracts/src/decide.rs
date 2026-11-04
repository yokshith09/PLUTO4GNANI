//! `Decision`, `Intent`, `DecideContext` and the `Decider` trait (API_CONTRACTS §3.4, §3.7).
//!
//! Owner: **D2** (T-005).

use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Intent {
    MeetingQuery,
    Task,
    Reminder,
    Note,
    Focus,
    Project,
    Approval,
    Reject,
    Cancel,
    Noise,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionSource {
    Rules,
    Llm,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    pub intent: Intent,
    pub tool: Option<String>,
    pub args: serde_json::Value,
    pub confidence: Option<f32>,
    pub source: DecisionSource,
    pub latency_ms: u32,
}

/// Context for one decision: the last 6 turns, today's open task titles, participants, timezone, now.
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecideContext {
    pub recent_turns: Vec<String>,
    pub open_task_titles: Vec<String>,
    pub participants: Vec<String>,
    pub timezone: String,
    pub now_ms: i64,
}

/// Rules first, then the LLM. Synchronous as written in API_CONTRACTS §3.7; D1 and D2 may agree an
/// async variant in the T-004 PR (contract change).
pub trait Decider: Send + Sync {
    fn decide(&self, text: &str, ctx: &DecideContext) -> Decision;
}
