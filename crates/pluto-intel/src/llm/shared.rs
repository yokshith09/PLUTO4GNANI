//! Pieces both providers share: status mapping, JSON parsing and the one repair attempt.

use std::future::Future;

use pluto_contracts::error::{VoiceError, VoiceErrorCode, VoiceResult};
use pluto_contracts::llm::{LlmMessage, LlmRequest, LlmRole};
use reqwest::StatusCode;
use serde_json::Value;

/// A fixed message per failure. Provider bodies can echo the prompt, so they are never copied.
pub(crate) fn map_status(status: StatusCode) -> VoiceError {
    match status.as_u16() {
        401 | 403 => VoiceError::new(
            VoiceErrorCode::InvalidApiKey,
            "The saved key was refused. Check it in Settings.",
        ),
        429 => VoiceError::new(
            VoiceErrorCode::RateLimited,
            "The service is busy right now. Try again in a moment.",
        ),
        500..=599 => VoiceError::new(
            VoiceErrorCode::LlmUnavailable,
            "The language service had a problem.",
        ),
        _ => VoiceError::new(
            VoiceErrorCode::LlmUnavailable,
            "The language service refused the request.",
        )
        .retryable(false),
    }
}

pub(crate) fn map_transport(err: &reqwest::Error) -> VoiceError {
    if err.is_timeout() {
        VoiceError::new(
            VoiceErrorCode::LlmUnavailable,
            "The language service took too long.",
        )
        .retryable(true)
    } else if err.is_connect() {
        VoiceError::new(
            VoiceErrorCode::NetworkOffline,
            "Pluto couldn't reach the internet.",
        )
    } else {
        VoiceError::new(
            VoiceErrorCode::LlmUnavailable,
            "The language service could not be reached.",
        )
        .retryable(true)
    }
}

pub(crate) fn invalid_output() -> VoiceError {
    VoiceError::new(
        VoiceErrorCode::LlmInvalidOutput,
        "The language service gave an answer Pluto couldn't use.",
    )
}

pub(crate) fn missing_key() -> VoiceError {
    VoiceError::new(
        VoiceErrorCode::MissingApiKey,
        "Add this service's key in Settings first.",
    )
}

/// Models sometimes wrap JSON in a code fence.
fn strip_fence(text: &str) -> &str {
    let t = text.trim();
    let t = t
        .strip_prefix("```json")
        .or_else(|| t.strip_prefix("```"))
        .unwrap_or(t);
    t.strip_suffix("```").unwrap_or(t).trim()
}

/// Parse and check the shape: valid JSON, an object or array, and every top-level `required` key.
pub(crate) fn parse_checked(text: &str, schema: &Value) -> VoiceResult<Value> {
    let value: Value = serde_json::from_str(strip_fence(text)).map_err(|_| invalid_output())?;
    if !(value.is_object() || value.is_array()) {
        return Err(invalid_output());
    }
    if let (Some(obj), Some(required)) = (
        value.as_object(),
        schema.get("required").and_then(Value::as_array),
    ) {
        if required
            .iter()
            .filter_map(Value::as_str)
            .any(|k| !obj.contains_key(k))
        {
            return Err(invalid_output());
        }
    }
    Ok(value)
}

/// Send the request; if the answer is not usable JSON, ask once more with the bad answer shown.
pub(crate) async fn with_repair<F, Fut>(req: &LlmRequest, mut send: F) -> VoiceResult<Value>
where
    F: FnMut(LlmRequest) -> Fut,
    Fut: Future<Output = VoiceResult<String>>,
{
    let first = send(req.clone()).await?;
    match parse_checked(&first, &req.json_schema) {
        Ok(v) => Ok(v),
        Err(_) => {
            let mut again = req.clone();
            again.messages.push(LlmMessage {
                role: LlmRole::Assistant,
                content: first,
            });
            again.messages.push(LlmMessage {
                role: LlmRole::User,
                content:
                    "That was not valid JSON for the schema. Reply with only the corrected JSON."
                        .into(),
            });
            let second = send(again).await?;
            parse_checked(&second, &req.json_schema)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pluto_contracts::llm::LlmTask;
    use serde_json::json;

    fn req() -> LlmRequest {
        LlmRequest {
            task: LlmTask::Extract,
            system: "s".into(),
            messages: vec![],
            json_schema: json!({"type":"object","required":["items"]}),
            timeout_ms: 1000,
        }
    }

    #[test]
    fn fences_and_required_keys_are_handled() {
        assert!(parse_checked("```json\n{\"items\":[]}\n```", &req().json_schema).is_ok());
        assert!(
            parse_checked("{\"other\":1}", &req().json_schema).is_err(),
            "missing required key"
        );
        assert!(parse_checked("42", &req().json_schema).is_err());
        assert!(parse_checked("not json", &req().json_schema).is_err());
    }

    #[tokio::test]
    async fn one_repair_attempt_then_gives_up() {
        let mut calls = 0;
        let ok = with_repair(&req(), |r| {
            calls += 1;
            let reply = if calls == 1 {
                "oops".to_owned()
            } else {
                "{\"items\":[]}".to_owned()
            };
            assert_eq!(
                r.messages.len(),
                if calls == 1 { 0 } else { 2 },
                "second call shows the bad answer"
            );
            async move { Ok(reply) }
        })
        .await;
        assert!(ok.is_ok());
        assert_eq!(calls, 2);

        let mut calls = 0;
        let bad = with_repair(&req(), |_| {
            calls += 1;
            async { Ok("still not json".to_owned()) }
        })
        .await;
        assert_eq!(bad.unwrap_err().code, VoiceErrorCode::LlmInvalidOutput);
        assert_eq!(calls, 2, "exactly one repair attempt");
    }

    #[test]
    fn statuses_map_to_the_documented_codes() {
        assert_eq!(
            map_status(StatusCode::TOO_MANY_REQUESTS).code,
            VoiceErrorCode::RateLimited
        );
        assert_eq!(
            map_status(StatusCode::UNAUTHORIZED).code,
            VoiceErrorCode::InvalidApiKey
        );
        assert_eq!(
            map_status(StatusCode::BAD_GATEWAY).code,
            VoiceErrorCode::LlmUnavailable
        );
        assert!(map_status(StatusCode::BAD_GATEWAY).retryable);
    }
}
