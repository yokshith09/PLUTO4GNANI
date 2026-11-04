//! Gemini and Groq clients, router with fallback, `MockLlm`.
//!
//! Owner: **D2**. Clients and router: T-104. `MockLlm` was added by T-005.

use std::collections::VecDeque;
use std::sync::{Mutex, PoisonError};

use async_trait::async_trait;
use pluto_contracts::error::{VoiceError, VoiceErrorCode, VoiceResult};
use pluto_contracts::llm::{Llm, LlmProvider, LlmRequest, LlmResponse};

/// Scripted `Llm`: returns queued results in order; records every request.
#[derive(Default)]
pub struct MockLlm {
    script: Mutex<VecDeque<VoiceResult<serde_json::Value>>>,
    seen: Mutex<Vec<LlmRequest>>,
}

impl MockLlm {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_json(&self, json: serde_json::Value) -> &Self {
        self.script
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_back(Ok(json));
        self
    }

    pub fn push_error(&self, err: VoiceError) -> &Self {
        self.script
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_back(Err(err));
        self
    }

    pub fn requests(&self) -> Vec<LlmRequest> {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[async_trait]
impl Llm for MockLlm {
    async fn complete_json(&self, req: LlmRequest) -> VoiceResult<LlmResponse> {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(req);
        match self
            .script
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
        {
            Some(Ok(json)) => Ok(LlmResponse {
                json,
                provider: LlmProvider::Mock,
                model: "mock".into(),
                latency_ms: 0,
                fell_back: false,
            }),
            Some(Err(e)) => Err(e),
            None => Err(VoiceError::new(
                VoiceErrorCode::LlmUnavailable,
                "MockLlm script is empty",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pluto_contracts::llm::LlmTask;

    fn req() -> LlmRequest {
        LlmRequest {
            task: LlmTask::Extract,
            system: "s".into(),
            messages: vec![],
            json_schema: serde_json::json!({}),
            timeout_ms: 1000,
        }
    }

    #[tokio::test]
    async fn returns_scripted_json_then_errors_when_empty() {
        let m = MockLlm::new();
        m.push_json(serde_json::json!({"ok": true}));
        let r = m.complete_json(req()).await.unwrap();
        assert_eq!(r.json["ok"], true);
        assert_eq!(r.provider, LlmProvider::Mock);
        assert!(m.complete_json(req()).await.is_err());
        assert_eq!(m.requests().len(), 2);
    }
}
