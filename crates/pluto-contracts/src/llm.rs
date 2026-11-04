//! LLM request/response types and the `Llm` trait (API_CONTRACTS §2.6, §3.7).
//!
//! Owner: **D2** (T-005). `MockLlm` lives in `pluto-intel::llm`.

use crate::error::VoiceResult;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmTask {
    Intent,
    Relevance,
    Extract,
    RecallAnswer,
    Recap,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmProvider {
    Gemini,
    Groq,
    Mock,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmRole {
    System,
    User,
    Assistant,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: LlmRole,
    pub content: String,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmRequest {
    pub task: LlmTask,
    pub system: String,
    pub messages: Vec<LlmMessage>,
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub json_schema: serde_json::Value,
    pub timeout_ms: u32,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmResponse {
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub json: serde_json::Value,
    pub provider: LlmProvider,
    pub model: String,
    pub latency_ms: u32,
    pub fell_back: bool,
}

/// Returns schema-shaped JSON. Adapters (Gemini, Groq), the router and `MockLlm` implement it.
#[async_trait]
pub trait Llm: Send + Sync {
    async fn complete_json(&self, req: LlmRequest) -> VoiceResult<LlmResponse>;
}
