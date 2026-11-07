//! Prisma Batch client: create, start, poll, files, transcript; segments with chunk offsets.
//!
//! Owner: **D1** (T-201). API_CONTRACTS §2.3, checked against Gnani's Batch reference
//! (docs.gnani.ai `STTBatch/*`):
//!
//! - `POST /stt/v3/batch/jobs` (multipart `config` JSON + `files`) → `{job_id, status: CREATED}`
//! - `POST …/{job_id}/start` → `{status: STARTING}`
//! - `GET …/{job_id}` → `{status, progress: {total_files, completed_files, failed_files}, cancel_reason}`
//! - `GET …/{job_id}/files` → `{data: [{file_id, original_path, status, transcript_url,
//!   error_message, error_code}], pagination: {has_more, next_cursor}}`
//! - `transcript_url` is a pre-signed link (1 hour) to `{segments: [{segment_id, speaker_id,
//!   start_time, end_time, text, language_detected}]}`, times in seconds.
//!
//! Every Gnani call goes through [`GnaniHttp`] (one Batch request in flight, retry, error mapping,
//! privacy log). The transcript download is the exception: the pre-signed link points at Gnani's
//! storage and needs no key, so it is fetched with a plain client and **the API key is never sent
//! to it**. It is still written to the privacy log.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use pluto_contracts::error::{VoiceError, VoiceErrorCode, VoiceResult};
use reqwest::multipart::{Form, Part};
use serde::Serialize;
use serde_json::Value;

use crate::http::{Call, Family, GnaniHttp, OutboundLog, OutboundRecord, REQUEST_TIMEOUT};

pub const JOBS_PATH: &str = "/stt/v3/batch/jobs";
pub const DEFAULT_MODEL: &str = "prisma-v3.0";
/// Gnani's limit for one directly uploaded file.
pub const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;
pub const MAX_FILES: usize = 100;
/// Formats Pluto offers in the import dialog (and Gnani accepts).
pub const AUDIO_EXTENSIONS: [&str; 5] = ["wav", "mp3", "m4a", "flac", "ogg"];

// ---- config --------------------------------------------------------------------------------

/// The job's `config` part. Empty optional fields are left out, so Gnani applies its defaults.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BatchConfig {
    pub model: String,
    pub language_code: String,
    pub format: String,
    pub with_diarization: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_speakers: Option<u8>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub speaker_names: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub keyterms: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
}

impl BatchConfig {
    /// The config for a meeting, inside Gnani's limits: `speaker_names` only when there are 1–4
    /// participants (Gnani's maximum), otherwise `num_speakers` = the count (1–8); at most 200
    /// keyterms; a topic only when it is 30–1,600 characters.
    pub fn for_meeting(participants: &[String], keyterms: &[String], topic: Option<&str>) -> Self {
        let names: Vec<String> = participants
            .iter()
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect();
        let (num_speakers, speaker_names) = match names.len() {
            0 => (None, vec![]),
            1..=4 => (None, names),
            n => (Some(n.min(8) as u8), vec![]),
        };
        let topic = topic
            .map(str::trim)
            .filter(|t| (30..=1600).contains(&t.chars().count()))
            .map(str::to_string);
        Self {
            model: DEFAULT_MODEL.into(),
            language_code: "indic-auto".into(),
            format: "codemix".into(),
            with_diarization: true,
            num_speakers,
            speaker_names,
            keyterms: keyterms
                .iter()
                .map(|k| k.trim().to_string())
                .filter(|k| !k.is_empty())
                .take(200)
                .collect(),
            topic,
        }
    }
}

/// One file to upload. `name` is how Gnani reports it back (`original_path`), so it must be
/// unique within the job: the pipeline uses `chunk-0000.wav`, `chunk-0001.wav`, …
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

// ---- job state -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchStatus {
    Created,
    Starting,
    Queued,
    InProgress,
    Completed,
    PartialFailure,
    Failed,
    StartFailed,
    Cancelling,
    Cancelled,
    /// A status this client doesn't know; treated as still running.
    Other(String),
}

impl BatchStatus {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_uppercase().as_str() {
            "CREATED" => Self::Created,
            "STARTING" => Self::Starting,
            "QUEUED" => Self::Queued,
            "IN_PROGRESS" => Self::InProgress,
            "COMPLETED" => Self::Completed,
            "PARTIAL_FAILURE" => Self::PartialFailure,
            "FAILED" => Self::Failed,
            "START_FAILED" => Self::StartFailed,
            "CANCELLING" => Self::Cancelling,
            "CANCELLED" => Self::Cancelled,
            other => Self::Other(other.to_string()),
        }
    }

    /// The job will not change any more.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed
                | Self::PartialFailure
                | Self::Failed
                | Self::StartFailed
                | Self::Cancelled
        )
    }

    /// Some files may have transcripts to fetch.
    pub fn has_results(&self) -> bool {
        matches!(self, Self::Completed | Self::PartialFailure)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobState {
    pub status: BatchStatus,
    pub total_files: u32,
    pub completed_files: u32,
    pub failed_files: u32,
    pub cancel_reason: Option<String>,
}

/// One file of a finished job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileResult {
    pub file_id: Option<String>,
    /// The upload name (`BatchFile::name`).
    pub original_path: String,
    /// `COMPLETED`, `FAILED`, `SKIPPED` or `CANCELLED`.
    pub status: String,
    pub transcript_url: Option<String>,
    pub error_message: Option<String>,
    pub error_code: Option<String>,
}

impl FileResult {
    pub fn is_completed(&self) -> bool {
        self.status.eq_ignore_ascii_case("COMPLETED") && self.transcript_url.is_some()
    }
}

/// A transcript segment, times relative to the start of its file.
#[derive(Debug, Clone, PartialEq)]
pub struct BatchSegment {
    pub start_ms: i64,
    pub end_ms: i64,
    pub speaker: Option<u8>,
    pub text: String,
    pub language: Option<String>,
}

// ---- the API as a trait (so the pipeline can be tested without a server) -----------------

#[async_trait]
pub trait BatchApi: Send + Sync {
    /// Uploads the files and creates the job. Returns Gnani's `job_id`.
    async fn create(
        &self,
        files: &[BatchFile],
        config: &BatchConfig,
        ref_id: &str,
    ) -> VoiceResult<String>;
    async fn start(&self, job_id: &str) -> VoiceResult<BatchStatus>;
    async fn status(&self, job_id: &str) -> VoiceResult<JobState>;
    /// Every file of the job (completed, failed and skipped), all pages.
    async fn files(&self, job_id: &str) -> VoiceResult<Vec<FileResult>>;
    /// Downloads and parses one transcript.
    async fn transcript(&self, url: &str, ref_id: &str) -> VoiceResult<Vec<BatchSegment>>;
    async fn cancel(&self, job_id: &str) -> VoiceResult<()>;
}

// ---- the client ----------------------------------------------------------------------------

pub struct BatchClient {
    http: Arc<GnaniHttp>,
    /// For pre-signed transcript links only: no key header, ever.
    download: reqwest::Client,
    log: Arc<dyn OutboundLog>,
}

impl BatchClient {
    pub fn new(http: Arc<GnaniHttp>, log: Arc<dyn OutboundLog>) -> VoiceResult<Self> {
        let download = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|_| {
                VoiceError::new(VoiceErrorCode::Core, "Couldn't start the network client.")
            })?;
        Ok(Self {
            http,
            download,
            log,
        })
    }

    fn job_path(job_id: &str, tail: &str) -> String {
        format!("{JOBS_PATH}/{}{tail}", encode_segment(job_id))
    }
}

#[async_trait]
impl BatchApi for BatchClient {
    async fn create(
        &self,
        files: &[BatchFile],
        config: &BatchConfig,
        ref_id: &str,
    ) -> VoiceResult<String> {
        check_files(files)?;
        let config_json = serde_json::to_string(config)
            .map_err(|_| VoiceError::new(VoiceErrorCode::Core, "Couldn't prepare the job."))?;
        let bytes: u64 = files.iter().map(|f| f.bytes.len() as u64).sum();
        let url = self.http.url(JOBS_PATH);
        let v: Value = self
            .http
            .execute_json(
                Call::new(Family::Batch, "meeting audio", bytes).ref_id(ref_id),
                |c| {
                    let mut form = Form::new().part(
                        "config",
                        Part::text(config_json.clone())
                            .mime_str("application/json")
                            .expect("static mime"),
                    );
                    for f in files {
                        form = form.part(
                            "files",
                            Part::bytes(f.bytes.clone())
                                .file_name(f.name.clone())
                                .mime_str(mime_for(&f.name))
                                .expect("static mime"),
                        );
                    }
                    c.post(&url).multipart(form)
                },
            )
            .await?;
        str_at(&v, &["job_id", "id"])
            .or_else(|| v.get("data").and_then(|d| str_at(d, &["job_id", "id"])))
            .ok_or_else(unreadable)
    }

    async fn start(&self, job_id: &str) -> VoiceResult<BatchStatus> {
        let url = self.http.url(&Self::job_path(job_id, "/start"));
        let v: Value = self
            .http
            .execute_json(
                Call::new(Family::Batch, "start job", 0).ref_id(job_id),
                |c| c.post(&url),
            )
            .await?;
        Ok(str_at(&v, &["status"])
            .map(|s| BatchStatus::parse(&s))
            .unwrap_or(BatchStatus::Starting))
    }

    async fn status(&self, job_id: &str) -> VoiceResult<JobState> {
        let url = self.http.url(&Self::job_path(job_id, ""));
        let v: Value = self
            .http
            .execute_json(
                Call::new(Family::Batch, "job status", 0).ref_id(job_id),
                |c| c.get(&url),
            )
            .await?;
        let status = str_at(&v, &["status"]).ok_or_else(unreadable)?;
        let count = |k: &str| {
            v.pointer(&format!("/progress/{k}"))
                .and_then(as_u64)
                .unwrap_or(0) as u32
        };
        Ok(JobState {
            status: BatchStatus::parse(&status),
            total_files: count("total_files"),
            completed_files: count("completed_files"),
            failed_files: count("failed_files"),
            cancel_reason: str_at(&v, &["cancel_reason"]),
        })
    }

    async fn files(&self, job_id: &str) -> VoiceResult<Vec<FileResult>> {
        let base = self.http.url(&Self::job_path(job_id, "/files"));
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        // A job has at most 100 files and a page holds 100, so this loops once in practice; the
        // bound stops a server that always says `has_more`.
        for _ in 0..10 {
            let url = match &cursor {
                Some(c) => format!("{base}?limit=100&cursor={}", encode_segment(c)),
                None => format!("{base}?limit=100"),
            };
            let v: Value = self
                .http
                .execute_json(
                    Call::new(Family::Batch, "job files", 0).ref_id(job_id),
                    |c| c.get(&url),
                )
                .await?;
            let items = v
                .get("data")
                .and_then(Value::as_array)
                .ok_or_else(unreadable)?;
            out.extend(items.iter().map(parse_file));
            let next = v
                .pointer("/pagination/next_cursor")
                .and_then(Value::as_str)
                .map(str::to_string);
            let more = v
                .pointer("/pagination/has_more")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            match (more, next) {
                (true, Some(n)) if cursor.as_deref() != Some(n.as_str()) => cursor = Some(n),
                _ => break,
            }
        }
        Ok(out)
    }

    async fn transcript(&self, url: &str, ref_id: &str) -> VoiceResult<Vec<BatchSegment>> {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err(unreadable());
        }
        self.http.cloud().check()?;
        self.log.record(OutboundRecord {
            destination: Family::Batch.destination(),
            purpose: "transcript download".into(),
            bytes: 0,
            ref_id: Some(ref_id.to_string()),
        });
        let mut attempt = 0;
        let resp = loop {
            match self.download.get(url).send().await {
                Ok(r) if r.status().is_success() => break r,
                Ok(r) if r.status().is_server_error() && attempt < 2 => {}
                Ok(r) if r.status().as_u16() == 403 => {
                    return Err(VoiceError::new(
                        VoiceErrorCode::GnaniUpstream,
                        "The transcript link expired. Retry the job to fetch it again.",
                    ))
                }
                Ok(_) => {
                    return Err(VoiceError::new(
                        VoiceErrorCode::GnaniUpstream,
                        "Couldn't download the transcript from Gnani. Try again.",
                    ))
                }
                Err(_) if attempt < 2 => {}
                Err(_) => {
                    return Err(VoiceError::new(
                        VoiceErrorCode::NetworkOffline,
                        "Can't reach Gnani. Check your internet connection.",
                    ))
                }
            }
            attempt += 1;
            tokio::time::sleep(Duration::from_millis(500 << attempt)).await;
        };
        let v: Value = resp.json().await.map_err(|_| unreadable())?;
        parse_transcript(&v)
    }

    async fn cancel(&self, job_id: &str) -> VoiceResult<()> {
        let url = self.http.url(&Self::job_path(job_id, "/cancel"));
        self.http
            .execute(
                Call::new(Family::Batch, "cancel job", 0).ref_id(job_id),
                |c| c.post(&url),
            )
            .await
            .map(|_| ())
    }
}

// ---- parsing -------------------------------------------------------------------------------

/// Refuses what Gnani would refuse, before uploading anything.
pub fn check_files(files: &[BatchFile]) -> VoiceResult<()> {
    if files.is_empty() {
        return Err(VoiceError::new(
            VoiceErrorCode::UnsupportedAudio,
            "There's no audio to transcribe.",
        ));
    }
    if files.len() > MAX_FILES {
        return Err(VoiceError::new(
            VoiceErrorCode::FileTooLarge,
            "That recording has too many parts for one Gnani job (100 at most).",
        ));
    }
    for f in files {
        if f.bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(file_too_large());
        }
        if !is_supported(&f.name) {
            return Err(unsupported());
        }
    }
    Ok(())
}

pub fn is_supported(name: &str) -> bool {
    extension(name).is_some_and(|e| AUDIO_EXTENSIONS.contains(&e.as_str()))
}

pub fn extension(name: &str) -> Option<String> {
    let (_, ext) = name.rsplit_once('.')?;
    Some(ext.to_ascii_lowercase())
}

pub fn file_too_large() -> VoiceError {
    VoiceError::new(
        VoiceErrorCode::FileTooLarge,
        "That file is over 10 MB, Gnani's limit for one upload. Split it or export a smaller copy.",
    )
}

pub fn unsupported() -> VoiceError {
    VoiceError::new(
        VoiceErrorCode::UnsupportedAudio,
        "Pluto can transcribe WAV, MP3, M4A, FLAC or OGG files.",
    )
}

fn mime_for(name: &str) -> &'static str {
    match extension(name).as_deref() {
        Some("wav") => "audio/wav",
        Some("mp3") => "audio/mpeg",
        Some("m4a") => "audio/mp4",
        Some("flac") => "audio/flac",
        Some("ogg") => "audio/ogg",
        _ => "application/octet-stream",
    }
}

fn unreadable() -> VoiceError {
    VoiceError::new(
        VoiceErrorCode::GnaniUpstream,
        "Gnani sent an answer Pluto couldn't read. Try again.",
    )
}

/// Percent-encodes a path segment (job ids are UUIDs; this only guards against odd input).
fn encode_segment(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn str_at(v: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| match v.get(*k)? {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
}

fn as_u64(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str()?.trim().parse().ok())
}

fn as_f64(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_str()?.trim().parse().ok())
}

fn parse_file(v: &Value) -> FileResult {
    FileResult {
        file_id: str_at(v, &["file_id", "id"]),
        original_path: str_at(v, &["original_path", "file_name", "filename"]).unwrap_or_default(),
        status: str_at(v, &["status"])
            .unwrap_or_default()
            .to_ascii_uppercase(),
        transcript_url: str_at(v, &["transcript_url"]),
        error_message: str_at(v, &["error_message", "error"]),
        error_code: str_at(v, &["error_code"]),
    }
}

/// `speaker_id` as Gnani sends it: a number (`1`) or a label (`"SPEAKER_1"`, `"spk_2"`).
fn parse_speaker(v: Option<&Value>) -> Option<u8> {
    match v? {
        Value::Number(n) => n.as_u64().and_then(|n| u8::try_from(n).ok()),
        Value::String(s) => {
            let digits: String = s
                .chars()
                .rev()
                .take_while(char::is_ascii_digit)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            digits.parse().ok()
        }
        _ => None,
    }
}

fn secs_to_ms(v: Option<&Value>) -> Option<i64> {
    let s = as_f64(v?)?;
    (s.is_finite() && s >= 0.0).then(|| (s * 1000.0).round() as i64)
}

/// The segments of one transcript document, in time order. Blank segments are dropped; a segment
/// whose end is before its start gets end = start.
pub fn parse_transcript(v: &Value) -> VoiceResult<Vec<BatchSegment>> {
    let list = v
        .get("segments")
        .or_else(|| v.pointer("/transcript/segments"))
        .or_else(|| v.pointer("/data/segments"))
        .and_then(Value::as_array)
        .ok_or_else(unreadable)?;
    let mut out: Vec<BatchSegment> = list
        .iter()
        .filter_map(|s| {
            let text = s.get("text")?.as_str()?.trim().to_string();
            if text.is_empty() {
                return None;
            }
            let start_ms = secs_to_ms(s.get("start_time").or_else(|| s.get("start")))?;
            let end_ms = secs_to_ms(s.get("end_time").or_else(|| s.get("end")))
                .unwrap_or(start_ms)
                .max(start_ms);
            Some(BatchSegment {
                start_ms,
                end_ms,
                speaker: parse_speaker(s.get("speaker_id").or_else(|| s.get("speaker"))),
                text,
                language: str_at(
                    s,
                    &["language_detected", "language", "detected_language_code"],
                ),
            })
        })
        .collect();
    out.sort_by_key(|s| (s.start_ms, s.end_ms));
    Ok(out)
}

// ---- assembling a meeting from chunks ------------------------------------------------------

/// One uploaded part of a meeting: its upload name and where it starts in the meeting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub index: u32,
    pub name: String,
    pub offset_ms: i64,
    pub duration_ms: Option<i64>,
}

/// What came back for one chunk.
#[derive(Debug, Clone, PartialEq)]
pub enum ChunkOutcome {
    Transcribed(Vec<BatchSegment>),
    Failed(String),
}

/// A stretch of the meeting with no transcript (a failed chunk). `end_ms` is `None` when the
/// chunk's length isn't known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gap {
    pub chunk_index: u32,
    pub start_ms: i64,
    pub end_ms: Option<i64>,
    pub reason: String,
}

/// A meeting segment: times are from the start of the meeting.
#[derive(Debug, Clone, PartialEq)]
pub struct MeetingSegment {
    pub chunk_index: u32,
    pub start_ms: i64,
    pub end_ms: i64,
    pub speaker: Option<u8>,
    pub text: String,
    pub language: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Assembled {
    pub segments: Vec<MeetingSegment>,
    pub gaps: Vec<Gap>,
}

impl Assembled {
    /// The language most of the transcript is in (by spoken time).
    pub fn dominant_language(&self) -> Option<String> {
        let mut by: Vec<(String, i64)> = Vec::new();
        for s in &self.segments {
            let Some(l) = &s.language else { continue };
            let d = (s.end_ms - s.start_ms).max(1);
            match by.iter_mut().find(|(k, _)| k == l) {
                Some((_, t)) => *t += d,
                None => by.push((l.clone(), d)),
            }
        }
        by.into_iter().max_by_key(|(_, t)| *t).map(|(l, _)| l)
    }
}

/// Puts the chunks' segments on the meeting's timeline (each chunk's times + its offset), in
/// order, and records a [`Gap`] for every chunk without a transcript. A chunk with no outcome at
/// all (Gnani didn't list it) is a gap too.
pub fn assemble(chunks: &[Chunk], outcomes: &[(String, ChunkOutcome)]) -> Assembled {
    let mut sorted: Vec<&Chunk> = chunks.iter().collect();
    sorted.sort_by_key(|c| (c.index, c.offset_ms));
    let mut out = Assembled::default();
    for chunk in sorted {
        let outcome = outcomes.iter().find(|(name, _)| *name == chunk.name);
        match outcome.map(|(_, o)| o) {
            Some(ChunkOutcome::Transcribed(segs)) => {
                out.segments.extend(segs.iter().map(|s| MeetingSegment {
                    chunk_index: chunk.index,
                    start_ms: s.start_ms + chunk.offset_ms,
                    end_ms: s.end_ms + chunk.offset_ms,
                    speaker: s.speaker,
                    text: s.text.clone(),
                    language: s.language.clone(),
                }));
            }
            other => out.gaps.push(Gap {
                chunk_index: chunk.index,
                start_ms: chunk.offset_ms,
                end_ms: chunk.duration_ms.map(|d| chunk.offset_ms + d),
                reason: match other {
                    Some(ChunkOutcome::Failed(r)) => r.clone(),
                    _ => "Gnani didn't return this part.".into(),
                },
            }),
        }
    }
    out.segments.sort_by_key(|s| (s.start_ms, s.chunk_index));
    out
}

/// `mm:ss` (or `h:mm:ss`) for messages about gaps.
pub fn clock_text(ms: i64) -> String {
    let s = ms.max(0) / 1000;
    let (h, m, s) = (s / 3600, (s / 60) % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// The message for a job that finished with gaps, e.g. "Part 2 (04:30–09:00) couldn't be
/// transcribed. Retry to try the missing parts again."
pub fn gaps_message(gaps: &[Gap]) -> Option<String> {
    if gaps.is_empty() {
        return None;
    }
    let parts: Vec<String> = gaps
        .iter()
        .map(|g| match g.end_ms {
            Some(e) => format!(
                "part {} ({}–{})",
                g.chunk_index + 1,
                clock_text(g.start_ms),
                clock_text(e)
            ),
            None => format!(
                "part {} (from {})",
                g.chunk_index + 1,
                clock_text(g.start_ms)
            ),
        })
        .collect();
    let list = parts.join(", ");
    let mut text = format!("Gnani couldn't transcribe {list}. Retry to try again.");
    if let Some(first) = text.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    Some(text)
}

// ---- polling -------------------------------------------------------------------------------

/// How often to ask for a job's status: `first`, then `step` longer each time up to `max`, for
/// at most `give_up_after`. Gnani asks for no more than one status call every 10 s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PollSchedule {
    pub first: Duration,
    pub step: Duration,
    pub max: Duration,
    pub give_up_after: Duration,
}

impl Default for PollSchedule {
    fn default() -> Self {
        Self {
            first: Duration::from_secs(10),
            step: Duration::from_millis(2_500),
            max: Duration::from_secs(15),
            give_up_after: Duration::from_secs(3 * 60 * 60),
        }
    }
}

impl PollSchedule {
    /// The wait before poll number `n` (0-based).
    pub fn delay(&self, n: u32) -> Duration {
        (self.first + self.step * n).min(self.max)
    }
}

/// Polls until the job is terminal, calling `on_progress` with every state. A run longer than
/// `give_up_after` ends with `BatchJobFailed` (the Gnani job is left alone: it may still finish,
/// and a retry starts a new one).
pub async fn wait_for<F>(
    api: &dyn BatchApi,
    job_id: &str,
    schedule: &PollSchedule,
    mut on_progress: F,
) -> VoiceResult<JobState>
where
    F: FnMut(&JobState) + Send,
{
    let started = tokio::time::Instant::now();
    let mut n = 0;
    loop {
        tokio::time::sleep(schedule.delay(n)).await;
        n += 1;
        let state = match api.status(job_id).await {
            Ok(s) => s,
            // A blip while polling isn't the job failing: keep polling until the deadline.
            Err(e) if e.retryable && started.elapsed() < schedule.give_up_after => continue,
            Err(e) => return Err(e),
        };
        on_progress(&state);
        if state.status.is_terminal() {
            return Ok(state);
        }
        if started.elapsed() >= schedule.give_up_after {
            return Err(VoiceError::new(
                VoiceErrorCode::BatchJobFailed,
                "Gnani is taking too long with this recording. Retry later.",
            )
            .retryable(true));
        }
    }
}

#[cfg(test)]
mod tests;
