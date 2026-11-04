//! `VoiceError` and `VoiceErrorCode`, returned by every new command (API_CONTRACTS §6).
//!
//! Owner: **D1** (T-004). Changes go through a `contract` PR that D2 approves.
//!
//! Existing commands keep core's `AppError`; core's `ErrorCode` has only 7 fixed values, so the
//! voice and meeting commands need their own. Messages are written for the person using the app:
//! what happened and the one next step. They never contain transcript text or API keys.

use pluto_core::error::AppError;
use serde::{Deserialize, Serialize};

/// Every error a new voice or meeting command can return. `snake_case` on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum VoiceErrorCode {
    /// Cloud voice is off in Settings; nothing was sent.
    CloudVoiceDisabled,
    /// The key for the needed service is not saved.
    MissingApiKey,
    /// The service rejected the saved key.
    InvalidApiKey,
    /// Gnani answered 402. Voice is disabled until credit is added.
    InsufficientCredit,
    /// 429 after the retries in `pluto-gnani::http` were used up.
    RateLimited,
    MicUnavailable,
    MicPermissionDenied,
    NetworkOffline,
    /// Gnani 5xx / 504.
    GnaniUpstream,
    /// Gnani rejected the request (`{success:false, error:{type}}` or another 4xx).
    GnaniBadRequest,
    /// A Batch job ended `FAILED` / `START_FAILED`, or returned `{error, message}`.
    BatchJobFailed,
    /// An import over 10 MB.
    FileTooLarge,
    UnsupportedAudio,
    /// Both LLM providers failed (timeouts, 429, 5xx).
    LlmUnavailable,
    /// Invalid JSON after one repair attempt.
    LlmInvalidOutput,
    /// The proposal's 30 s window passed before an answer.
    ProposalExpired,
    ToolFailed,
    /// A mic recording was started without the consent confirmation.
    ConsentRequired,
    /// Another voice session or recording is active, or a Gnani queue is full.
    Busy,
    /// A core function failed during a tool call; `message` carries its text.
    Core,
}

impl VoiceErrorCode {
    /// Whether the same request may succeed if tried again later without any change by the user.
    /// `VoiceError::new` uses this as the default for `retryable`.
    pub const fn retryable_by_default(self) -> bool {
        matches!(
            self,
            Self::RateLimited
                | Self::NetworkOffline
                | Self::GnaniUpstream
                | Self::LlmUnavailable
                | Self::Busy
        )
    }
}

/// Serializes to `{ "code": "rate_limited", "message": "...", "retryable": true }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{code:?}: {message}")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct VoiceError {
    pub code: VoiceErrorCode,
    pub message: String,
    pub retryable: bool,
}

pub type VoiceResult<T> = std::result::Result<T, VoiceError>;

impl VoiceError {
    /// `retryable` follows [`VoiceErrorCode::retryable_by_default`]; override with
    /// [`VoiceError::retryable`].
    pub fn new(code: VoiceErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retryable: code.retryable_by_default(),
        }
    }

    pub fn retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }
}

/// A core function failed during a tool call. The message is core's, already written for the user
/// and free of record content.
impl From<AppError> for VoiceError {
    fn from(e: AppError) -> Self {
        Self::new(VoiceErrorCode::Core, e.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pluto_core::error::ErrorCode;

    #[test]
    fn serializes_snake_case_with_retryable() {
        let e = VoiceError::new(VoiceErrorCode::RateLimited, "Gnani is busy. Try again.");
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            serde_json::json!({"code": "rate_limited", "message": "Gnani is busy. Try again.", "retryable": true})
        );
        let back: VoiceError = serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap();
        assert_eq!(back, e);
    }

    #[test]
    fn every_code_has_the_documented_wire_spelling() {
        let expected = [
            (VoiceErrorCode::CloudVoiceDisabled, "cloud_voice_disabled"),
            (VoiceErrorCode::MissingApiKey, "missing_api_key"),
            (VoiceErrorCode::InvalidApiKey, "invalid_api_key"),
            (VoiceErrorCode::InsufficientCredit, "insufficient_credit"),
            (VoiceErrorCode::RateLimited, "rate_limited"),
            (VoiceErrorCode::MicUnavailable, "mic_unavailable"),
            (VoiceErrorCode::MicPermissionDenied, "mic_permission_denied"),
            (VoiceErrorCode::NetworkOffline, "network_offline"),
            (VoiceErrorCode::GnaniUpstream, "gnani_upstream"),
            (VoiceErrorCode::GnaniBadRequest, "gnani_bad_request"),
            (VoiceErrorCode::BatchJobFailed, "batch_job_failed"),
            (VoiceErrorCode::FileTooLarge, "file_too_large"),
            (VoiceErrorCode::UnsupportedAudio, "unsupported_audio"),
            (VoiceErrorCode::LlmUnavailable, "llm_unavailable"),
            (VoiceErrorCode::LlmInvalidOutput, "llm_invalid_output"),
            (VoiceErrorCode::ProposalExpired, "proposal_expired"),
            (VoiceErrorCode::ToolFailed, "tool_failed"),
            (VoiceErrorCode::ConsentRequired, "consent_required"),
            (VoiceErrorCode::Busy, "busy"),
            (VoiceErrorCode::Core, "core"),
        ];
        for (code, wire) in expected {
            assert_eq!(serde_json::to_value(code).unwrap(), wire);
        }
    }

    #[test]
    fn only_transient_failures_are_retryable_by_default() {
        assert!(VoiceError::new(VoiceErrorCode::GnaniUpstream, "x").retryable);
        assert!(!VoiceError::new(VoiceErrorCode::InvalidApiKey, "x").retryable);
        assert!(!VoiceError::new(VoiceErrorCode::InsufficientCredit, "x").retryable);
        assert!(
            VoiceError::new(VoiceErrorCode::InvalidApiKey, "x")
                .retryable(true)
                .retryable
        );
    }

    #[test]
    fn a_core_error_becomes_core_with_its_message() {
        let e: VoiceError =
            AppError::new(ErrorCode::NotFound, "That task no longer exists.").into();
        assert_eq!(e.code, VoiceErrorCode::Core);
        assert_eq!(e.message, "That task no longer exists.");
        assert!(!e.retryable);
    }
}
