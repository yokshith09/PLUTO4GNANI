//! Test doubles for the D1 traits `ToolExecutor` and `FrameSink` (feature `mock`).
//!
//! Owner: **D1** (T-004). `MockStt` and `MockTts` live in `pluto-gnani::mock`. They are here, not
//! in `pluto-voice`, because `pluto-intel` needs them and never depends on `pluto-voice`.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde_json::Value;

use crate::error::{VoiceError, VoiceErrorCode, VoiceResult};
use crate::speech::{AudioFrame, FrameSink};
use crate::voice::{tools, Proposal, ProposalRequest, ToolExecutor, ToolResult};

/// How long a proposal stays open (ARCHITECTURE §5).
pub const PROPOSAL_WINDOW_MS: i64 = 30_000;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// A `ToolExecutor` that records every call. Read tools return a scripted result (default: ok,
/// "ok"); write tools are refused by `run_read`; `propose` stores and returns the proposal without
/// running anything.
#[derive(Default)]
pub struct MockToolExecutor {
    reads: Mutex<Vec<(String, Value)>>,
    proposals: Mutex<Vec<Proposal>>,
    scripted: Mutex<HashMap<String, VoiceResult<ToolResult>>>,
}

impl MockToolExecutor {
    pub fn new() -> Self {
        Self::default()
    }

    /// The result `run_read(tool, …)` returns from now on.
    pub fn script(&self, tool: &str, result: VoiceResult<ToolResult>) {
        if let Ok(mut s) = self.scripted.lock() {
            s.insert(tool.to_string(), result);
        }
    }

    /// Every `run_read` call so far, in order.
    pub fn reads(&self) -> Vec<(String, Value)> {
        self.reads.lock().map(|r| r.clone()).unwrap_or_default()
    }

    /// Every proposal created so far, in order.
    pub fn proposals(&self) -> Vec<Proposal> {
        self.proposals.lock().map(|p| p.clone()).unwrap_or_default()
    }
}

#[async_trait]
impl ToolExecutor for MockToolExecutor {
    async fn run_read(&self, tool: &str, args: &Value) -> VoiceResult<ToolResult> {
        if let Ok(mut r) = self.reads.lock() {
            r.push((tool.to_string(), args.clone()));
        }
        if tools::WRITE.contains(&tool) {
            return Err(VoiceError::new(
                VoiceErrorCode::ToolFailed,
                "That change needs your approval first.",
            ));
        }
        let scripted = self.scripted.lock().ok().and_then(|s| s.get(tool).cloned());
        scripted.unwrap_or_else(|| {
            Ok(ToolResult {
                tool: tool.to_string(),
                ok: true,
                message: "ok".into(),
                record_id: None,
            })
        })
    }

    async fn propose(&self, request: ProposalRequest) -> VoiceResult<Proposal> {
        if !tools::WRITE.contains(&request.tool.as_str()) {
            return Err(VoiceError::new(
                VoiceErrorCode::ToolFailed,
                "Only changes need a proposal.",
            ));
        }
        let proposal = Proposal {
            id: pluto_core::ids::new_id(),
            interaction_id: request
                .interaction_id
                .unwrap_or_else(pluto_core::ids::new_id),
            tool: request.tool,
            args: request.args,
            summary_text: request.summary_text,
            language: request.language,
            expires_at: now_ms() + PROPOSAL_WINDOW_MS,
        };
        if let Ok(mut p) = self.proposals.lock() {
            p.push(proposal.clone());
        }
        Ok(proposal)
    }
}

/// A `FrameSink` that keeps every frame and every ended session. `failing()` makes `push` return
/// an error, to test that the fan-out keeps feeding the other sinks.
#[derive(Default)]
pub struct RecordingSink {
    frames: Mutex<Vec<AudioFrame>>,
    ended: Mutex<Vec<String>>,
    fail: bool,
}

impl RecordingSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn failing() -> Self {
        Self {
            fail: true,
            ..Self::default()
        }
    }

    pub fn frames(&self) -> Vec<AudioFrame> {
        self.frames.lock().map(|f| f.clone()).unwrap_or_default()
    }

    pub fn ended(&self) -> Vec<String> {
        self.ended.lock().map(|e| e.clone()).unwrap_or_default()
    }
}

impl FrameSink for RecordingSink {
    fn name(&self) -> &'static str {
        "recording"
    }

    fn push(&self, frame: &AudioFrame) -> VoiceResult<()> {
        if self.fail {
            return Err(VoiceError::new(VoiceErrorCode::Core, "sink failed"));
        }
        if let Ok(mut f) = self.frames.lock() {
            f.push(frame.clone());
        }
        Ok(())
    }

    fn end(&self, session_id: &str) -> VoiceResult<()> {
        if let Ok(mut e) = self.ended.lock() {
            e.push(session_id.to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::speech::FRAME_BYTES;
    use serde_json::json;

    fn block_on<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(f)
    }

    #[test]
    fn reads_run_and_writes_are_refused_without_a_proposal() {
        let ex = MockToolExecutor::new();
        let ok = block_on(ex.run_read(tools::TASKS_LIST, &json!({"view": "today"}))).unwrap();
        assert!(ok.ok);
        let err = block_on(ex.run_read(tools::TASK_CREATE, &json!({}))).unwrap_err();
        assert_eq!(err.code, VoiceErrorCode::ToolFailed);
        assert_eq!(ex.reads().len(), 2);
    }

    #[test]
    fn propose_stores_a_pending_proposal_with_a_30_second_window() {
        let ex = MockToolExecutor::new();
        let before = now_ms();
        let p = block_on(ex.propose(ProposalRequest {
            interaction_id: None,
            tool: tools::REMINDER_CREATE.into(),
            args: json!({"title": "DBMS submit", "remind_at": 1}),
            summary_text: "Reminder 'DBMS submit' for 5 PM tomorrow. Shall I set it?".into(),
            language: "hi-IN".into(),
        }))
        .unwrap();
        assert!(!p.id.is_empty() && !p.interaction_id.is_empty());
        assert!(p.expires_at >= before + PROPOSAL_WINDOW_MS);
        assert_eq!(ex.proposals(), vec![p]);
    }

    #[test]
    fn a_read_tool_cannot_be_proposed() {
        let ex = MockToolExecutor::new();
        let err = block_on(ex.propose(ProposalRequest {
            interaction_id: Some("i".into()),
            tool: tools::TASKS_LIST.into(),
            args: json!({}),
            summary_text: String::new(),
            language: "en-IN".into(),
        }))
        .unwrap_err();
        assert_eq!(err.code, VoiceErrorCode::ToolFailed);
    }

    #[test]
    fn scripted_read_results_are_returned() {
        let ex = MockToolExecutor::new();
        ex.script(
            tools::MEETING_SEARCH,
            Err(VoiceError::new(VoiceErrorCode::LlmUnavailable, "down")),
        );
        let err = block_on(ex.run_read(tools::MEETING_SEARCH, &json!({}))).unwrap_err();
        assert_eq!(err.code, VoiceErrorCode::LlmUnavailable);
    }

    #[test]
    fn the_recording_sink_keeps_frames_and_ends() {
        let sink = RecordingSink::new();
        let frame = AudioFrame {
            session_id: "s".into(),
            seq: 0,
            pcm: vec![0; FRAME_BYTES],
        };
        sink.push(&frame).unwrap();
        sink.end("s").unwrap();
        assert_eq!(sink.frames(), vec![frame.clone()]);
        assert_eq!(sink.ended(), vec!["s".to_string()]);
        assert!(RecordingSink::failing().push(&frame).is_err());
    }
}
