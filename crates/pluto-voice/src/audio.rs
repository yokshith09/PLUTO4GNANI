//! The Rust half of the hybrid audio path (ARCHITECTURE §4, step 5). Owner: **D1** (T-013).
//!
//! `audio_push` hands every frame from the UI to an [`AudioRouter`]. A capture session (a
//! push-to-talk press or a meeting recording) is opened with the sinks that should receive its
//! audio — the Realtime sender, the [`WavSink`], the recorder — and every frame reaches all of
//! them in arrival order, numbered from 0. One capture stream can therefore feed both a WAV file
//! and Realtime (ARCHITECTURE §5).
//!
//! Frames for a session that is not open (a few arrive just after the key is released) are
//! dropped silently. A frame that is not exactly [`FRAME_BYTES`] long is rejected.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use pluto_contracts::error::{VoiceError, VoiceErrorCode, VoiceResult};
use pluto_contracts::speech::{AudioFrame, FrameSink, CAPTURE_SAMPLE_RATE, FRAME_BYTES};

/// Length of one frame: 512 samples at 16 kHz = exactly 32 ms (31.25 frames per second).
pub const FRAME_MS: u64 = (FRAME_BYTES as u64 / 2) * 1000 / CAPTURE_SAMPLE_RATE as u64;

// ---- router --------------------------------------------------------------------------------

/// What happened to one pushed frame.
#[derive(Debug, Default)]
pub struct PushOutcome {
    /// The frame's 0-based number in its session, or `None` when it was dropped because the
    /// session is not open.
    pub seq: Option<u64>,
    /// Sinks that failed for the first time on this frame. Each sink is reported once per session;
    /// later failures of the same sink are only logged. The other sinks still got the frame.
    pub new_failures: Vec<(&'static str, VoiceError)>,
}

/// Totals for a session that just ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionSummary {
    pub frames: u64,
    /// Audio length, from the frame count.
    pub duration_ms: u64,
}

struct Session {
    sinks: Vec<Arc<dyn FrameSink>>,
    failed: Vec<bool>,
    next_seq: u64,
}

/// Routes frames from `audio_push` to each open session's sinks. Cheap to share (`Arc`).
#[derive(Default)]
pub struct AudioRouter {
    sessions: Mutex<HashMap<String, Session>>,
}

impl AudioRouter {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, Session>> {
        // A panic in a sink must not take capture down for good.
        self.sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Opens a session. Fails with `Busy` if one with the same id is already open.
    pub fn open(&self, session_id: &str, sinks: Vec<Arc<dyn FrameSink>>) -> VoiceResult<()> {
        let mut sessions = self.lock();
        if sessions.contains_key(session_id) {
            return Err(VoiceError::new(
                VoiceErrorCode::Busy,
                "That audio session is already running.",
            ));
        }
        let failed = vec![false; sinks.len()];
        sessions.insert(
            session_id.to_string(),
            Session {
                sinks,
                failed,
                next_seq: 0,
            },
        );
        Ok(())
    }

    /// Whether a session is open.
    pub fn is_open(&self, session_id: &str) -> bool {
        self.lock().contains_key(session_id)
    }

    /// Ids of the open sessions.
    pub fn open_sessions(&self) -> Vec<String> {
        self.lock().keys().cloned().collect()
    }

    /// One frame from `audio_push`. Sinks run while the router is locked, so frames reach every
    /// sink in exactly the order they arrived; `FrameSink::push` must not block.
    pub fn push(&self, session_id: &str, pcm: Vec<u8>) -> VoiceResult<PushOutcome> {
        if pcm.len() != FRAME_BYTES {
            return Err(VoiceError::new(
                VoiceErrorCode::UnsupportedAudio,
                format!(
                    "Audio frames must be {FRAME_BYTES} bytes; got {}.",
                    pcm.len()
                ),
            ));
        }
        let mut sessions = self.lock();
        let Some(session) = sessions.get_mut(session_id) else {
            return Ok(PushOutcome::default());
        };
        let frame = AudioFrame {
            session_id: session_id.to_string(),
            seq: session.next_seq,
            pcm,
        };
        session.next_seq += 1;
        let mut new_failures = Vec::new();
        for (sink, failed) in session.sinks.iter().zip(session.failed.iter_mut()) {
            if let Err(e) = sink.push(&frame) {
                tracing::warn!(sink = sink.name(), code = ?e.code, "audio sink failed");
                if !*failed {
                    *failed = true;
                    new_failures.push((sink.name(), e));
                }
            }
        }
        Ok(PushOutcome {
            seq: Some(frame.seq),
            new_failures,
        })
    }

    /// Closes a session: every sink gets `end`. Returns `None` if it was not open. Sink errors
    /// from `end` are returned (the session is closed either way).
    pub fn close(
        &self,
        session_id: &str,
    ) -> Option<(SessionSummary, Vec<(&'static str, VoiceError)>)> {
        let session = self.lock().remove(session_id)?;
        let mut errors = Vec::new();
        for sink in &session.sinks {
            if let Err(e) = sink.end(session_id) {
                tracing::warn!(sink = sink.name(), code = ?e.code, "audio sink failed to close");
                errors.push((sink.name(), e));
            }
        }
        let frames = session.next_seq;
        Some((
            SessionSummary {
                frames,
                duration_ms: frames * FRAME_MS,
            },
            errors,
        ))
    }
}

// ---- WAV writer ----------------------------------------------------------------------------

/// How often the WAV header is rewritten, so a crash loses at most this much audio
/// (ARCHITECTURE §11: chunks are flushed every ≤ 30 s).
pub const WAV_FLUSH_EVERY_FRAMES: u64 = 30_000 / FRAME_MS;

// Checked at compile time: a frame is exactly 32 ms, and a crash loses at most 30 s.
const _: () = assert!(FRAME_MS == 32);
const _: () = assert!(WAV_FLUSH_EVERY_FRAMES == 937 && WAV_FLUSH_EVERY_FRAMES * FRAME_MS <= 30_000);

struct WavState {
    writer: Option<hound::WavWriter<BufWriter<File>>>,
    frames: u64,
}

/// A `FrameSink` that writes 16 kHz mono PCM16 to a WAV file. The header is rewritten every
/// [`WAV_FLUSH_EVERY_FRAMES`] frames, so the file stays playable if the app crashes; `end`
/// finalises it. Used for the T-IPC-01 check and by the recorder (D2) for meeting chunks.
pub struct WavSink {
    path: PathBuf,
    flush_every: u64,
    state: Mutex<WavState>,
}

fn file_error(what: &str) -> VoiceError {
    // `Core` is the closest code: a local failure, not a network or Gnani one. No path in the
    // message: it can contain the user's name.
    VoiceError::new(
        VoiceErrorCode::Core,
        format!("Couldn't {what} the recording file. Check there is free disk space."),
    )
}

impl WavSink {
    /// Creates (or replaces) the file at `path`.
    pub fn create(path: impl Into<PathBuf>) -> VoiceResult<Self> {
        Self::with_flush_interval(path, WAV_FLUSH_EVERY_FRAMES)
    }

    /// As [`WavSink::create`], rewriting the header every `flush_every` frames (≥ 1).
    pub fn with_flush_interval(path: impl Into<PathBuf>, flush_every: u64) -> VoiceResult<Self> {
        let path = path.into();
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: CAPTURE_SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let writer = hound::WavWriter::create(&path, spec).map_err(|_| file_error("create"))?;
        Ok(Self {
            path,
            flush_every: flush_every.max(1),
            state: Mutex::new(WavState {
                writer: Some(writer),
                frames: 0,
            }),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Frames written so far.
    pub fn frames(&self) -> u64 {
        self.lock().frames
    }

    fn lock(&self) -> MutexGuard<'_, WavState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl FrameSink for WavSink {
    fn name(&self) -> &'static str {
        "wav"
    }

    fn push(&self, frame: &AudioFrame) -> VoiceResult<()> {
        let mut state = self.lock();
        let Some(writer) = state.writer.as_mut() else {
            return Err(file_error("write"));
        };
        for pair in frame.pcm.chunks_exact(2) {
            let sample = i16::from_le_bytes([pair[0], pair[1]]);
            writer
                .write_sample(sample)
                .map_err(|_| file_error("write"))?;
        }
        state.frames += 1;
        if state.frames.is_multiple_of(self.flush_every) {
            if let Some(writer) = state.writer.as_mut() {
                writer.flush().map_err(|_| file_error("write"))?;
            }
        }
        Ok(())
    }

    fn end(&self, _session_id: &str) -> VoiceResult<()> {
        match self.lock().writer.take() {
            Some(writer) => writer.finalize().map_err(|_| file_error("save")),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pluto_contracts::mock::RecordingSink;

    fn frame_of(value: i16) -> Vec<u8> {
        value
            .to_le_bytes()
            .iter()
            .copied()
            .cycle()
            .take(FRAME_BYTES)
            .collect()
    }

    #[test]
    fn frames_reach_every_sink_in_order_numbered_from_zero() {
        let router = AudioRouter::new();
        let a = Arc::new(RecordingSink::new());
        let b = Arc::new(RecordingSink::new());
        router.open("s1", vec![a.clone(), b.clone()]).unwrap();
        for i in 0..5 {
            let out = router.push("s1", frame_of(i)).unwrap();
            assert_eq!(out.seq, Some(i as u64));
            assert!(out.new_failures.is_empty());
        }
        for sink in [&a, &b] {
            let frames = sink.frames();
            assert_eq!(frames.len(), 5);
            for (i, f) in frames.iter().enumerate() {
                assert_eq!(f.seq, i as u64);
                assert_eq!(f.session_id, "s1");
                assert_eq!(f.pcm, frame_of(i as i16));
            }
        }
    }

    #[test]
    fn sessions_are_independent() {
        let router = AudioRouter::new();
        let ptt = Arc::new(RecordingSink::new());
        let rec = Arc::new(RecordingSink::new());
        router.open("ptt", vec![ptt.clone()]).unwrap();
        router.open("rec", vec![rec.clone()]).unwrap();
        router.push("rec", frame_of(1)).unwrap();
        router.push("ptt", frame_of(2)).unwrap();
        router.push("rec", frame_of(3)).unwrap();
        assert_eq!(ptt.frames().len(), 1);
        assert_eq!(
            rec.frames().iter().map(|f| f.seq).collect::<Vec<_>>(),
            [0, 1]
        );
        let mut open = router.open_sessions();
        open.sort();
        assert_eq!(open, ["ptt", "rec"]);
    }

    #[test]
    fn a_frame_of_the_wrong_size_is_rejected() {
        let router = AudioRouter::new();
        router.open("s", vec![]).unwrap();
        let err = router.push("s", vec![0; 1000]).unwrap_err();
        assert_eq!(err.code, VoiceErrorCode::UnsupportedAudio);
        assert!(router.push("s", vec![]).is_err());
    }

    #[test]
    fn frames_for_a_closed_or_unknown_session_are_dropped_silently() {
        let router = AudioRouter::new();
        let sink = Arc::new(RecordingSink::new());
        router.open("s", vec![sink.clone()]).unwrap();
        router.push("s", frame_of(0)).unwrap();
        router.close("s").unwrap();
        let late = router.push("s", frame_of(1)).unwrap();
        assert_eq!(late.seq, None);
        assert_eq!(router.push("never", frame_of(1)).unwrap().seq, None);
        assert_eq!(sink.frames().len(), 1);
    }

    #[test]
    fn a_failing_sink_is_reported_once_and_the_others_keep_getting_frames() {
        let router = AudioRouter::new();
        let good = Arc::new(RecordingSink::new());
        router
            .open("s", vec![Arc::new(RecordingSink::failing()), good.clone()])
            .unwrap();
        let first = router.push("s", frame_of(0)).unwrap();
        assert_eq!(first.new_failures.len(), 1);
        assert_eq!(first.new_failures[0].0, "recording");
        let second = router.push("s", frame_of(1)).unwrap();
        assert!(second.new_failures.is_empty(), "reported only once");
        assert_eq!(good.frames().len(), 2);
    }

    #[test]
    fn close_ends_every_sink_and_summarises_the_session() {
        let router = AudioRouter::new();
        let sink = Arc::new(RecordingSink::new());
        router.open("s", vec![sink.clone()]).unwrap();
        for i in 0..125 {
            router.push("s", frame_of(i)).unwrap();
        }
        let (summary, errors) = router.close("s").unwrap();
        assert_eq!(summary.frames, 125);
        assert_eq!(summary.duration_ms, 4000, "125 frames × 32 ms");
        assert!(errors.is_empty());
        assert_eq!(sink.ended(), ["s"]);
        assert!(!router.is_open("s"));
        assert!(router.close("s").is_none());
    }

    #[test]
    fn opening_a_session_twice_is_busy() {
        let router = AudioRouter::new();
        router.open("s", vec![]).unwrap();
        assert_eq!(
            router.open("s", vec![]).unwrap_err().code,
            VoiceErrorCode::Busy
        );
    }

    #[test]
    fn the_wav_sink_writes_16khz_mono_pcm16_that_reads_back_exactly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t-ipc-01.wav");
        let wav = Arc::new(WavSink::create(&path).unwrap());
        let router = AudioRouter::new();
        router.open("rec", vec![wav.clone()]).unwrap();
        for i in 0..10 {
            router.push("rec", frame_of(i * 100 - 300)).unwrap();
        }
        let (summary, errors) = router.close("rec").unwrap();
        assert!(errors.is_empty());
        assert_eq!(summary.frames, 10);
        assert_eq!(wav.frames(), 10);

        let mut reader = hound::WavReader::open(&path).unwrap();
        let spec = reader.spec();
        assert_eq!(
            (spec.channels, spec.sample_rate, spec.bits_per_sample),
            (1, 16_000, 16)
        );
        let samples: Vec<i16> = reader.samples::<i16>().map(Result::unwrap).collect();
        assert_eq!(samples.len(), 10 * FRAME_BYTES / 2);
        for (i, chunk) in samples.chunks(FRAME_BYTES / 2).enumerate() {
            let expected = i as i16 * 100 - 300;
            assert!(chunk.iter().all(|&s| s == expected), "frame {i}");
        }
    }

    #[test]
    fn the_wav_header_is_rewritten_periodically_so_a_crash_keeps_the_audio() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crash.wav");
        let wav = WavSink::with_flush_interval(&path, 2).unwrap();
        let frame = |seq| AudioFrame {
            session_id: "s".into(),
            seq,
            pcm: frame_of(7),
        };
        for seq in 0..4 {
            wav.push(&frame(seq)).unwrap();
        }
        // Never finalised: simulate a crash by reading the file while the writer is still open.
        let reader = hound::WavReader::open(&path).unwrap();
        assert_eq!(reader.len() as usize, 4 * FRAME_BYTES / 2);
        drop(reader);
        wav.end("s").unwrap();
        wav.end("s").unwrap(); // twice is fine
        assert!(wav.push(&frame(4)).is_err(), "no writes after end");
    }

    #[test]
    fn a_wav_sink_in_a_missing_folder_fails_with_a_message_without_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing").join("x.wav");
        let err = WavSink::create(&path).err().unwrap();
        assert_eq!(err.code, VoiceErrorCode::Core);
        assert!(!err.message.contains("missing"));
    }
}
