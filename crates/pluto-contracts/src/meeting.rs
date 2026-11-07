//! Transcript, meeting-AI, extraction and commitment types (API_CONTRACTS §3.3).
//!
//! Owner: **D2** (T-005).

use pluto_core::tasks::Priority;

use async_trait::async_trait;

use crate::error::VoiceResult;
use crate::voice::Proposal;
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeetingAiStatus {
    pub meeting_id: String,
    pub status: AiStatus,
    pub dominant_language: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub updated_at: i64,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiStatus {
    Recording,
    Transcribing,
    NeedsSpeakers,
    NeedsReview,
    Done,
    Failed,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentSource {
    Realtime,
    Batch,
    Rest,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relevance {
    Decision,
    ActionItem,
    Question,
    Risk,
    Chitchat,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub id: String,
    pub meeting_id: Option<String>,
    pub source: SegmentSource,
    pub index: u32,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub start_ms: Option<i64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub end_ms: Option<i64>,
    pub speaker_label: Option<u8>,
    pub participant: Option<String>,
    pub text: String,
    pub language: Option<String>,
    pub relevance: Option<Relevance>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordingSession {
    pub id: String,
    pub meeting_id: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub started_at: i64,
    pub live_captions: bool,
    pub chunk_count: u32,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptionJob {
    pub id: String,
    pub meeting_id: String,
    pub gnani_job_id: Option<String>,
    pub status: JobStatus,
    pub error: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub updated_at: i64,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Uploading,
    Created,
    InProgress,
    Completed,
    PartialFailure,
    Failed,
    Cancelled,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeakerMapping {
    pub speaker_label: u8,
    pub participant: Option<String>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractedItem {
    pub id: String,
    pub kind: Relevance,
    pub body: String,
    pub owner: Option<String>,
    pub due_phrase: Option<String>,
    pub due_date: Option<String>,
    pub priority: Option<Priority>,
    pub source_segment_id: String,
    pub quote: String,
    pub duplicate_of_task_id: Option<String>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QualityWarning {
    NoDecisions,
    ActionItemWithoutOwner { item_id: String },
    UnresolvedDueDate { item_id: String },
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractionResult {
    pub meeting_id: String,
    pub items: Vec<ExtractedItem>,
    pub warnings: Vec<QualityWarning>,
    pub dominant_language: Option<String>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditedItem {
    pub item_id: String,
    pub body: String,
    pub owner: Option<String>,
    pub due_date: Option<String>,
    pub priority: Option<Priority>,
    pub create_task: bool,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyExtraction {
    pub meeting_id: String,
    pub accepted: Vec<EditedItem>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Citation {
    pub meeting_id: String,
    pub meeting_title: String,
    pub segment_id: String,
    pub speaker: Option<String>,
    pub quote: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub starts_at: i64,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecallAnswer {
    pub answer_text: String,
    pub language: String,
    pub citations: Vec<Citation>,
    pub proposal: Option<Proposal>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commitment {
    pub action_item_id: String,
    pub meeting_id: String,
    pub meeting_title: String,
    pub owner: Option<String>,
    pub body: String,
    pub due_date: Option<String>,
    pub task_id: Option<String>,
    pub status: CommitmentStatus,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub days_overdue: Option<i64>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitmentStatus {
    Open,
    Done,
    Overdue,
    NoTask,
}

/// What the voice assistant may ask of the meeting knowledge: a grounded answer from the user's
/// own transcripts and the list of commitments. Implemented by `pluto-intel`; used by
/// `pluto-voice`, which never depends on `pluto-intel` (WORKING_AGREEMENT §3). Read-only.
#[async_trait]
pub trait MeetingKnowledge: Send + Sync {
    async fn recall(&self, question: &str, language: Option<&str>) -> VoiceResult<RecallAnswer>;
    async fn open_commitments(&self) -> VoiceResult<Vec<Commitment>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enums_use_snake_case_on_the_wire() {
        assert_eq!(
            serde_json::to_string(&Relevance::ActionItem).unwrap(),
            "\"action_item\""
        );
        assert_eq!(
            serde_json::to_string(&AiStatus::NeedsReview).unwrap(),
            "\"needs_review\""
        );
    }

    #[test]
    fn priority_keeps_the_bases_spelling() {
        let item = EditedItem {
            item_id: "i".into(),
            body: "b".into(),
            owner: None,
            due_date: None,
            priority: Some(Priority::High),
            create_task: true,
        };
        let v = serde_json::to_value(&item).unwrap();
        assert_eq!(v["priority"], "HIGH");
        assert!(v["owner"].is_null());
    }

    #[test]
    fn quality_warning_round_trips() {
        let w = QualityWarning::ActionItemWithoutOwner {
            item_id: "x".into(),
        };
        let back: QualityWarning =
            serde_json::from_str(&serde_json::to_string(&w).unwrap()).unwrap();
        assert_eq!(w, back);
    }
}
