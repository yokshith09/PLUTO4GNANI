//! Always-on listening with an on-device wake word, "Pluto" (T-705).
//!
//! Owner: **D1**.
//!
//! - **Teaching:** the user says "Pluto" a few times ([`take_clip`] keeps clear single words).
//!   [`WakeStore`] saves them and builds a Rustpotter reference from them, all on this computer.
//! - **Listening:** while always-on is enabled, the UI keeps the microphone open and sends its
//!   frames for one session whose only sink is [`WakeSink`]. The sink just queues them; the
//!   [`run`] loop does the work off the audio router's lock:
//!   - wherever push-to-talk works (idle, a proposal waiting for its answer, after an error, over
//!     a reply), the detector looks for "Pluto". **Nothing leaves the device for this;**
//!   - on "Pluto" it starts a turn exactly as push-to-talk does, sends the last half-second (the
//!     words right after "Pluto") and then everything that follows, and ends the turn after a
//!     pause (or at the time limit), so Gnani hears only the command;
//!   - while any turn is listening (hotkey, button or wake) the frames go to that turn, so one
//!     microphone serves everything;
//!   - while Pluto is thinking, running a change, recording a meeting, or Cloud voice is off,
//!     frames are dropped and the detector rests.
//!
//! Measured on D1's laptop (`docs/results/wake-word.md`): about 2% of one core while listening.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pluto_contracts::error::{VoiceError, VoiceErrorCode, VoiceResult};
use pluto_contracts::speech::{AudioFrame, FrameSink, FRAME_BYTES};
use pluto_contracts::voice::VoiceState;
use rustpotter::{
    Rustpotter, RustpotterConfig, SampleFormat, ScoreMode, WakewordLoad, WakewordRef,
    WakewordRefBuildFromBuffers, WakewordSave,
};
use tokio::sync::mpsc;

use crate::audio::AudioRouter;
use crate::runtime::VoiceRuntime;

const RATE: usize = 16_000;
/// Good recordings needed to build the wake word; the UI asks for [`ASK_FOR`].
pub const NEEDED: u32 = 6;
pub const ASK_FOR: u32 = 8;
/// The detection threshold. On D1's voice, 0.5 caught 4/4 commands with no false trigger in
/// normal talk or on 72 sound-alike words; 0.55 already missed one.
pub const THRESHOLD: f32 = 0.5;
const REFERENCE_FILE: &str = "pluto.rpw";

fn bad(message: &str) -> VoiceError {
    VoiceError::new(VoiceErrorCode::Core, message).retryable(false)
}

// ---- teaching --------------------------------------------------------------------------------

fn rms(s: &[i16]) -> f32 {
    if s.is_empty() {
        return 0.;
    }
    (s.iter().map(|x| f32::from(*x).powi(2)).sum::<f32>() / s.len() as f32).sqrt()
}

/// Little-endian PCM16 bytes → samples.
pub fn samples(pcm: &[u8]) -> Vec<i16> {
    pcm.chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect()
}

/// Checks one teaching recording and cuts it to the spoken word (plus 100 ms each side).
/// `Err` says what to do instead.
pub fn take_clip(pcm: &[i16]) -> Result<Vec<i16>, &'static str> {
    let w = RATE / 100; // 10 ms
    let levels: Vec<f32> = pcm.chunks(w).map(rms).collect();
    let peak = pcm.iter().map(|x| x.unsigned_abs()).max().unwrap_or(0);
    if levels.len() < 50 || peak < 1500 {
        return Err("Too quiet. Say \u{201c}Pluto\u{201d} again, a little louder.");
    }
    let top = levels.iter().copied().fold(0., f32::max);
    let loud: Vec<usize> = (0..levels.len())
        .filter(|i| levels[*i] > top / 4.)
        .collect();
    let (first, last) = (loud[0], loud[loud.len() - 1]);
    if first < 3 || last + 3 >= levels.len() {
        return Err("The word was cut off. Wait for the prompt, then say \u{201c}Pluto\u{201d}.");
    }
    let span = last - first + 1;
    if span < 20 {
        return Err("Too short. Say the whole word, \u{201c}Pluto\u{201d}.");
    }
    if span > 130 {
        return Err("That sounded like more than one word. Say just \u{201c}Pluto\u{201d}.");
    }
    let a = first.saturating_sub(10) * w;
    let b = ((last + 11) * w).min(pcm.len());
    Ok(pcm[a..b].to_vec())
}

fn wav_bytes(samples: &[i16]) -> VoiceResult<Vec<u8>> {
    let mut buf = std::io::Cursor::new(Vec::new());
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: RATE as u32,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let err = |_| bad("Couldn't prepare the recording.");
    let mut w = hound::WavWriter::new(&mut buf, spec).map_err(err)?;
    for s in samples {
        w.write_sample(*s).map_err(err)?;
    }
    w.finalize().map_err(err)?;
    Ok(buf.into_inner())
}

/// The user's "Pluto": teaching recordings and the reference built from them, in `dir`.
pub struct WakeStore {
    dir: PathBuf,
}

impl WakeStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn clip_paths(&self) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(&self.dir)
            .map(|d| {
                d.filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter(|p| {
                        p.file_name()
                            .and_then(|n| n.to_str())
                            .is_some_and(|n| n.starts_with("clip_") && n.ends_with(".wav"))
                    })
                    .collect()
            })
            .unwrap_or_default();
        paths.sort();
        paths
    }

    /// Good recordings kept so far.
    pub fn clips(&self) -> u32 {
        u32::try_from(self.clip_paths().len()).unwrap_or(u32::MAX)
    }

    /// A reference has been built.
    pub fn enrolled(&self) -> bool {
        self.dir.join(REFERENCE_FILE).is_file()
    }

    /// Keeps one recording (already checked by [`take_clip`]). Returns the count.
    pub fn add_clip(&self, word: &[i16]) -> VoiceResult<u32> {
        std::fs::create_dir_all(&self.dir).map_err(|_| bad("Couldn't save the recording."))?;
        let n = self.clips() + 1;
        std::fs::write(self.dir.join(format!("clip_{n:02}.wav")), wav_bytes(word)?)
            .map_err(|_| bad("Couldn't save the recording."))?;
        Ok(n)
    }

    /// Builds the reference from the recordings (at least [`NEEDED`]).
    pub fn build(&self) -> VoiceResult<()> {
        let paths = self.clip_paths();
        if (paths.len() as u32) < NEEDED {
            return Err(bad("Say \u{201c}Pluto\u{201d} a few more times first."));
        }
        let mut samples = HashMap::new();
        for p in &paths {
            let bytes = std::fs::read(p).map_err(|_| bad("Couldn't read a recording."))?;
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("clip");
            samples.insert(name.to_string(), bytes);
        }
        let reference =
            WakewordRef::new_from_sample_buffers("pluto".into(), None, None, samples, 16)
                .map_err(|_| bad("Couldn't learn the wake word from those recordings."))?;
        let path = self.dir.join(REFERENCE_FILE);
        reference
            .save_to_file(&path.to_string_lossy())
            .map_err(|_| bad("Couldn't save the wake word."))
    }

    /// The saved reference, if any.
    pub fn reference(&self) -> Option<WakewordRef> {
        WakewordRef::load_from_file(&self.dir.join(REFERENCE_FILE).to_string_lossy()).ok()
    }

    /// Forgets the recordings and the reference.
    pub fn reset(&self) -> VoiceResult<()> {
        if self.dir.exists() {
            std::fs::remove_dir_all(&self.dir)
                .map_err(|_| bad("Couldn't remove the wake word."))?;
        }
        Ok(())
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

/// Collects one teaching recording.
#[derive(Default)]
pub struct EnrollSink(Mutex<Vec<i16>>);

impl EnrollSink {
    pub fn take(&self) -> Vec<i16> {
        std::mem::take(
            &mut *self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }
}

impl FrameSink for EnrollSink {
    fn name(&self) -> &'static str {
        "wake-enroll"
    }
    fn push(&self, frame: &AudioFrame) -> VoiceResult<()> {
        let mut all = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // At most 5 s: a teaching recording is one word.
        if all.len() < 5 * RATE {
            all.extend(samples(&frame.pcm));
        }
        Ok(())
    }
}

// ---- listening -------------------------------------------------------------------------------

/// Hands the always-on microphone's frames to [`run`]. Never blocks: when the queue is full the
/// frame is dropped (the detector would be behind anyway).
pub struct WakeSink(pub mpsc::Sender<Vec<u8>>);

impl FrameSink for WakeSink {
    fn name(&self) -> &'static str {
        "wake"
    }
    fn push(&self, frame: &AudioFrame) -> VoiceResult<()> {
        let _ = self.0.try_send(frame.pcm.clone());
        Ok(())
    }
}

/// Spots the wake word in 16 kHz PCM16 audio.
pub trait Detector: Send {
    /// More audio (any length). True when "Pluto" was just heard.
    fn feed(&mut self, pcm: &[i16]) -> bool;
    /// Forget partial matches (after a detection, or when listening pauses).
    fn reset(&mut self);
}

/// The Rustpotter detector for the user's reference.
pub struct PlutoDetector {
    rp: Rustpotter,
    frame: usize,
    pending: Vec<i16>,
}

impl PlutoDetector {
    pub fn new(reference: WakewordRef) -> VoiceResult<Self> {
        let mut config = RustpotterConfig::default();
        config.fmt.sample_rate = RATE;
        config.fmt.sample_format = SampleFormat::I16;
        config.fmt.channels = 1;
        config.detector.threshold = THRESHOLD;
        config.detector.score_mode = ScoreMode::Max;
        let mut rp =
            Rustpotter::new(&config).map_err(|_| bad("Couldn't start the wake-word detector."))?;
        rp.add_wakeword_ref("pluto", reference)
            .map_err(|_| bad("Couldn't load the wake word."))?;
        let frame = rp.get_samples_per_frame();
        Ok(Self {
            rp,
            frame,
            pending: Vec::new(),
        })
    }
}

impl Detector for PlutoDetector {
    fn feed(&mut self, pcm: &[i16]) -> bool {
        self.pending.extend_from_slice(pcm);
        let mut heard = false;
        while self.pending.len() >= self.frame {
            let chunk: Vec<i16> = self.pending.drain(..self.frame).collect();
            heard |= self.rp.process_samples(chunk).is_some();
        }
        heard
    }
    fn reset(&mut self) {
        self.pending.clear();
        self.rp.reset();
    }
}

/// What [`run`] needs from the voice runtime and the audio router.
pub trait Turns: Send + Sync {
    fn state(&self) -> VoiceState;
    /// The session of the turn now listening, if any.
    fn listening(&self) -> Option<String>;
    /// Starts a turn, as a push-to-talk press does. Its session, or `None` if refused.
    fn start(&self) -> Option<String>;
    /// Ends the turn's listening, as releasing the key does.
    fn stop(&self);
    /// One frame for `session`.
    fn push(&self, session: &str, pcm: Vec<u8>);
}

/// The app's runtime and router.
pub struct RuntimeTurns {
    pub runtime: Arc<VoiceRuntime>,
    pub router: Arc<AudioRouter>,
}

impl Turns for RuntimeTurns {
    fn state(&self) -> VoiceState {
        self.runtime.state()
    }
    fn listening(&self) -> Option<String> {
        let s = self.runtime.status();
        (s.state == VoiceState::Listening)
            .then_some(s.session_id)
            .flatten()
    }
    fn start(&self) -> Option<String> {
        self.runtime.ptt_down().ok()
    }
    fn stop(&self) {
        self.runtime.ptt_up();
    }
    fn push(&self, session: &str, pcm: Vec<u8>) {
        let _ = self.router.push(session, pcm);
    }
}

/// Timing for a wake-started turn.
#[derive(Debug, Clone)]
pub struct WakePolicy {
    /// Audio from just before the detection that goes to the turn: the words right after
    /// "Pluto" (detection lands about half a second after the word ends).
    pub preroll: Duration,
    /// A pause this long after speech ends the command.
    pub end_silence: Duration,
    /// No speech at all for this long after "Pluto": the turn ends.
    pub no_speech: Duration,
    /// The longest command.
    pub max_turn: Duration,
}

impl Default for WakePolicy {
    fn default() -> Self {
        Self {
            preroll: Duration::from_millis(500),
            end_silence: Duration::from_millis(800),
            no_speech: Duration::from_secs(4),
            max_turn: Duration::from_secs(15),
        }
    }
}

/// The end of the wake word that can follow its detection.
const WAKE_TAIL: Duration = Duration::from_millis(320);

fn frames(d: Duration) -> usize {
    (d.as_millis() as usize).div_ceil(32)
}

/// Speech or not, per frame, against a slowly tracked background level.
struct Vad {
    floor: f32,
}

impl Vad {
    fn new() -> Self {
        Self { floor: 150. }
    }
    fn speech(&mut self, level: f32) -> bool {
        let threshold = (self.floor * 2.5).max(350.);
        if level < threshold {
            // Follow the room's background level, slowly.
            self.floor = self.floor * 0.95 + level * 0.05;
            false
        } else {
            true
        }
    }
}

/// A wake-started turn in progress.
struct Following {
    session: String,
    frames: usize,
    speech_seen: bool,
    quiet: usize,
}

/// The always-on loop: until the queue closes (always-on turned off).
pub async fn run(
    mut rx: mpsc::Receiver<Vec<u8>>,
    mut detector: Box<dyn Detector>,
    turns: Arc<dyn Turns>,
    policy: WakePolicy,
) {
    let preroll = frames(policy.preroll).max(1);
    let mut recent: VecDeque<Vec<u8>> = VecDeque::with_capacity(preroll + 1);
    let mut following: Option<Following> = None;
    let mut vad = Vad::new();
    let mut resting = false;
    while let Some(pcm) = rx.recv().await {
        if pcm.len() != FRAME_BYTES {
            continue;
        }
        let pcm16 = samples(&pcm);
        let level = rms(&pcm16);
        let is_speech = vad.speech(level);
        recent.push_back(pcm.clone());
        while recent.len() > preroll {
            recent.pop_front();
        }
        match turns.state() {
            VoiceState::Listening => {
                let Some(session) = turns.listening() else {
                    continue;
                };
                turns.push(&session, pcm);
                let Some(f) = following.as_mut().filter(|f| f.session == session) else {
                    continue; // a hotkey or button turn: the key or button ends it
                };
                f.frames += 1;
                // The first frames after detection can still be the end of "Pluto" itself:
                // they don't count as the command starting.
                let after_wake = f.frames > frames(WAKE_TAIL);
                if is_speech && after_wake {
                    f.speech_seen = true;
                    f.quiet = 0;
                } else if f.speech_seen {
                    f.quiet += 1;
                }
                let done = (f.speech_seen && f.quiet >= frames(policy.end_silence))
                    || (!f.speech_seen && f.frames >= frames(policy.no_speech))
                    || f.frames >= frames(policy.max_turn);
                if done {
                    let why = if !f.speech_seen {
                        "nothing said"
                    } else if f.quiet >= frames(policy.end_silence) {
                        "pause"
                    } else {
                        "time limit"
                    };
                    dev_log(&format!(
                        "command ended after {:.1} s ({why})",
                        f.frames as f32 * 0.032
                    ));
                    following = None;
                    turns.stop();
                }
            }
            // Wherever push-to-talk works, "Pluto" works: idle, a proposal waiting for its
            // answer ("Pluto, haan"), after an error, and over a reply (which it stops).
            VoiceState::Idle
            | VoiceState::AwaitingApproval
            | VoiceState::Error
            | VoiceState::Speaking => {
                following = None;
                if resting {
                    detector.reset();
                    resting = false;
                }
                if detector.feed(&pcm16) {
                    detector.reset();
                    let state = turns.state();
                    match turns.start() {
                        Some(session) => {
                            dev_log(&format!(
                                "heard Pluto ({state:?}); listening for the command"
                            ));
                            for f in recent.drain(..) {
                                turns.push(&session, f);
                            }
                            following = Some(Following {
                                session,
                                frames: 0,
                                speech_seen: false,
                                quiet: 0,
                            });
                        }
                        None => dev_log(&format!("heard Pluto but no turn started ({state:?})")),
                    }
                }
            }
            // Thinking, running a change, recording a meeting, Cloud voice off: nothing to do.
            _ => {
                following = None;
                resting = true;
            }
        }
    }
}

/// Development builds only: what always-on did, on the console, so a miss can be explained.
/// Never what was said.
fn dev_log(what: &str) {
    if cfg!(debug_assertions) {
        eprintln!("[voice] wake: {what}");
    }
}

/// The command without the wake word: "Pluto, add a task" → "add a task".
pub fn strip_wake_word(text: &str) -> String {
    let t = text.trim_start();
    let lower = t.to_lowercase();
    for lead in [
        "hey pluto",
        "ok pluto",
        "okay pluto",
        "hi pluto",
        "pluto",
        "प्लूटो",
        "ப்ளூட்டோ",
    ] {
        if let Some(rest) = lower.strip_prefix(lead) {
            // Only a whole word: "plutonium" stays.
            if rest.chars().next().is_some_and(char::is_alphanumeric) {
                continue;
            }
            let cut = t.len() - rest.len();
            return t[cut..]
                .trim_start_matches(|c: char| c.is_whitespace() || ",.!?:;-\u{2013}".contains(c))
                .to_string();
        }
    }
    text.to_string()
}

#[cfg(test)]
mod tests;
