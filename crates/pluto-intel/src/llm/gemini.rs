//! Google Gemini (`generateContent`) as an `Llm` (API_CONTRACTS §2.6).

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use pluto_contracts::error::{VoiceError, VoiceErrorCode, VoiceResult};
use pluto_contracts::llm::{Llm, LlmProvider, LlmRequest, LlmResponse, LlmRole};
use pluto_contracts::voice::Destination;
use pluto_core::secrets::{Account, SecretStore};
use pluto_gnani::http::{OutboundLog, OutboundRecord};
use serde_json::{json, Value};

use super::shared::{invalid_output, map_status, map_transport, missing_key, with_repair};

pub const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com";
pub const DEFAULT_MODEL: &str = "gemini-2.5-flash";

pub struct GeminiClient {
    http: reqwest::Client,
    base_url: String,
    model: String,
    secrets: Arc<dyn SecretStore>,
    log: Arc<dyn OutboundLog>,
}

impl GeminiClient {
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
        let body = request_body(&req);
        let bytes = body.to_string().len() as u64;
        // Logged before it leaves the device (T-PR-02): destination, purpose, size; never content.
        self.log.record(OutboundRecord {
            destination: Destination::Gemini,
            purpose: format!("{:?}", req.task).to_lowercase(),
            bytes,
            ref_id: None,
        });
        let url = format!(
            "{}/v1beta/models/{}:generateContent",
            self.base_url, self.model
        );
        let resp = self
            .http
            .post(url)
            .header("x-goog-api-key", key)
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
        v["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(invalid_output)
    }
}

#[async_trait]
impl Llm for GeminiClient {
    async fn complete_json(&self, req: LlmRequest) -> VoiceResult<LlmResponse> {
        let key = self
            .secrets
            .get(Account::Gemini)
            .map_err(|e| VoiceError::new(VoiceErrorCode::Core, e.message))?
            .ok_or_else(missing_key)?;
        let started = Instant::now();
        let json = with_repair(&req, |r| self.send(&key, r)).await?;
        Ok(LlmResponse {
            json,
            provider: LlmProvider::Gemini,
            model: self.model.clone(),
            latency_ms: u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX),
            fell_back: false,
        })
    }
}

/// Gemini's `responseSchema` is an OpenAPI subset: upper-case type names, no `additionalProperties`,
/// `$schema` or `title`. Callers write ordinary JSON Schema; this converts it.
pub(crate) fn to_gemini_schema(schema: &Value) -> Value {
    match schema {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(k, _)| {
                    !matches!(
                        k.as_str(),
                        "additionalProperties" | "$schema" | "title" | "default"
                    )
                })
                .map(|(k, v)| match (k.as_str(), v) {
                    ("type", Value::String(t)) => (k.clone(), Value::String(t.to_uppercase())),
                    ("properties", Value::Object(props)) => (
                        k.clone(),
                        Value::Object(
                            props
                                .iter()
                                .map(|(n, s)| (n.clone(), to_gemini_schema(s)))
                                .collect(),
                        ),
                    ),
                    _ => (k.clone(), to_gemini_schema(v)),
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(to_gemini_schema).collect()),
        other => other.clone(),
    }
}

fn request_body(req: &LlmRequest) -> Value {
    let mut system = req.system.clone();
    let mut contents = Vec::new();
    for m in &req.messages {
        match m.role {
            LlmRole::System => {
                system.push_str("\n\n");
                system.push_str(&m.content);
            }
            LlmRole::User => contents.push(json!({"role":"user","parts":[{"text":m.content}]})),
            LlmRole::Assistant => {
                contents.push(json!({"role":"model","parts":[{"text":m.content}]}))
            }
        }
    }
    json!({
        "systemInstruction": {"parts": [{"text": system}]},
        "contents": contents,
        "generationConfig": {
            "responseMimeType": "application/json",
            "responseSchema": to_gemini_schema(&req.json_schema),
            "temperature": 0
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_is_converted_for_gemini() {
        let s = json!({"$schema":"x","type":"object","additionalProperties":false,
            "properties":{"items":{"type":"array","items":{"type":"object","properties":{"kind":{"type":"string","enum":["a"]}}}}},
            "required":["items"]});
        let g = to_gemini_schema(&s);
        assert_eq!(g["type"], "OBJECT");
        assert_eq!(
            g["properties"]["items"]["items"]["properties"]["kind"]["type"],
            "STRING"
        );
        assert_eq!(
            g["properties"]["items"]["items"]["properties"]["kind"]["enum"][0],
            "a"
        );
        assert!(g.get("additionalProperties").is_none() && g.get("$schema").is_none());
        assert_eq!(g["required"][0], "items");
    }
}
