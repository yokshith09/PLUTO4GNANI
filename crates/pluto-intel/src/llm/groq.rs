//! Groq (OpenAI-compatible chat completions) as an `Llm` (API_CONTRACTS §2.6).

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use pluto_contracts::error::{VoiceError, VoiceErrorCode, VoiceResult};
use pluto_contracts::llm::{Llm, LlmProvider, LlmRequest, LlmResponse, LlmRole, LlmTask};
use pluto_contracts::voice::Destination;
use pluto_core::secrets::{Account, SecretStore};
use pluto_gnani::http::{OutboundLog, OutboundRecord};
use serde_json::{json, Value};

use super::shared::{invalid_output, map_status, map_transport, missing_key, with_repair};

pub const DEFAULT_BASE_URL: &str = "https://api.groq.com";
pub const DEFAULT_MODEL: &str = "openai/gpt-oss-120b";

pub struct GroqClient {
    http: reqwest::Client,
    base_url: String,
    model: String,
    secrets: Arc<dyn SecretStore>,
    log: Arc<dyn OutboundLog>,
}

impl GroqClient {
    pub fn new(
        model: impl Into<String>,
        secrets: Arc<dyn SecretStore>,
        log: Arc<dyn OutboundLog>,
    ) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: DEFAULT_BASE_URL.into(),
            model: model.into(),
            secrets,
            log,
        }
    }

    /// For tests: point at a local server.
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    async fn send(&self, key: &str, req: LlmRequest) -> VoiceResult<String> {
        let body = self.request_body(&req);
        let bytes = body.to_string().len() as u64;
        self.log.record(OutboundRecord {
            destination: Destination::Groq,
            purpose: format!("{:?}", req.task).to_lowercase(),
            bytes,
            ref_id: None,
        });
        let resp = self
            .http
            .post(format!("{}/openai/v1/chat/completions", self.base_url))
            .bearer_auth(key)
            .timeout(Duration::from_millis(u64::from(req.timeout_ms)))
            .json(&body)
            .send()
            .await
            .map_err(|e| map_transport(&e))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(map_status(status));
        }
        let v: Value = resp.json().await.map_err(|_| invalid_output())?;
        v["choices"][0]["message"]["content"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(invalid_output)
    }

    /// JSON mode guarantees valid JSON but not the shape, so the schema goes in the prompt too.
    fn request_body(&self, req: &LlmRequest) -> Value {
        let system = format!(
            "{}\n\nReply with only a JSON object that matches this JSON Schema:\n{}",
            req.system, req.json_schema
        );
        let mut messages = vec![json!({"role":"system","content":system})];
        for m in &req.messages {
            let role = match m.role {
                LlmRole::System => "system",
                LlmRole::User => "user",
                LlmRole::Assistant => "assistant",
            };
            messages.push(json!({"role":role,"content":m.content}));
        }
        let mut body = json!({"model": self.model, "temperature": 0, "response_format": {"type":"json_object"}, "messages": messages});
        // gpt-oss thinks before it answers; for a spoken command that is seconds of waiting.
        // Short commands get the lowest effort, long meeting text keeps a little more.
        if self.model.contains("gpt-oss") {
            let effort = if req.task == LlmTask::Intent {
                "low"
            } else {
                "medium"
            };
            body["reasoning_effort"] = json!(effort);
        }
        body
    }
}

#[async_trait]
impl Llm for GroqClient {
    async fn complete_json(&self, req: LlmRequest) -> VoiceResult<LlmResponse> {
        let key = self
            .secrets
            .get(Account::Groq)
            .map_err(|e| VoiceError::new(VoiceErrorCode::Core, e.message))?
            .ok_or_else(missing_key)?;
        let started = Instant::now();
        let json = with_repair(&req, |r| self.send(&key, r)).await?;
        Ok(LlmResponse {
            json,
            provider: LlmProvider::Groq,
            model: self.model.clone(),
            latency_ms: u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX),
            fell_back: false,
        })
    }
}
