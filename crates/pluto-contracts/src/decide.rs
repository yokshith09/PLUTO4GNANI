//! `AssistantDecision`, `Intent`, `DecideContext` and the `Decider` trait (API_CONTRACTS §3.4, §3.7).
//!
//! Owner: **D2** (T-005).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::VoiceResult;

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
    /// "Meetings kholo", "go to settings": switch the dashboard page (`open_page`).
    Navigate,
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
pub struct AssistantDecision {
    pub intent: Intent,
    pub tool: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
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
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub now_ms: i64,
}

/// Rules first, then the LLM. Async and fallible: the LLM call can fail (`llm_unavailable`).
#[async_trait]
pub trait Decider: Send + Sync {
    async fn decide(&self, text: &str, ctx: &DecideContext) -> VoiceResult<AssistantDecision>;
}
