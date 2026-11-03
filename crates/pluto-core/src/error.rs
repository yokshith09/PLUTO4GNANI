//! The one error type that crosses the IPC boundary (TRD §6.9).
//!
//! Messages are written for the person using Pluto: what happened and what is still
//! safe (UI brief §2). They never contain note, task or meeting content.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    Validation,
    NotFound,
    InvalidTransition,
    Conflict,
    Db,
    Io,
    Internal,
}

/// Serializes to `{ "code": "...", "message": "...", "field": "..." }` (`field` omitted when absent).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{code:?}: {message}")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub field: Option<String>,
}

pub type Result<T> = std::result::Result<T, AppError>;

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            field: None,
        }
    }

    pub fn with_field(mut self, field: impl Into<String>) -> Self {
        self.field = Some(field.into());
        self
    }

    pub fn validation(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Validation, message).with_field(field)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }

    pub fn invalid_transition(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidTransition, message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Conflict, message)
    }

    pub fn db(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Db, message)
    }

    pub fn io(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Io, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message)
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        // Only the error kind: the OS message can embed file paths.
        Self::io(format!(
            "Couldn't read or write a file ({:?}). Your data is unchanged.",
            e.kind()
        ))
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        // The driver's message can quote SQL and values, which may be user content. Keep it out
        // of anything shown or logged; callers that can say something more helpful (a duplicate
        // label, say) map the specific constraint themselves.
        let _ = e;
        Self::db("Couldn't complete that. Your data is unchanged.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_code_serializes_to_its_documented_name() {
        let cases = [
            (ErrorCode::Validation, "VALIDATION"),
            (ErrorCode::NotFound, "NOT_FOUND"),
            (ErrorCode::InvalidTransition, "INVALID_TRANSITION"),
            (ErrorCode::Conflict, "CONFLICT"),
            (ErrorCode::Db, "DB"),
            (ErrorCode::Io, "IO"),
            (ErrorCode::Internal, "INTERNAL"),
        ];
        for (code, name) in cases {
            let v = serde_json::to_value(AppError::new(code, "m")).unwrap();
            assert_eq!(v, json!({ "code": name, "message": "m" }));
        }
    }

    #[test]
    fn field_is_included_only_when_present() {
        let v = serde_json::to_value(AppError::validation("title", "Title is too long.")).unwrap();
        assert_eq!(
            v,
            json!({ "code": "VALIDATION", "message": "Title is too long.", "field": "title" })
        );
    }

    #[test]
    fn round_trips_through_json() {
        let e = AppError::conflict("That label already exists.").with_field("name");
        let back: AppError = serde_json::from_str(&serde_json::to_string(&e).unwrap()).unwrap();
        assert_eq!(e, back);
    }

    #[test]
    fn io_errors_keep_the_kind_but_not_the_path() {
        let io = std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "/home/secret/notes.db: denied",
        );
        let e = AppError::from(io);
        assert_eq!(e.code, ErrorCode::Io);
        assert!(e.message.contains("PermissionDenied"));
        assert!(
            !e.message.contains("secret"),
            "paths must not leak into user-facing messages"
        );
    }
}
