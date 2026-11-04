//! Picks the provider per task and falls back to the other (ARCHITECTURE §7).

use std::sync::Arc;

use async_trait::async_trait;
use pluto_contracts::error::{VoiceError, VoiceErrorCode, VoiceResult};
use pluto_contracts::llm::{Llm, LlmProvider, LlmRequest, LlmResponse, LlmTask};

/// Intent calls must be quick; everything else may take longer.
pub const INTENT_TIMEOUT_MS: u32 = 4_000;
pub const LONG_TIMEOUT_MS: u32 = 30_000;

/// The timeout a caller should put in `LlmRequest.timeout_ms` for this task.
pub fn default_timeout_ms(task: LlmTask) -> u32 {
    if task == LlmTask::Intent {
        INTENT_TIMEOUT_MS
    } else {
        LONG_TIMEOUT_MS
    }
}

/// Which provider answers a task first: Groq for short intent calls, Gemini for long text.
pub fn primary_for(task: LlmTask) -> LlmProvider {
    match task {
        LlmTask::Intent => LlmProvider::Groq,
        _ => LlmProvider::Gemini,
    }
}

pub struct Router {
    gemini: Arc<dyn Llm>,
    groq: Arc<dyn Llm>,
}

impl Router {
    pub fn new(gemini: Arc<dyn Llm>, groq: Arc<dyn Llm>) -> Self {
        Self { gemini, groq }
    }

    fn client(&self, p: LlmProvider) -> &Arc<dyn Llm> {
        if p == LlmProvider::Groq {
            &self.groq
        } else {
            &self.gemini
        }
    }
}

fn is_key_problem(e: &VoiceError) -> bool {
    matches!(
        e.code,
        VoiceErrorCode::MissingApiKey | VoiceErrorCode::InvalidApiKey
    )
}

#[async_trait]
impl Llm for Router {
    async fn complete_json(&self, req: LlmRequest) -> VoiceResult<LlmResponse> {
        let primary = primary_for(req.task);
        let secondary = if primary == LlmProvider::Groq {
            LlmProvider::Gemini
        } else {
            LlmProvider::Groq
        };
        let first = match self.client(primary).complete_json(req.clone()).await {
            Ok(r) => return Ok(r),
            Err(e) => e,
        };
        tracing::info!(task = ?req.task, code = ?first.code, "primary LLM failed, trying the fallback");
        match self.client(secondary).complete_json(req).await {
            Ok(mut r) => {
                r.fell_back = true;
                Ok(r)
            }
            Err(second) if is_key_problem(&first) && is_key_problem(&second) => Err(first),
            Err(_) => Err(VoiceError::new(
                VoiceErrorCode::LlmUnavailable,
                "Pluto couldn't reach a language service. Nothing was changed.",
            )
            .retryable(true)),
        }
    }
}
