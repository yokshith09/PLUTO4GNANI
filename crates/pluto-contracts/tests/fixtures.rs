//! T-CT-01 / T-CT-05: every shared fixture deserializes into its Rust type, round-trips, and
//! carries the current `CONTRACT_VERSION`.

use std::path::{Path, PathBuf};

use pluto_contracts::meeting::{
    ExtractionResult, RecallAnswer, TranscriptSegment, TranscriptionJob,
};
use pluto_contracts::voice::{ToolResult, UsageCounters, VoiceSettings, VoiceStatus};
use pluto_contracts::CONTRACT_VERSION;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/fixtures")
}

fn check<T: DeserializeOwned + Serialize>(file: &str) {
    let path = dir().join(file);
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        doc["contract_version"], CONTRACT_VERSION,
        "{file}: contract_version"
    );
    let data = doc["data"].clone();
    let typed: T = serde_json::from_value(data.clone()).unwrap_or_else(|e| panic!("{file}: {e}"));
    assert_eq!(
        serde_json::to_value(&typed).unwrap(),
        data,
        "{file}: round trip"
    );
}

#[test]
fn voice_fixtures_match_their_types() {
    check::<VoiceSettings>("voice/voice_settings.default.json");
    check::<VoiceSettings>("voice/voice_settings.enabled.json");
    check::<VoiceStatus>("voice/voice_status.idle.json");
    check::<VoiceStatus>("voice/voice_status.busy_recording.json");
    check::<UsageCounters>("voice/usage_counters.sample.json");
    check::<ToolResult>("voice/tool_result.ok.json");
}

#[test]
fn meeting_fixtures_match_their_types() {
    check::<Vec<TranscriptSegment>>("meetings/transcript_segments.as1.json");
    check::<TranscriptionJob>("meetings/transcription_job.completed.json");
    check::<ExtractionResult>("meetings/extraction_result.as1.json");
    check::<RecallAnswer>("meetings/recall_answer.ok.json");
}

#[test]
fn every_fixture_file_is_covered_above() {
    let mut files = 0;
    for sub in ["voice", "meetings"] {
        files += std::fs::read_dir(dir().join(sub)).unwrap().count();
    }
    assert_eq!(files, 10, "add new fixtures to the checks above");
}
