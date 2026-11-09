//! Mixes the microphone with the computer's own sound into one recording.
//!
//! The two arrive as separate streams at their own pace: the microphone from the page (about 31
//! frames a second) and the system sound from the Windows loopback capture, which sends nothing
//! while the computer is quiet. Appending them one after the other would garble the recording, so
//! they are added together in time: every 32 ms one frame is made from whatever each source has
//! waiting, with silence where one has nothing. Output never stops, so the recording's clock stays
//! true to the wall clock and transcript times line up with the call.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use pluto_contracts::error::VoiceResult;
use pluto_contracts::speech::{AudioFrame, FrameSink, FRAME_BYTES};

/// Samples in one frame (16-bit mono).
const FRAME_SAMPLES: usize = FRAME_BYTES / 2;
/// One frame is 32 ms of 16 kHz audio.
pub const TICK: Duration = Duration::from_millis(32);
/// The most audio one source may have waiting (about 1.5 s). A source that runs ahead loses its
/// oldest audio instead of growing without bound.
const MAX_WAITING: usize = FRAME_SAMPLES * 47;

/// The arithmetic, with no threads: two queues and the sum of their next frame.
#[derive(Debug, Default)]
pub struct MixCore {
    mic: VecDeque<i16>,
    system: VecDeque<i16>,
}

fn queue(q: &mut VecDeque<i16>, samples: &[i16]) {
    q.extend(samples.iter().copied());
    let over = q.len().saturating_sub(MAX_WAITING);
    if over > 0 {
        q.drain(..over);
    }
}

fn pop(q: &mut VecDeque<i16>) -> i32 {
    q.pop_front().map_or(0, i32::from)
}

impl MixCore {
    pub fn push_mic(&mut self, samples: &[i16]) {
        queue(&mut self.mic, samples);
    }

    pub fn push_system(&mut self, samples: &[i16]) {
        queue(&mut self.system, samples);
    }

    /// Is anything waiting?
    pub fn has_audio(&self) -> bool {
        !self.mic.is_empty() || !self.system.is_empty()
    }

    /// The next frame: both sources added sample by sample (clipped, not wrapped), silence where a
    /// source has run out. Always [`FRAME_BYTES`] long.
    pub fn next_frame(&mut self) -> Vec<u8> {
        let mut out = Vec::with_capacity(FRAME_BYTES);
        for _ in 0..FRAME_SAMPLES {
            let sum = pop(&mut self.mic) + pop(&mut self.system);
            let v = sum.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
            out.extend_from_slice(&v.to_le_bytes());
        }
        out
    }
}

/// Mixes into `out` (the recorder). Create it, give [`Mixer::mic_sink`] to the audio router as the
/// session's sink, feed [`Mixer::push_system`] from the loopback capture, and start the clock with
/// [`Mixer::spawn_ticker`] (tests call [`Mixer::tick`] by hand instead).
pub struct Mixer {
    core: Mutex<MixCore>,
    out: Arc<dyn FrameSink>,
    session_id: String,
    seq: AtomicU64,
    stop: AtomicBool,
    ticker: Mutex<Option<JoinHandle<()>>>,
}

impl Mixer {
    pub fn new(out: Arc<dyn FrameSink>, session_id: &str) -> Arc<Self> {
        Arc::new(Self {
            core: Mutex::new(MixCore::default()),
            out,
            session_id: session_id.to_owned(),
            seq: AtomicU64::new(0),
            stop: AtomicBool::new(false),
            ticker: Mutex::new(None),
        })
    }

    fn core(&self) -> MutexGuard<'_, MixCore> {
        self.core.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// System sound (16 kHz mono PCM16) from the loopback capture.
    pub fn push_system(&self, samples: &[i16]) {
        self.core().push_system(samples);
    }

    /// The input the audio router feeds with the microphone's frames.
    pub fn mic_sink(self: &Arc<Self>) -> Arc<dyn FrameSink> {
        Arc::new(MicInput(self.clone()))
    }

    /// Makes one frame and hands it to the recorder.
    pub fn tick(&self) {
        let pcm = self.core().next_frame();
        let frame = AudioFrame {
            session_id: self.session_id.clone(),
            seq: self.seq.fetch_add(1, Ordering::Relaxed),
            pcm,
        };
        if let Err(e) = self.out.push(&frame) {
            tracing::warn!(code = ?e.code, "the recorder refused a mixed frame");
        }
    }

    /// Starts the clock: one frame every 32 ms of real time. The next frame is due a fixed time
    /// after the last was due, not after it finished, so slow ticks do not stretch the recording.
    pub fn spawn_ticker(self: &Arc<Self>) {
        let me = self.clone();
        let handle = std::thread::Builder::new()
            .name("pluto-mixer".into())
            .spawn(move || {
                let mut due = Instant::now();
                while !me.stop.load(Ordering::SeqCst) {
                    due += TICK;
                    let now = Instant::now();
                    if due > now {
                        std::thread::sleep(due - now);
                    }
                    me.tick();
                }
            });
        if let Ok(h) = handle {
            *self.ticker.lock().unwrap_or_else(PoisonError::into_inner) = Some(h);
        }
    }

    /// The recording ended: stop the clock, write out whatever is still waiting, close the recorder.
    pub fn finish(&self, session_id: &str) -> VoiceResult<()> {
        self.stop.store(true, Ordering::SeqCst);
        let ticker = self
            .ticker
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(t) = ticker {
            let _ = t.join();
        }
        // At most the waiting audio (a second and a half) in frames; stops sooner when it runs out.
        for _ in 0..48 {
            if !self.core().has_audio() {
                break;
            }
            self.tick();
        }
        self.out.end(session_id)
    }
}

struct MicInput(Arc<Mixer>);

impl FrameSink for MicInput {
    fn name(&self) -> &'static str {
        "mixer"
    }

    fn push(&self, frame: &AudioFrame) -> VoiceResult<()> {
        let samples: Vec<i16> = frame
            .pcm
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        self.0.core().push_mic(&samples);
        Ok(())
    }

    fn end(&self, session_id: &str) -> VoiceResult<()> {
        self.0.finish(session_id)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use pluto_contracts::speech::AudioFrame;

    use super::*;

    /// Collects what the recorder would get.
    #[derive(Default)]
    struct Collect {
        frames: Mutex<Vec<AudioFrame>>,
        ended: Mutex<Vec<String>>,
    }
    impl FrameSink for Collect {
        fn name(&self) -> &'static str {
            "collect"
        }
        fn push(&self, f: &AudioFrame) -> VoiceResult<()> {
            self.frames.lock().unwrap().push(f.clone());
            Ok(())
        }
        fn end(&self, s: &str) -> VoiceResult<()> {
            self.ended.lock().unwrap().push(s.into());
            Ok(())
        }
    }

    fn mic_frame(value: i16) -> AudioFrame {
        AudioFrame {
            session_id: "s".into(),
            seq: 0,
            pcm: std::iter::repeat_n(value.to_le_bytes(), FRAME_SAMPLES)
                .flatten()
                .collect(),
        }
    }

    fn samples(frame: &AudioFrame) -> Vec<i16> {
        frame
            .pcm
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect()
    }

    /// A recorder that ignores frames while paused, like `RecordingSink`.
    #[derive(Default)]
    struct Pausable {
        paused: std::sync::atomic::AtomicBool,
        kept: Mutex<Vec<AudioFrame>>,
    }
    impl FrameSink for Pausable {
        fn name(&self) -> &'static str {
            "pausable"
        }
        fn push(&self, f: &AudioFrame) -> VoiceResult<()> {
            if !self.paused.load(Ordering::SeqCst) {
                self.kept.lock().unwrap().push(f.clone());
            }
            Ok(())
        }
        fn end(&self, _: &str) -> VoiceResult<()> {
            Ok(())
        }
    }

    #[test]
    fn sound_that_plays_during_a_pause_is_dropped_and_does_not_come_back_on_resume() {
        let rec = Arc::new(Pausable::default());
        let mixer = Mixer::new(rec.clone(), "s");
        mixer.push_system(&[9; FRAME_SAMPLES]);
        mixer.tick();
        assert_eq!(rec.kept.lock().unwrap().len(), 1);

        rec.paused.store(true, Ordering::SeqCst);
        // Ten frames of the video keep arriving while paused; the clock keeps draining them.
        for _ in 0..10 {
            mixer.push_system(&[9; FRAME_SAMPLES]);
            mixer.tick();
        }
        assert_eq!(
            rec.kept.lock().unwrap().len(),
            1,
            "nothing is written while paused"
        );

        rec.paused.store(false, Ordering::SeqCst);
        mixer.push_system(&[5; FRAME_SAMPLES]);
        mixer.tick();
        let kept = rec.kept.lock().unwrap();
        assert_eq!(kept.len(), 2);
        assert!(
            samples(&kept[1]).iter().all(|&v| v == 5),
            "only new sound, no backlog"
        );
    }

    #[test]
    fn both_sources_are_added_and_a_missing_one_is_silence() {
        let mut core = MixCore::default();
        core.push_mic(&[100; FRAME_SAMPLES]);
        core.push_system(&[20; FRAME_SAMPLES]);
        let f = core.next_frame();
        assert_eq!(f.len(), FRAME_BYTES);
        assert!(f
            .chunks_exact(2)
            .all(|b| i16::from_le_bytes([b[0], b[1]]) == 120));
        // Only the system sound this time, and then nothing at all: still a full frame.
        core.push_system(&[7; FRAME_SAMPLES]);
        assert!(core
            .next_frame()
            .chunks_exact(2)
            .all(|b| i16::from_le_bytes([b[0], b[1]]) == 7));
        let quiet = core.next_frame();
        assert_eq!(quiet.len(), FRAME_BYTES);
        assert!(quiet.iter().all(|&b| b == 0));
    }

    #[test]
    fn loud_sources_clip_instead_of_wrapping() {
        let mut core = MixCore::default();
        core.push_mic(&[30_000; FRAME_SAMPLES]);
        core.push_system(&[30_000; FRAME_SAMPLES]);
        core.push_mic(&[-30_000; FRAME_SAMPLES]);
        core.push_system(&[-30_000; FRAME_SAMPLES]);
        let hi = core.next_frame();
        assert_eq!(i16::from_le_bytes([hi[0], hi[1]]), i16::MAX);
        let lo = core.next_frame();
        assert_eq!(i16::from_le_bytes([lo[0], lo[1]]), i16::MIN);
    }

    #[test]
    fn a_source_that_runs_ahead_loses_its_oldest_audio_not_memory() {
        let mut core = MixCore::default();
        // Ten seconds of system audio with nothing taking it.
        for _ in 0..10 {
            core.push_system(&vec![1; 16_000]);
        }
        assert!(core.system.len() <= MAX_WAITING);
        // A distinct tail survives: the newest audio is what is kept.
        core.push_system(&[9; 10]);
        assert_eq!(core.system.back(), Some(&9));
    }

    #[test]
    fn the_mixer_feeds_the_recorder_and_ends_it_after_flushing() {
        let out = Arc::new(Collect::default());
        let mixer = Mixer::new(out.clone(), "s");
        let mic = mixer.mic_sink();
        // Two microphone frames arrive, and some system sound while the computer plays something.
        mic.push(&mic_frame(500)).unwrap();
        mic.push(&mic_frame(500)).unwrap();
        mixer.push_system(&[100; FRAME_SAMPLES]);
        mixer.tick();
        mixer.tick();
        {
            let frames = out.frames.lock().unwrap();
            assert_eq!(frames.len(), 2);
            assert_eq!(frames[0].seq, 0);
            assert_eq!(frames[1].seq, 1);
            assert!(
                samples(&frames[0]).iter().all(|&s| s == 600),
                "mic + system"
            );
            assert!(samples(&frames[1]).iter().all(|&s| s == 500), "mic alone");
        }
        // Audio still waiting when the recording ends is written out before the end.
        mic.push(&mic_frame(300)).unwrap();
        mic.end("s").unwrap();
        let frames = out.frames.lock().unwrap();
        assert_eq!(frames.len(), 3);
        assert!(samples(&frames[2]).iter().all(|&s| s == 300));
        assert_eq!(*out.ended.lock().unwrap(), vec!["s".to_string()]);
    }

    #[test]
    fn the_clock_runs_in_real_time_and_stops_when_the_recording_ends() {
        let out = Arc::new(Collect::default());
        let mixer = Mixer::new(out.clone(), "s");
        let mic = mixer.mic_sink();
        mixer.spawn_ticker();
        std::thread::sleep(Duration::from_millis(400));
        mic.end("s").unwrap();
        let n = out.frames.lock().unwrap().len();
        // 400 ms is about 12 frames; allow for a slow machine either way.
        assert!((6..=20).contains(&n), "{n} frames");
        // Nothing more arrives after the end.
        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(out.frames.lock().unwrap().len(), n);
    }
}
