//! Meeting recorder (T-207): microphone frames in, WAV chunks of at most 4.5 minutes out.
//!
//! - [`Segmenter`]: writes PCM16 16 kHz mono into numbered WAV files, starting a new one when the
//!   current reaches the limit, and keeping each file's header correct at least every 30 s so a
//!   crash loses at most that much (and [`repair_wav`] fixes the rest on the next start).
//! - [`RecordingSink`]: a `FrameSink` around it (what `audio_push` feeds). Frames are dropped while
//!   paused; resuming starts a new chunk (A3.4).
//! - [`Recorder`]: the service. Needs consent (also enforced by the database schema), makes the
//!   meeting, saves a database row per chunk in order, and on stop queues the transcription job.
//!
//! Nothing here sends audio anywhere. Gnani Batch (T-201) picks up the queued job.

use std::fs::{File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use pluto_contracts::error::{VoiceError, VoiceErrorCode, VoiceResult};
use pluto_contracts::meeting::RecordingSession;
use pluto_contracts::speech::{AudioFrame, FrameSink};
use pluto_core::clock::Clock;
use pluto_core::db::Database;
use pluto_core::error::AppError;
use pluto_core::ids::new_id;
use pluto_core::meeting_ai_store as store;
use pluto_core::meetings::{self, MeetingInput};
use tokio::sync::{mpsc, oneshot};

/// PCM16 mono 16 kHz: 32 bytes per millisecond.
pub const BYTES_PER_MS: u64 = 32;
/// A chunk never holds more than this (Gnani Batch takes files up to about 10 MB).
pub const CHUNK_MAX_MS: u64 = 270_000;
/// The file header is brought up to date at least this often.
pub const FLUSH_EVERY_MS: u64 = 30_000;
const HEADER_BYTES: u64 = 44;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChunkEvent {
    Opened {
        index: u32,
        path: PathBuf,
        offset_ms: i64,
    },
    Closed {
        index: u32,
        duration_ms: i64,
    },
}

struct Current {
    index: u32,
    file: File,
    bytes: u64,
    since_flush: u64,
}

/// Writes numbered WAV chunks into a folder.
pub struct Segmenter {
    dir: PathBuf,
    stem: String,
    max_bytes: u64,
    flush_bytes: u64,
    next_index: u32,
    offset_ms: i64,
    cur: Option<Current>,
}

fn header(data_bytes: u32) -> [u8; 44] {
    let mut h = [0u8; 44];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&(36 + data_bytes).to_le_bytes());
    h[8..16].copy_from_slice(b"WAVEfmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes()); // PCM
    h[22..24].copy_from_slice(&1u16.to_le_bytes()); // mono
    h[24..28].copy_from_slice(&16_000u32.to_le_bytes());
    h[28..32].copy_from_slice(&32_000u32.to_le_bytes()); // byte rate
    h[32..34].copy_from_slice(&2u16.to_le_bytes()); // block align
    h[34..36].copy_from_slice(&16u16.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data_bytes.to_le_bytes());
    h
}

fn update_sizes(file: &mut File, data_bytes: u64) -> io::Result<()> {
    let n = u32::try_from(data_bytes).unwrap_or(u32::MAX - 36);
    file.seek(SeekFrom::Start(4))?;
    file.write_all(&(36 + n).to_le_bytes())?;
    file.seek(SeekFrom::Start(40))?;
    file.write_all(&n.to_le_bytes())?;
    file.seek(SeekFrom::End(0))?;
    file.flush()?;
    file.sync_data()
}

impl Segmenter {
    pub fn new(dir: impl Into<PathBuf>, stem: impl Into<String>) -> Self {
        Self::with_limits(
            dir,
            stem,
            CHUNK_MAX_MS * BYTES_PER_MS,
            FLUSH_EVERY_MS * BYTES_PER_MS,
        )
    }

    pub fn with_limits(
        dir: impl Into<PathBuf>,
        stem: impl Into<String>,
        max_bytes: u64,
        flush_bytes: u64,
    ) -> Self {
        Self {
            dir: dir.into(),
            stem: stem.into(),
            max_bytes,
            flush_bytes,
            next_index: 0,
            offset_ms: 0,
            cur: None,
        }
    }

    fn path_for(&self, index: u32) -> PathBuf {
        self.dir.join(format!("{}-{index:03}.wav", self.stem))
    }

    fn open_next(&mut self) -> io::Result<ChunkEvent> {
        std::fs::create_dir_all(&self.dir)?;
        let index = self.next_index;
        let path = self.path_for(index);
        let mut file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&path)?;
        file.write_all(&header(0))?;
        self.next_index += 1;
        self.cur = Some(Current {
            index,
            file,
            bytes: 0,
            since_flush: 0,
        });
        Ok(ChunkEvent::Opened {
            index,
            path,
            offset_ms: self.offset_ms,
        })
    }

    /// Finish the open chunk (header sizes written). `None` if nothing is open.
    pub fn close_current(&mut self) -> io::Result<Option<ChunkEvent>> {
        let Some(mut c) = self.cur.take() else {
            return Ok(None);
        };
        update_sizes(&mut c.file, c.bytes)?;
        let duration_ms = i64::try_from(c.bytes / BYTES_PER_MS).unwrap_or(i64::MAX);
        self.offset_ms += duration_ms;
        Ok(Some(ChunkEvent::Closed {
            index: c.index,
            duration_ms,
        }))
    }

    /// Append PCM. Returns the chunk events this caused (a chunk opened and/or closed).
    pub fn write(&mut self, pcm: &[u8]) -> io::Result<Vec<ChunkEvent>> {
        let mut events = Vec::new();
        if self.cur.as_ref().is_some_and(|c| c.bytes >= self.max_bytes) {
            events.extend(self.close_current()?);
        }
        if self.cur.is_none() {
            events.push(self.open_next()?);
        }
        let flush_bytes = self.flush_bytes;
        let Some(c) = self.cur.as_mut() else {
            return Ok(events);
        };
        c.file.write_all(pcm)?;
        c.bytes += pcm.len() as u64;
        c.since_flush += pcm.len() as u64;
        if c.since_flush >= flush_bytes {
            update_sizes(&mut c.file, c.bytes)?;
            c.since_flush = 0;
        }
        Ok(events)
    }
}

/// Fix a WAV left open by a crash: set the sizes from the file length. Returns the audio length
/// in ms. A file shorter than a header is an error.
pub fn repair_wav(path: &Path) -> io::Result<i64> {
    let len = std::fs::metadata(path)?.len();
    if len < HEADER_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "not a recording",
        ));
    }
    let data = (len - HEADER_BYTES) / 2 * 2; // whole samples only
    let mut f = OpenOptions::new().read(true).write(true).open(path)?;
    f.set_len(HEADER_BYTES + data)?;
    update_sizes(&mut f, data)?;
    Ok(i64::try_from(data / BYTES_PER_MS).unwrap_or(i64::MAX))
}

fn disk_error() -> VoiceError {
    VoiceError::new(
        VoiceErrorCode::Core,
        "Pluto couldn't save the recording to disk. Check there is free space.",
    )
}

/// What `audio_push` feeds during a recording.
pub struct RecordingSink {
    seg: Mutex<Segmenter>,
    paused: AtomicBool,
    frames: AtomicU64,
    on_event: Box<dyn Fn(ChunkEvent) + Send + Sync>,
}

impl RecordingSink {
    pub fn new(seg: Segmenter, on_event: impl Fn(ChunkEvent) + Send + Sync + 'static) -> Self {
        Self {
            seg: Mutex::new(seg),
            paused: AtomicBool::new(false),
            frames: AtomicU64::new(0),
            on_event: Box::new(on_event),
        }
    }

    fn emit(&self, events: Vec<ChunkEvent>) {
        events.into_iter().for_each(|e| (self.on_event)(e));
    }

    /// Frames received while recording (not while paused).
    pub fn frames(&self) -> u64 {
        self.frames.load(Ordering::Relaxed)
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    /// Stop writing and finish the chunk; frames are ignored until [`RecordingSink::resume`].
    pub fn pause(&self) -> VoiceResult<()> {
        self.paused.store(true, Ordering::Relaxed);
        let ev = self
            .seg
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .close_current()
            .map_err(|_| disk_error())?;
        self.emit(ev.into_iter().collect());
        Ok(())
    }

    /// The next frame starts a new chunk.
    pub fn resume(&self) {
        self.paused.store(false, Ordering::Relaxed);
    }
}

impl FrameSink for RecordingSink {
    fn name(&self) -> &'static str {
        "recorder"
    }

    fn push(&self, frame: &AudioFrame) -> VoiceResult<()> {
        if self.is_paused() {
            return Ok(());
        }
        let events = self
            .seg
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .write(&frame.pcm)
            .map_err(|_| disk_error())?;
        self.frames.fetch_add(1, Ordering::Relaxed);
        self.emit(events);
        Ok(())
    }

    fn end(&self, _session_id: &str) -> VoiceResult<()> {
        let ev = self
            .seg
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .close_current()
            .map_err(|_| disk_error())?;
        self.emit(ev.into_iter().collect());
        Ok(())
    }
}

// ---- the service ---------------------------------------------------------------------------

enum Msg {
    Chunk(ChunkEvent),
    /// Everything before this is saved; answer when it is.
    Done(oneshot::Sender<()>),
}

/// One recording in progress.
pub struct ActiveRecording {
    pub session: RecordingSession,
    pub sink: Arc<RecordingSink>,
    tx: mpsc::UnboundedSender<Msg>,
}

pub struct Recorder {
    db: Arc<Database>,
    clock: Arc<dyn Clock>,
    dir: PathBuf,
}

fn core(e: AppError) -> VoiceError {
    VoiceError::new(VoiceErrorCode::Core, e.message)
}

impl Recorder {
    pub fn new(db: Arc<Database>, clock: Arc<dyn Clock>, dir: impl Into<PathBuf>) -> Self {
        Self {
            db,
            clock,
            dir: dir.into(),
        }
    }

    /// Start a recording. Without consent nothing is created (the schema refuses it too).
    pub async fn start(
        &self,
        title: &str,
        participants: Vec<String>,
        live_captions: bool,
        consent: bool,
    ) -> VoiceResult<ActiveRecording> {
        if !consent {
            return Err(VoiceError::new(
                VoiceErrorCode::ConsentRequired,
                "Confirm that everyone in the meeting knows it is being recorded.",
            ));
        }
        let now = self.clock.now_ms();
        let meeting = meetings::create(
            &self.db,
            &*self.clock,
            MeetingInput {
                title: if title.trim().is_empty() {
                    "Recorded meeting".into()
                } else {
                    title.trim().to_owned()
                },
                participants,
                ..Default::default()
            },
        )
        .await
        .map_err(core)?;
        let (meeting_id, session_id) = (meeting.id, new_id());
        let mid = meeting_id.clone();
        self.db
            .write(move |tx| {
                store::set_status(tx, &mid, "recording", None, now)?;
                Ok(((), vec![]))
            })
            .await
            .map_err(core)?;

        // Chunk rows are saved by one task, in order, so a crash leaves an accurate trail.
        let (tx, mut rx) = mpsc::unbounded_channel::<Msg>();
        let (db, mid, dir_clock) = (self.db.clone(), meeting_id.clone(), self.clock.clone());
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                match msg {
                    Msg::Chunk(ChunkEvent::Opened {
                        index,
                        path,
                        offset_ms,
                    }) => {
                        let row = store::RecordingRow {
                            id: new_id(),
                            meeting_id: mid.clone(),
                            source: "mic".into(),
                            chunk_index: index,
                            offset_ms,
                            file_path: path.to_string_lossy().into_owned(),
                            duration_ms: None,
                            consent_confirmed_at: Some(now),
                            created_at: dir_clock.now_ms(),
                        };
                        if db
                            .write(move |tx| {
                                store::insert_recording(tx, &row)?;
                                Ok(((), vec![]))
                            })
                            .await
                            .is_err()
                        {
                            tracing::warn!("could not save a recording chunk row");
                        }
                    }
                    Msg::Chunk(ChunkEvent::Closed { index, duration_ms }) => {
                        let m = mid.clone();
                        if db
                            .write(move |tx| {
                                store::finish_recording(tx, &m, index, duration_ms)?;
                                Ok(((), vec![]))
                            })
                            .await
                            .is_err()
                        {
                            tracing::warn!("could not save a recording length");
                        }
                    }
                    Msg::Done(ack) => {
                        let _ = ack.send(());
                    }
                }
            }
        });

        let sender = tx.clone();
        let sink = Arc::new(RecordingSink::new(
            Segmenter::new(self.dir.clone(), meeting_id.clone()),
            move |e| {
                let _ = sender.send(Msg::Chunk(e));
            },
        ));
        Ok(ActiveRecording {
            session: RecordingSession {
                id: session_id,
                meeting_id,
                started_at: now,
                live_captions,
                chunk_count: 0,
            },
            sink,
            tx,
        })
    }

    async fn settle(rec: &ActiveRecording) {
        let (ack, wait) = oneshot::channel();
        if rec.tx.send(Msg::Done(ack)).is_ok() {
            let _ = wait.await;
        }
    }

    /// Finish the recording: close the last chunk and save everything. Returns the session with its
    /// final chunk count. The caller then queues the transcription job (`MeetingJobs::queue`),
    /// which also moves the meeting to "transcribing"; this crate never calls Gnani.
    pub async fn stop(&self, rec: &ActiveRecording) -> VoiceResult<RecordingSession> {
        rec.sink.end(&rec.session.id)?;
        Self::settle(rec).await;
        let mid = rec.session.meeting_id.clone();
        let chunks = self
            .db
            .read(move |c| Ok(store::list_recordings(c, &mid)?.len()))
            .await
            .map_err(core)?;
        Ok(RecordingSession {
            chunk_count: u32::try_from(chunks).unwrap_or(u32::MAX),
            ..rec.session.clone()
        })
    }

    pub async fn pause(&self, rec: &ActiveRecording) -> VoiceResult<RecordingSession> {
        rec.sink.pause()?;
        Self::settle(rec).await;
        Ok(rec.session.clone())
    }

    pub fn resume(&self, rec: &ActiveRecording) -> RecordingSession {
        rec.sink.resume();
        rec.session.clone()
    }

    /// Recordings the app stopped in the middle of (crash, kill, power cut). Repairs each file and
    /// records its length; returns the meetings affected so the person can be asked what to do.
    pub async fn recover(&self) -> VoiceResult<Vec<String>> {
        let open = self
            .db
            .read(store::unfinished_recordings)
            .await
            .map_err(core)?;
        let mut meetings: Vec<String> = Vec::new();
        for r in open {
            let Ok(ms) = repair_wav(Path::new(&r.file_path)) else {
                tracing::warn!("a recording file is missing or damaged; skipped");
                continue;
            };
            let (mid, idx) = (r.meeting_id.clone(), r.chunk_index);
            self.db
                .write(move |tx| {
                    store::finish_recording(tx, &mid, idx, ms)?;
                    Ok(((), vec![]))
                })
                .await
                .map_err(core)?;
            if !meetings.contains(&r.meeting_id) {
                meetings.push(r.meeting_id);
            }
        }
        Ok(meetings)
    }
}
