//! Voice runtime, settings, proposal, tool and privacy types; the `ToolExecutor` trait
//! (API_CONTRACTS §3.2, §3.5, §3.6, §3.7).
//!
//! Owner: **D1** (T-004). Changes go through a `contract` PR that D2 approves.
//!
//! `AssistantTurn` (§3.5) embeds D2's `AssistantDecision` from `decide.rs`.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::decide::AssistantDecision;
use crate::error::VoiceResult;

// ---- runtime -------------------------------------------------------------------------------

/// The voice runtime's state (ARCHITECTURE §5). Emitted as `voice://status` on every transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum VoiceState {
    /// Cloud voice off or keys missing.
    Disabled,
    Idle,
    /// Push-to-talk held; Realtime session open.
    Listening,
    /// Rules → LLM → proposal or answer.
    Thinking,
    /// Proposal shown and spoken; 30 s window.
    AwaitingApproval,
    /// A core function is running.
    Executing,
    /// TTS playback; the hotkey stops it (barge-in).
    Speaking,
    /// Meeting capture.
    Recording,
    /// An error with one recovery action; `VoiceStatus.detail` says which.
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum VoiceMode {
    Assistant,
    Meeting,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct VoiceStatus {
    pub state: VoiceState,
    pub mode: Option<VoiceMode>,
    /// Short, user-facing text (e.g. "Busy: recording"). Never transcript text.
    pub detail: Option<String>,
    pub session_id: Option<String>,
}

impl VoiceStatus {
    /// A status with only the state set.
    pub fn new(state: VoiceState) -> Self {
        Self {
            state,
            mode: None,
            detail: None,
            session_id: None,
        }
    }
}

// ---- settings ------------------------------------------------------------------------------

/// Prisma output format (`format` header / field).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum SttFormat {
    /// Native script per language (the default; "Native" caption style).
    Codemix,
    /// Romanised ("Romanised" caption style).
    Transliterate,
    Verbatim,
    Transcribe,
}

impl SttFormat {
    /// The value Gnani expects in the `format` header / field.
    pub const fn as_gnani(self) -> &'static str {
        match self {
            Self::Codemix => "codemix",
            Self::Transliterate => "transliterate",
            Self::Verbatim => "verbatim",
            Self::Transcribe => "transcribe",
        }
    }
}

/// Settings keys `voice.*` (DATA_SCHEMA §4) plus whether each key is saved. Keys themselves never
/// leave the keychain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct VoiceSettings {
    pub cloud_voice_enabled: bool,
    /// Empty = `indic-auto`; otherwise 1–4 codes such as `hi-IN`.
    pub languages: Vec<String>,
    pub stt_format: SttFormat,
    pub tts_voice: String,
    pub tts_model: String,
    /// The push-to-talk shortcut, e.g. `Ctrl+Shift+Space`.
    pub hotkey: String,
    pub audio_tags: bool,
    pub always_on_enabled: bool,
    pub has_gnani_key: bool,
    pub has_gemini_key: bool,
    pub has_groq_key: bool,
}

/// Argument of `voice_settings_set`. A missing field is left unchanged. Keys are set with
/// `voice_key_set`, so the `has_*_key` flags are not patchable.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct VoiceSettingsPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub cloud_voice_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub languages: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub stt_format: Option<SttFormat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub tts_voice: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub tts_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub hotkey: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub audio_tags: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub always_on_enabled: Option<bool>,
}

// ---- tools and proposals -------------------------------------------------------------------

/// Tool names (API_CONTRACTS §3.5). Every tool calls an existing `pluto-core` function.
pub mod tools {
    pub const TASKS_LIST: &str = "tasks_list";
    pub const TASK_CREATE: &str = "task_create";
    pub const TASK_COMPLETE: &str = "task_complete";
    pub const TASK_UPDATE: &str = "task_update";
    pub const REMINDERS_LIST: &str = "reminders_list";
    pub const REMINDER_CREATE: &str = "reminder_create";
    pub const REMINDER_DONE: &str = "reminder_done";
    pub const MEETING_SEARCH: &str = "meeting_search";
    pub const COMMITMENTS_LIST: &str = "commitments_list";
    pub const NOTES_SEARCH: &str = "notes_search";
    pub const NOTE_CREATE: &str = "note_create";
    pub const FOCUS_START: &str = "focus_start";
    pub const FOCUS_STOP: &str = "focus_stop";
    pub const PROJECTS_LIST: &str = "projects_list";
    /// Switches the dashboard page. Changes no data, so it runs directly.
    pub const OPEN_PAGE: &str = "open_page";

    /// Tools that change data. They run only after `voice_proposal_respond(Approve)`.
    pub const WRITE: &[&str] = &[
        TASK_CREATE,
        TASK_COMPLETE,
        TASK_UPDATE,
        REMINDER_CREATE,
        REMINDER_DONE,
        NOTE_CREATE,
        FOCUS_START,
        FOCUS_STOP,
    ];

    /// Tools that only read. They run directly.
    pub const READ: &[&str] = &[
        TASKS_LIST,
        REMINDERS_LIST,
        MEETING_SEARCH,
        COMMITMENTS_LIST,
        NOTES_SEARCH,
        PROJECTS_LIST,
        OPEN_PAGE,
    ];
}

/// Dashboard pages voice can open (`open_page {page}`), as the UI names them (`Page` in
/// `src/features/shell/Sidebar.tsx`).
pub mod pages {
    pub const ALL: &[&str] = &[
        "today",
        "tasks",
        "time",
        "notes",
        "projects",
        "study",
        "meetings",
        "voice",
        "archive",
        "bin",
        "character",
        "mcp",
        "settings",
    ];
    /// Hidden by the demo view (`advanced.demo_pages_only`; `DEMO_HIDDEN` in `pages.ts`).
    pub const DEMO_HIDDEN: &[&str] = &["time", "study", "character", "mcp", "archive"];

    /// The page a spoken word means: "home" → `today`, "reminders" → `tasks` (they live there),
    /// "trash" → `bin`, "history" → `voice`. `None` when it names no page.
    pub fn named(word: &str) -> Option<&'static str> {
        let w = word.trim().to_lowercase();
        let w = w
            .trim_start_matches("the ")
            .trim_end_matches(" page")
            .trim_end_matches(" tab")
            .trim();
        let page = match w {
            "home" | "dashboard" | "overview" | "today" | "aaj" => "today",
            "task" | "tasks" | "todo" | "todos" | "to-do" | "to do" | "kaam" | "reminder"
            | "reminders" => "tasks",
            "note" | "notes" | "notebook" => "notes",
            "project" | "projects" => "projects",
            "meeting" | "meetings" | "recordings" | "transcripts" => "meetings",
            "voice" | "assistant" | "history" | "voice history" => "voice",
            "bin" | "trash" | "recycle bin" | "deleted" => "bin",
            "setting" | "settings" | "preferences" | "options" => "settings",
            "time" | "screen time" | "usage" => "time",
            "study" | "student" => "study",
            "archive" | "archived" => "archive",
            "character" | "pet" => "character",
            "mcp" => "mcp",
            _ => return None,
        };
        Some(page)
    }
}

/// A pending write, waiting for the user's answer. Emitted as `voice://proposal`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Proposal {
    pub id: String,
    pub interaction_id: String,
    pub tool: String,
    /// Resolved arguments (dates as `YYYY-MM-DD`, times as ms), as the tool will receive them.
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub args: Value,
    /// What will happen, in the reply language. Shown and spoken.
    pub summary_text: String,
    pub language: String,
    /// ms since the Unix epoch; 30 s after creation.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub expires_at: i64,
}

/// The user's answer to a proposal. Tagged by `kind`:
/// `{"kind":"approve"}`, `{"kind":"reject"}`, `{"kind":"edit","args":{…}}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProposalResponse {
    Approve,
    Reject,
    /// Approve with changed arguments.
    Edit {
        #[cfg_attr(feature = "ts", ts(type = "unknown"))]
        args: Value,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ToolResult {
    pub tool: String,
    pub ok: bool,
    /// What happened, in the reply language (spoken: "Ho gaya.").
    pub message: String,
    /// The created or changed record, when there is one.
    pub record_id: Option<String>,
}

/// One assistant turn: what was heard, what was decided, and what happened. Emitted as
/// `voice://turn` and listed by `voice_history` (newest first).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AssistantTurn {
    pub interaction_id: String,
    /// What the user said (shown in the history; never logged above `debug`).
    pub transcript: String,
    pub language: Option<String>,
    pub decision: AssistantDecision,
    /// What the assistant said back, also shown as text.
    pub reply_text: Option<String>,
    /// The write it proposed, if any.
    pub proposal: Option<Proposal>,
    /// The tool's result, once it ran.
    pub result: Option<ToolResult>,
}

/// What a caller asks `ToolExecutor::propose` to create. Not on the wire.
#[derive(Debug, Clone, PartialEq)]
pub struct ProposalRequest {
    /// The assistant turn this belongs to; `None` makes the executor log a new interaction
    /// (e.g. a recall started from the Meetings page).
    pub interaction_id: Option<String>,
    pub tool: String,
    pub args: Value,
    pub summary_text: String,
    pub language: String,
}

/// Runs tools over `pluto-core`. Implemented by `pluto-voice::assistant`; used by `pluto-intel`
/// (recall follow-up proposals) and the assistant itself. **The only path to a write is
/// `propose` followed by the user's approval through `voice_proposal_respond`** (T-AS-05).
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Runs a read tool directly. A write tool returns `ToolFailed` without doing anything.
    async fn run_read(&self, tool: &str, args: &Value) -> VoiceResult<ToolResult>;

    /// Stores a pending proposal for a write tool and returns it. Never executes anything.
    async fn propose(&self, request: ProposalRequest) -> VoiceResult<Proposal>;
}

// ---- privacy and usage ---------------------------------------------------------------------

/// Where outbound data went. Stored as the wire spelling in `privacy_log.destination`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Destination {
    GnaniRealtime,
    GnaniRest,
    GnaniBatch,
    GnaniTts,
    GnaniClone,
    Gemini,
    Groq,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PrivacyLogEntry {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub id: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub at: i64,
    pub destination: Destination,
    /// Why it was sent, e.g. "assistant command", "meeting transcript". Never content.
    pub purpose: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub bytes: i64,
    pub ref_id: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct UsageCounters {
    pub stt_seconds_today: f64,
    pub batch_seconds_today: f64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub tts_chars_today: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub gemini_calls_today: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub groq_calls_today: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn states_use_snake_case() {
        assert_eq!(
            serde_json::to_value(VoiceState::AwaitingApproval).unwrap(),
            "awaiting_approval"
        );
        let s = VoiceStatus {
            mode: Some(VoiceMode::Assistant),
            ..VoiceStatus::new(VoiceState::Listening)
        };
        assert_eq!(
            serde_json::to_value(&s).unwrap(),
            json!({"state": "listening", "mode": "assistant", "detail": null, "session_id": null})
        );
    }

    #[test]
    fn proposal_response_is_tagged_by_kind() {
        assert_eq!(
            serde_json::to_value(ProposalResponse::Approve).unwrap(),
            json!({"kind": "approve"})
        );
        let edit: ProposalResponse =
            serde_json::from_value(json!({"kind": "edit", "args": {"due_date": "2026-11-09"}}))
                .unwrap();
        assert_eq!(
            edit,
            ProposalResponse::Edit {
                args: json!({"due_date": "2026-11-09"})
            }
        );
    }

    #[test]
    fn a_patch_with_missing_fields_changes_nothing() {
        let p: VoiceSettingsPatch =
            serde_json::from_value(json!({"hotkey": "Ctrl+Shift+Space"})).unwrap();
        assert_eq!(
            p,
            VoiceSettingsPatch {
                hotkey: Some("Ctrl+Shift+Space".into()),
                ..Default::default()
            }
        );
    }

    #[test]
    fn an_assistant_turn_round_trips_with_its_decision() {
        use crate::decide::{DecisionSource, Intent};
        let turn = AssistantTurn {
            interaction_id: "i1".into(),
            transcript: "aaj ke tasks batao".into(),
            language: Some("hi-IN".into()),
            decision: AssistantDecision {
                intent: Intent::Task,
                tool: Some(tools::TASKS_LIST.into()),
                args: json!({"view": "today"}),
                confidence: Some(0.9),
                source: DecisionSource::Llm,
                latency_ms: 640,
            },
            reply_text: Some("Aaj 3 tasks hain.".into()),
            proposal: None,
            result: None,
        };
        let v = serde_json::to_value(&turn).unwrap();
        assert_eq!(v["decision"]["source"], "llm");
        assert_eq!(v["decision"]["intent"], "task");
        assert!(v["proposal"].is_null());
        let back: AssistantTurn = serde_json::from_value(v).unwrap();
        assert_eq!(back, turn);
    }

    #[test]
    fn every_tool_is_either_read_or_write_and_never_both() {
        for t in tools::WRITE {
            assert!(!tools::READ.contains(t), "{t} is listed as both");
        }
        assert_eq!(tools::WRITE.len() + tools::READ.len(), 15);
    }

    #[test]
    fn destinations_match_the_privacy_log_check() {
        let all = [
            (Destination::GnaniRealtime, "gnani_realtime"),
            (Destination::GnaniRest, "gnani_rest"),
            (Destination::GnaniBatch, "gnani_batch"),
            (Destination::GnaniTts, "gnani_tts"),
            (Destination::GnaniClone, "gnani_clone"),
            (Destination::Gemini, "gemini"),
            (Destination::Groq, "groq"),
        ];
        for (d, wire) in all {
            assert_eq!(serde_json::to_value(d).unwrap(), wire);
        }
    }

    #[test]
    fn stt_format_matches_gnani_and_the_wire() {
        for f in [
            SttFormat::Codemix,
            SttFormat::Transliterate,
            SttFormat::Verbatim,
            SttFormat::Transcribe,
        ] {
            assert_eq!(serde_json::to_value(f).unwrap(), f.as_gnani());
        }
    }
}
