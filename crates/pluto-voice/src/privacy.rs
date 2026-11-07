//! Cloud-voice opt-in checks, usage counters and the privacy log.
//!
//! Owner: **D1**. The Cloud voice gate is T-403 (T-PR-04); usage counters and the privacy log
//! screen are T-506.
//!
//! **Cloud voice off → zero outbound requests.** One [`CloudSwitch`] is shared by every client
//! and checked right before anything leaves the device:
//! - Gnani REST, Batch and its transcript downloads ([`pluto_gnani::http::GnaniHttp`]);
//! - Prisma Realtime, including reconnects and audio mid-session;
//! - Timbre;
//! - the LLMs, through [`CloudGated`] around the Gemini/Groq router.

use std::sync::Arc;

use async_trait::async_trait;
use pluto_contracts::error::VoiceResult;
use pluto_contracts::llm::{Llm, LlmRequest, LlmResponse};
pub use pluto_gnani::http::{cloud_off, CloudSwitch};

/// An [`Llm`] that sends nothing while Cloud voice is off (`CloudVoiceDisabled` instead).
pub struct CloudGated {
    inner: Arc<dyn Llm>,
    cloud: Arc<CloudSwitch>,
}

impl CloudGated {
    pub fn new(inner: Arc<dyn Llm>, cloud: Arc<CloudSwitch>) -> Self {
        Self { inner, cloud }
    }
}

#[async_trait]
impl Llm for CloudGated {
    async fn complete_json(&self, req: LlmRequest) -> VoiceResult<LlmResponse> {
        self.cloud.check()?;
        self.inner.complete_json(req).await
    }
}
