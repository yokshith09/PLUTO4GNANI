//! Meeting transcription jobs (T-201): import an audio file, send the meeting's audio to Prisma
//! Batch, poll, and store speaker-labelled segments with their offsets in the meeting.
//!
//! Owner: **D1**. The Tauri commands `meeting_import`, `meeting_job_get` and `meeting_job_retry`
//! are thin wrappers over [`MeetingJobs`]; every state change is also sent to [`JobEvents`]
//! (the shell forwards it as `meeting://job`).
//!
//! A job moves `queued → uploading → created → in_progress →` one of
//! - `completed`: every part transcribed; the meeting goes to `needs_speakers`;
//! - `partial_failure`: some parts failed; finished parts are kept, the job's `error` names the
//!   missing stretches (the gap markers) and `meeting_job_retry` tries again;
//! - `failed`: nothing usable; the meeting goes to `failed`.
//!
//! Imported files are copied into the app's data folder first, so the job never depends on the
//! original staying where it was. Audio leaves the machine only to Gnani, and every call is in
//! the privacy log (through `pluto_gnani::http`).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use pluto_contracts::error::{VoiceError, VoiceErrorCode, VoiceResult};
use pluto_contracts::meeting::{JobStatus, TranscriptionJob};
use pluto_core::clock::Clock;
use pluto_core::db::Database;
use pluto_core::ids::new_id;
use pluto_core::meeting_ai_store::{self as store, JobRow, RecordingRow, SegmentRow};
use pluto_core::meetings::{self, MeetingInput};
use pluto_gnani::batch::{
    self, assemble, gaps_message, wait_for, Assembled, BatchApi, BatchConfig, BatchFile,
    BatchStatus, Chunk, ChunkOutcome, PollSchedule, MAX_FILE_BYTES,
};

/// Where job updates go (the shell emits `meeting://job`).
pub trait JobEvents: Send + Sync {
    fn job(&self, job: &TranscriptionJob);
}

/// For tests and for running without a UI.
pub struct NoEvents;
impl JobEvents for NoEvents {
    fn job(&self, _: &TranscriptionJob) {}
}

pub struct MeetingJobs {
    db: Arc<Database>,
    clock: Arc<dyn Clock>,
    api: Arc<dyn BatchApi>,
    /// `<app data>/meetings`: one folder per meeting with its audio.
    audio_dir: PathBuf,
    events: Arc<dyn JobEvents>,
    schedule: PollSchedule,
    /// Meetings with a job running in this process.
    running: Mutex<HashSet<String>>,
}

impl MeetingJobs {
    pub fn new(
        db: Arc<Database>,
        clock: Arc<dyn Clock>,
        api: Arc<dyn BatchApi>,
        audio_dir: PathBuf,
        events: Arc<dyn JobEvents>,
    ) -> Self {
        Self {
            db,
            clock,
            api,
            audio_dir,
            events,
            schedule: PollSchedule::default(),
            running: Mutex::default(),
        }
    }

    pub fn with_schedule(mut self, schedule: PollSchedule) -> Self {
        self.schedule = schedule;
        self
    }

    /// Creates the meeting, copies the file into the data folder and starts transcribing it.
    /// Returns the queued job at once; progress arrives through [`JobEvents`].
    pub async fn import(
        self: &Arc<Self>,
        title: &str,
        participants: &[String],
        path: &Path,
    ) -> VoiceResult<TranscriptionJob> {
        let ext = check_import(path).await?;
        let meeting = meetings::create(
            &self.db,
            &*self.clock,
            MeetingInput {
                title: title.to_string(),
                participants: participants.to_vec(),
                ..Default::default()
            },
        )
        .await?;

        let folder = self.audio_dir.join(&meeting.id);
        let target = folder.join(format!("import-0000.{ext}"));
        let copied = async {
            tokio::fs::create_dir_all(&folder).await?;
            tokio::fs::copy(path, &target).await
        }
        .await;
        if copied.is_err() {
            // Don't leave an empty meeting behind for a file that couldn't be read.
            let _ = meetings::delete(&self.db, &*self.clock, &meeting.id).await;
            let _ = tokio::fs::remove_dir_all(&folder).await;
            return Err(VoiceError::new(
                VoiceErrorCode::Core,
                "Pluto couldn't copy that file into its data folder.",
            ));
        }

        let now = self.clock.now_ms();
        let recording = RecordingRow {
            id: new_id(),
            meeting_id: meeting.id.clone(),
            source: "import".into(),
            chunk_index: 0,
            offset_ms: 0,
            file_path: target.to_string_lossy().into_owned(),
            duration_ms: wav_duration_ms(&target),
            consent_confirmed_at: None,
            created_at: now,
        };
        let meeting_id = meeting.id.clone();
        self.db
            .write(move |tx| {
                store::insert_recording(tx, &recording)?;
                Ok(((), vec![]))
            })
            .await?;
        self.queue(&meeting_id).await
    }

    /// The meeting's newest job, if any.
    pub async fn job(&self, meeting_id: &str) -> VoiceResult<Option<TranscriptionJob>> {
        let id = meeting_id.to_string();
        let row = self.db.read(move |c| store::latest_job(c, &id)).await?;
        Ok(row.map(|r| to_job(&r)))
    }

    /// Transcribes the meeting's audio again as a new job (after `failed`, `partial_failure` or
    /// `cancelled`). Refused while a job for the meeting is still running.
    pub async fn retry(self: &Arc<Self>, meeting_id: &str) -> VoiceResult<TranscriptionJob> {
        self.queue(meeting_id).await
    }

    /// Records a new job for the meeting's recordings and runs it in the background.
    pub async fn queue(self: &Arc<Self>, meeting_id: &str) -> VoiceResult<TranscriptionJob> {
        if !self.claim(meeting_id) {
            return Err(VoiceError::new(
                VoiceErrorCode::Busy,
                "This meeting is already being transcribed.",
            ));
        }
        match self.insert_job(meeting_id).await {
            Ok(job) => {
                self.events.job(&job);
                let me = Arc::clone(self);
                let (job_id, mid) = (job.id.clone(), meeting_id.to_string());
                tokio::spawn(async move {
                    me.run(&job_id, &mid).await;
                    me.release(&mid);
                });
                Ok(job)
            }
            Err(e) => {
                self.release(meeting_id);
                Err(e)
            }
        }
    }

    fn claim(&self, meeting_id: &str) -> bool {
        lock(&self.running).insert(meeting_id.to_string())
    }

    fn release(&self, meeting_id: &str) {
        lock(&self.running).remove(meeting_id);
    }

    async fn insert_job(&self, meeting_id: &str) -> VoiceResult<TranscriptionJob> {
        let meeting = meetings::get(&self.db, meeting_id).await?;
        let config = BatchConfig::for_meeting(&meeting.participants, &[], None);
        let config_json = serde_json::to_string(&config).unwrap_or_else(|_| "{}".into());
        let (id, mid, now) = (new_id(), meeting_id.to_string(), self.clock.now_ms());
        let row = self
            .db
            .write(move |tx| {
                if store::list_recordings(tx, &mid)?.is_empty() {
                    return Err(pluto_core::error::AppError::validation(
                        "meeting_id",
                        "This meeting has no audio to transcribe.",
                    ));
                }
                store::insert_job(tx, &id, &mid, &config_json, now)?;
                store::set_status(tx, &mid, "transcribing", None, now)?;
                Ok((store::latest_job(tx, &mid)?, vec![]))
            })
            .await?;
        row.map(|r| to_job(&r))
            .ok_or_else(|| VoiceError::new(VoiceErrorCode::Core, "The job wasn't saved."))
    }

    /// Runs one job to the end. Never returns an error: every outcome is written to the job.
    async fn run(&self, job_id: &str, meeting_id: &str) {
        let outcome = self.transcribe(job_id, meeting_id).await;
        let now = self.clock.now_ms();
        let (jid, mid) = (job_id.to_string(), meeting_id.to_string());
        let written = match outcome {
            Ok((assembled, status)) => {
                let message = gaps_message(&assembled.gaps);
                let language = assembled.dominant_language();
                let has_speakers = assembled.segments.iter().any(|s| s.speaker.is_some());
                let rows = segment_rows(&mid, &assembled);
                self.db
                    .write(move |tx| {
                        store::replace_segments(tx, &mid, &rows)?;
                        let next = if has_speakers {
                            "needs_speakers"
                        } else {
                            "needs_review"
                        };
                        store::set_status(tx, &mid, next, language.as_deref(), now)?;
                        store::update_job(tx, &jid, status, None, message.as_deref(), now)?;
                        Ok((store::latest_job(tx, &mid)?, vec![]))
                    })
                    .await
            }
            Err(e) => {
                tracing::warn!(code = ?e.code, "transcription job failed");
                let message = e.message;
                self.db
                    .write(move |tx| {
                        store::update_job(tx, &jid, "failed", None, Some(&message), now)?;
                        store::set_status(tx, &mid, "failed", None, now)?;
                        Ok((store::latest_job(tx, &mid)?, vec![]))
                    })
                    .await
            }
        };
        match written {
            Ok(Some(row)) => self.events.job(&to_job(&row)),
            Ok(None) => {}
            Err(e) => tracing::warn!(error = %e.message, "couldn't save the transcription result"),
        }
    }

    async fn set_job(&self, job_id: &str, status: &str, gnani_id: Option<&str>) {
        let (jid, status, gid) = (
            job_id.to_string(),
            status.to_string(),
            gnani_id.map(str::to_string),
        );
        let now = self.clock.now_ms();
        let row = self
            .db
            .write(move |tx| {
                store::update_job(tx, &jid, &status, gid.as_deref(), None, now)?;
                let mid: String = tx.query_row(
                    "SELECT meeting_id FROM transcription_jobs WHERE id = ?1",
                    [&jid],
                    |r| r.get(0),
                )?;
                Ok((store::latest_job(tx, &mid)?, vec![]))
            })
            .await;
        if let Ok(Some(row)) = row {
            self.events.job(&to_job(&row));
        }
    }

    /// Upload, start, poll and fetch. Returns the assembled transcript and the final job status
    /// (`completed` or `partial_failure`).
    async fn transcribe(
        &self,
        job_id: &str,
        meeting_id: &str,
    ) -> VoiceResult<(Assembled, &'static str)> {
        self.set_job(job_id, "uploading", None).await;
        let mid = meeting_id.to_string();
        let recordings = self
            .db
            .read(move |c| store::list_recordings(c, &mid))
            .await?;
        let meeting = meetings::get(&self.db, meeting_id).await?;

        let mut files = Vec::with_capacity(recordings.len());
        let mut chunks = Vec::with_capacity(recordings.len());
        for r in &recordings {
            let ext = batch::extension(&r.file_path).unwrap_or_else(|| "wav".into());
            let name = format!("chunk-{:04}.{ext}", r.chunk_index);
            let bytes = tokio::fs::read(&r.file_path).await.map_err(|_| {
                VoiceError::new(
                    VoiceErrorCode::Core,
                    "This meeting's audio file is missing from Pluto's data folder.",
                )
            })?;
            files.push(BatchFile {
                name: name.clone(),
                bytes,
            });
            chunks.push(Chunk {
                index: r.chunk_index,
                name,
                offset_ms: r.offset_ms,
                duration_ms: r.duration_ms,
            });
        }

        let config = BatchConfig::for_meeting(&meeting.participants, &[], None);
        let gnani_id = self.api.create(&files, &config, meeting_id).await?;
        drop(files);
        self.set_job(job_id, "created", Some(&gnani_id)).await;

        let started = self.api.start(&gnani_id).await?;
        if matches!(started, BatchStatus::StartFailed | BatchStatus::Failed) {
            return Err(job_failed());
        }
        self.set_job(job_id, "in_progress", None).await;

        let state = wait_for(&*self.api, &gnani_id, &self.schedule, |_| {}).await?;
        if !state.status.has_results() {
            return Err(match state.status {
                BatchStatus::Cancelled => VoiceError::new(
                    VoiceErrorCode::BatchJobFailed,
                    "The transcription was cancelled. Retry to start it again.",
                ),
                _ => job_failed(),
            });
        }

        let results = self.api.files(&gnani_id).await?;
        let mut outcomes = Vec::with_capacity(chunks.len());
        for chunk in &chunks {
            let Some(file) = results
                .iter()
                .find(|f| f.original_path.ends_with(&chunk.name))
            else {
                continue; // `assemble` turns a missing part into a gap
            };
            let outcome = match (&file.transcript_url, file.is_completed()) {
                (Some(url), true) => match self.api.transcript(url, meeting_id).await {
                    Ok(segments) => ChunkOutcome::Transcribed(segments),
                    Err(e) => ChunkOutcome::Failed(e.message),
                },
                _ => ChunkOutcome::Failed(
                    file.error_code
                        .clone()
                        .unwrap_or_else(|| file.status.clone()),
                ),
            };
            outcomes.push((chunk.name.clone(), outcome));
        }

        let assembled = assemble(&chunks, &outcomes);
        if assembled.segments.is_empty() {
            return Err(if assembled.gaps.is_empty() {
                VoiceError::new(
                    VoiceErrorCode::BatchJobFailed,
                    "Gnani found no speech in this recording.",
                )
            } else {
                job_failed()
            });
        }
        let status = if assembled.gaps.is_empty() {
            "completed"
        } else {
            "partial_failure"
        };
        Ok((assembled, status))
    }
}

fn job_failed() -> VoiceError {
    VoiceError::new(
        VoiceErrorCode::BatchJobFailed,
        "Gnani couldn't transcribe this recording. Retry, or try another file.",
    )
    .retryable(true)
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Checks an import before anything is created. Returns the lower-case extension.
async fn check_import(path: &Path) -> VoiceResult<String> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    if !batch::is_supported(name) {
        return Err(batch::unsupported());
    }
    let meta = tokio::fs::metadata(path)
        .await
        .ok()
        .filter(std::fs::Metadata::is_file)
        .ok_or_else(|| VoiceError::new(VoiceErrorCode::Core, "Pluto can't open that file."))?;
    if meta.len() == 0 {
        return Err(VoiceError::new(
            VoiceErrorCode::UnsupportedAudio,
            "That file is empty.",
        ));
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(batch::file_too_large());
    }
    Ok(batch::extension(name).unwrap_or_default())
}

/// The length of a WAV file from its header (other formats: unknown).
fn wav_duration_ms(path: &Path) -> Option<i64> {
    let reader = hound::WavReader::open(path).ok()?;
    let spec = reader.spec();
    let rate = i64::from(spec.sample_rate);
    (rate > 0).then(|| i64::from(reader.duration()) * 1000 / rate)
}

fn segment_rows(meeting_id: &str, a: &Assembled) -> Vec<SegmentRow> {
    a.segments
        .iter()
        .enumerate()
        .map(|(i, s)| SegmentRow {
            id: new_id(),
            meeting_id: meeting_id.to_string(),
            index: i as u32,
            start_ms: s.start_ms,
            end_ms: s.end_ms,
            speaker_label: s.speaker,
            participant_position: None,
            text: s.text.clone(),
            language: s.language.clone(),
            relevance: None,
        })
        .collect()
}

/// The stored row as the contract type. Statuses are stored in their wire spelling.
pub fn to_job(r: &JobRow) -> TranscriptionJob {
    TranscriptionJob {
        id: r.id.clone(),
        meeting_id: r.meeting_id.clone(),
        gnani_job_id: r.gnani_job_id.clone(),
        status: serde_json::from_value(serde_json::Value::String(r.status.clone()))
            .unwrap_or(JobStatus::Failed),
        error: r.error.clone(),
        updated_at: r.updated_at,
    }
}

#[cfg(test)]
mod tests;
