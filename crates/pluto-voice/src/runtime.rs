//! Voice state machine (ARCHITECTURE §5): push-to-talk, `voice://status`, barge-in, `Busy` while
//! recording.
//!
//! Owner: **D1** (T-301). Two layers:
//!
//! - [`next`] is the pure transition table: a state and an [`Input`] give the next state and the
//!   [`Effect`]s to run, or a [`Refusal`]. Every transition is tested on its own.
//! - [`VoiceRuntime`] drives it: it opens the Prisma Realtime session behind a press, routes the
//!   captured frames to it (buffered while it connects), commits on release, waits up to 3 s for
//!   the final transcript, hands the words to the [`TurnHandler`] (the assistant, T-303), and stops
//!   a reply when the key is pressed again (barge-in).
//!
//! Push-to-talk works the same from the global hotkey and from the on-screen button: a press
//! shows Listening at once (the Realtime connection opens in the background, so the 100 ms
//! target never waits on the network) and reports the new session in `VoiceStatus.session_id`,
//! which is the session the UI's capture sends frames for. A press under 300 ms is ignored; a
//! press held over 60 s is committed automatically.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use async_trait::async_trait;
use pluto_contracts::error::{VoiceError, VoiceErrorCode, VoiceResult};
use pluto_contracts::speech::{
    AudioFrame, FrameSink, RtConfig, RtEvent, RtInput, RtTranscript, SpeechToText,
};
use pluto_contracts::voice::{VoiceMode, VoiceState, VoiceStatus};
use tokio::sync::mpsc;
use tokio::task::AbortHandle;
use tokio::time::Instant;

use crate::audio::AudioRouter;
use crate::endpoint::{EndpointPolicy, Endpointer};

// ---- policy --------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePolicy {
    /// Shorter presses are taken as accidental and ignored.
    pub min_press: Duration,
    /// Longer presses are committed automatically.
    pub max_press: Duration,
    /// After release, how long to wait for the final transcript.
    pub final_wait: Duration,
    /// Frames held while Realtime connects (10 s of audio).
    pub buffer_frames: usize,
}

impl Default for RuntimePolicy {
    fn default() -> Self {
        Self {
            min_press: Duration::from_millis(300),
            max_press: Duration::from_secs(60),
            final_wait: Duration::from_secs(3),
            buffer_frames: 320,
        }
    }
}

// ---- the transition table ------------------------------------------------------------------

/// What happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// Cloud voice turned on and the Gnani key is present.
    Enable,
    /// Cloud voice turned off or the key removed.
    Disable,
    PttDown,
    /// The key was released after being held for `held`.
    PttUp {
        held: Duration,
    },
    /// The key has been held for `max_press`.
    AutoCommit,
    /// The final words arrived (or the wait ended with some words).
    Heard,
    /// The wait ended with nothing said.
    NothingHeard,
    /// The assistant finished without anything to say.
    TurnDone,
    /// The assistant has a reply; the UI speaks it (`voice_speak`), which sends `SpeakStart`.
    TurnReply,
    /// The assistant made a proposal and waits for approval (30 s).
    TurnProposal,
    ProposalResolved {
        approved: bool,
    },
    ExecutionDone,
    SpeakStart,
    SpeakEnd,
    RecordStart,
    RecordStop,
    Fail,
    /// The person dismissed the error.
    Dismiss,
    Cancel,
}

/// What the driver must do for a transition, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Open a push-to-talk session (capture route + Realtime).
    Open,
    /// Release: send `commit` and wait for the final transcript.
    Commit,
    /// Close the push-to-talk session without a turn.
    Discard,
    /// Stop the reply being spoken (barge-in).
    StopSpeaking,
    /// Hand the words to the assistant.
    Think,
}

/// Why an input was not accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The input means nothing in this state (e.g. a key repeat while Listening).
    Ignored,
    /// Push-to-talk while a meeting is recording.
    Busy,
    /// Push-to-talk with cloud voice off.
    Disabled,
}

/// The transition table (ARCHITECTURE §5). Deviations, both deliberate:
/// - `Thinking → Idle` on a reply: the UI speaks it with `voice_speak`, which then moves
///   `Idle → Speaking`, so a reply is never "speaking" without audio.
/// - `AwaitingApproval → Listening`: the person approves by voice ("haan") with push-to-talk.
pub fn next(
    state: VoiceState,
    input: &Input,
    policy: &RuntimePolicy,
) -> Result<(VoiceState, Vec<Effect>), Refusal> {
    use Effect as E;
    use Input as I;
    use VoiceState as S;

    // Turning voice off ends whatever was going on.
    if *input == I::Disable {
        return match state {
            S::Disabled => Err(Refusal::Ignored),
            S::Listening | S::Thinking => Ok((S::Disabled, vec![E::Discard])),
            S::Speaking => Ok((S::Disabled, vec![E::StopSpeaking])),
            _ => Ok((S::Disabled, vec![])),
        };
    }

    Ok(match (state, input) {
        (S::Disabled, I::Enable) => (S::Idle, vec![]),
        (S::Disabled, I::PttDown) => return Err(Refusal::Disabled),
        (S::Disabled, _) => return Err(Refusal::Ignored),
        (_, I::Enable) => return Err(Refusal::Ignored),

        (S::Recording, I::PttDown) => return Err(Refusal::Busy),
        (S::Recording, I::RecordStop) => (S::Idle, vec![]),
        (S::Recording, I::Fail) => (S::Error, vec![]),
        (S::Recording, _) => return Err(Refusal::Ignored),

        (S::Idle | S::Error, I::PttDown) => (S::Listening, vec![E::Open]),
        (S::AwaitingApproval, I::PttDown) => (S::Listening, vec![E::Open]),
        (S::Speaking, I::PttDown) => (S::Listening, vec![E::StopSpeaking, E::Open]),
        (S::Idle, I::RecordStart) => (S::Recording, vec![]),
        (S::Idle, I::SpeakStart) => (S::Speaking, vec![]),

        (S::Listening, I::PttUp { held }) if *held < policy.min_press => {
            (S::Idle, vec![E::Discard])
        }
        (S::Listening, I::PttUp { .. } | I::AutoCommit) => (S::Thinking, vec![E::Commit]),
        (S::Listening, I::Cancel) => (S::Idle, vec![E::Discard]),
        (S::Listening, I::Fail) => (S::Error, vec![E::Discard]),

        (S::Thinking, I::Heard) => (S::Thinking, vec![E::Think]),
        (S::Thinking, I::NothingHeard | I::TurnDone | I::TurnReply | I::Cancel) => {
            (S::Idle, vec![])
        }
        (S::Thinking, I::TurnProposal) => (S::AwaitingApproval, vec![]),
        (S::Thinking, I::Fail) => (S::Error, vec![]),

        (S::AwaitingApproval, I::ProposalResolved { approved: true }) => (S::Executing, vec![]),
        (S::AwaitingApproval, I::ProposalResolved { approved: false } | I::Cancel) => {
            (S::Idle, vec![])
        }
        // The proposal is read out while it waits.
        (S::AwaitingApproval, I::SpeakStart | I::SpeakEnd) => (S::AwaitingApproval, vec![]),

        (S::Executing, I::ExecutionDone) => (S::Idle, vec![]),
        (S::Executing, I::Fail) => (S::Error, vec![]),

        (S::Speaking, I::SpeakStart) => (S::Speaking, vec![]),
        (S::Speaking, I::SpeakEnd) => (S::Idle, vec![]),
        (S::Speaking, I::Cancel) => (S::Idle, vec![E::StopSpeaking]),
        (S::Speaking, I::Fail) => (S::Error, vec![E::StopSpeaking]),

        (S::Idle, I::Fail) => (S::Error, vec![]),
        (S::AwaitingApproval, I::Fail) => (S::Error, vec![]),
        (S::Error, I::Dismiss | I::Cancel) => (S::Idle, vec![]),

        _ => return Err(Refusal::Ignored),
    })
}

// ---- what the runtime talks to -------------------------------------------------------------

/// Where the runtime reports (the shell emits `voice://status`, `voice://transcript`,
/// `voice://error`).
pub trait RuntimeEvents: Send + Sync {
    fn status(&self, status: &VoiceStatus);
    fn transcript(&self, session_id: &str, transcript: &RtTranscript);
    fn error(&self, error: &VoiceError);
}

/// The words of one push-to-talk turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heard {
    pub session_id: String,
    pub text: String,
    /// The language most of the words were in, if Gnani said.
    pub language: Option<String>,
}

/// What the assistant did with a turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Turn {
    /// Nothing to say.
    Done,
    /// A reply was sent to the UI (`voice://turn`), which speaks it.
    Reply,
    /// A proposal waits for approval (`voice://proposal`).
    Proposal,
}

/// The assistant (T-303). Called in Thinking with the final words.
#[async_trait]
pub trait TurnHandler: Send + Sync {
    async fn turn(&self, heard: Heard) -> VoiceResult<Turn>;
}

/// Until the assistant lands: the words are shown (`voice://transcript`) and nothing else happens.
pub struct NoAssistant;

#[async_trait]
impl TurnHandler for NoAssistant {
    async fn turn(&self, _heard: Heard) -> VoiceResult<Turn> {
        Ok(Turn::Done)
    }
}

/// Types text into whatever window has the focus (dictation). Implemented by the shell for the
/// operating system; a test records what it was asked to type.
pub trait Typer: Send + Sync {
    fn type_text(&self, text: &str) -> VoiceResult<()>;
}

/// How the next turn starts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TurnKind {
    /// Hold the key (or the Talk button); release sends.
    #[default]
    Held,
    /// One press starts, a pause ends it (hands-free).
    HandsFree,
    /// One press starts, the next press ends it; each phrase is typed where the cursor is.
    Dictation,
}

/// Sends captured frames toward Realtime. Never blocks: when the buffer is full (Realtime far
/// behind), the frame is refused and the router reports it once.
struct RealtimeSink(
    mpsc::Sender<RtInput>,
    Arc<CaptureStats>,
    Option<Arc<AutoEnd>>,
);

/// What reached Rust during one press: how many frames, and the loudest sample. Tells "no audio
/// arrived" and "only silence arrived" apart from "Gnani heard nothing it could transcribe".
#[derive(Debug, Default)]
pub struct CaptureStats {
    frames: AtomicU64,
    peak: AtomicU32,
}

impl CaptureStats {
    pub fn add(&self, pcm: &[u8]) {
        self.frames.fetch_add(1, Ordering::Relaxed);
        let peak = pcm
            .chunks_exact(2)
            .map(|b| u32::from(i16::from_le_bytes([b[0], b[1]]).unsigned_abs()))
            .max()
            .unwrap_or(0);
        self.peak.fetch_max(peak, Ordering::Relaxed);
    }

    pub fn frames(&self) -> u64 {
        self.frames.load(Ordering::Relaxed)
    }

    /// The loudest sample (0–32768).
    pub fn peak(&self) -> u32 {
        self.peak.load(Ordering::Relaxed)
    }
}

/// Below this peak (about −50 dBFS) a press is taken as silence: a muted or wrong microphone.
pub const SILENCE_PEAK: u32 = 100;

/// What to say when a press produced no words, from what actually arrived.
pub fn nothing_heard_detail(frames: u64, peak: u32) -> String {
    if frames == 0 {
        "No sound reached Pluto from the microphone. Check that Windows lets Pluto use it \
         (Settings → Privacy → Microphone)."
            .into()
    } else if peak < SILENCE_PEAK {
        "Pluto only heard silence. Check the microphone isn't muted and is the one Windows uses."
            .into()
    } else {
        "Didn't catch that. Hold the key while you speak.".into()
    }
}

impl FrameSink for RealtimeSink {
    fn name(&self) -> &'static str {
        "realtime"
    }

    fn push(&self, frame: &AudioFrame) -> VoiceResult<()> {
        self.1.add(&frame.pcm);
        if let Some(auto) = &self.2 {
            auto.feed(&frame.pcm);
        }
        self.0
            .try_send(RtInput::Frame(frame.pcm.clone()))
            .map_err(|_| {
                VoiceError::new(
                    VoiceErrorCode::NetworkOffline,
                    "Gnani isn't keeping up; some of what you said may be missed.",
                )
            })
    }
}

/// A hands-free turn: watches its own frames and presses "release" after the person stops talking.
struct AutoEnd {
    endpointer: Mutex<Endpointer>,
    runtime: std::sync::Weak<VoiceRuntime>,
    session_id: String,
    fired: std::sync::atomic::AtomicBool,
}

impl AutoEnd {
    fn feed(&self, pcm: &[u8]) {
        use std::sync::atomic::Ordering;
        if self.fired.load(Ordering::Relaxed) {
            return;
        }
        let over = self
            .endpointer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .feed(pcm);
        if over && !self.fired.swap(true, Ordering::SeqCst) {
            // Off this call: ending the turn closes the very route this frame is travelling on.
            let (runtime, session) = (self.runtime.clone(), self.session_id.clone());
            let end = move || {
                if let Some(rt) = runtime.upgrade() {
                    if rt.is_listening(&session) {
                        rt.ptt_up();
                    }
                }
            };
            match tokio::runtime::Handle::try_current() {
                Ok(handle) => {
                    handle.spawn(async move { end() });
                }
                Err(_) => {
                    std::thread::spawn(end);
                }
            }
        }
    }
}

type SinkFactory = dyn Fn(&str) -> Vec<Arc<dyn FrameSink>> + Send + Sync;
type ConfigSource = dyn Fn() -> RtConfig + Send + Sync;

// ---- the driver ----------------------------------------------------------------------------

struct Listen {
    session_id: String,
    pressed: Instant,
    /// Into the session's buffer (frames, then commit / close).
    tx: mpsc::Sender<RtInput>,
    stats: Arc<CaptureStats>,
    kind: TurnKind,
}

struct Inner {
    status: VoiceStatus,
    listen: Option<Listen>,
    /// The push-to-talk session whose words are being waited for or thought about.
    turn_session: Option<String>,
    /// The reply being spoken and its number (so a stopped reply finishing late can't end a
    /// newer one).
    speaking: Option<(u64, AbortHandle)>,
    speak_seq: u64,
    /// How the next press starts (set just before `ptt_down`, read when the route opens).
    next_kind: TurnKind,
    /// How the turn whose words are being waited for or thought about started.
    turn_kind: TurnKind,
}

pub struct VoiceRuntime {
    stt: Arc<dyn SpeechToText>,
    router: Arc<AudioRouter>,
    events: Arc<dyn RuntimeEvents>,
    handler: Arc<dyn TurnHandler>,
    policy: RuntimePolicy,
    config: Box<ConfigSource>,
    extra_sinks: Box<SinkFactory>,
    typer: std::sync::OnceLock<Arc<dyn Typer>>,
    inner: Mutex<Inner>,
}

impl VoiceRuntime {
    /// Starts `Disabled` when `enabled` is false (cloud voice off or no key), else `Idle`.
    pub fn new(
        stt: Arc<dyn SpeechToText>,
        router: Arc<AudioRouter>,
        events: Arc<dyn RuntimeEvents>,
        handler: Arc<dyn TurnHandler>,
        enabled: bool,
    ) -> Self {
        let state = if enabled {
            VoiceState::Idle
        } else {
            VoiceState::Disabled
        };
        Self {
            stt,
            router,
            events,
            handler,
            policy: RuntimePolicy::default(),
            config: Box::new(RtConfig::default),
            extra_sinks: Box::new(|_| Vec::new()),
            typer: std::sync::OnceLock::new(),
            inner: Mutex::new(Inner {
                status: VoiceStatus::new(state),
                listen: None,
                turn_session: None,
                speaking: None,
                speak_seq: 0,
                next_kind: TurnKind::Held,
                turn_kind: TurnKind::Held,
            }),
        }
    }

    pub fn with_policy(mut self, policy: RuntimePolicy) -> Self {
        self.policy = policy;
        self
    }

    /// The Realtime settings for each press (language, format, keyterms from the settings).
    pub fn with_config(mut self, config: impl Fn() -> RtConfig + Send + Sync + 'static) -> Self {
        self.config = Box::new(config);
        self
    }

    /// More sinks for every push-to-talk session (the mic check's WAV, T-011).
    pub fn with_extra_sinks(
        mut self,
        sinks: impl Fn(&str) -> Vec<Arc<dyn FrameSink>> + Send + Sync + 'static,
    ) -> Self {
        self.extra_sinks = Box::new(sinks);
        self
    }

    /// Where dictated text goes.
    pub fn with_typer(self, typer: Arc<dyn Typer>) -> Self {
        let _ = self.typer.set(typer);
        self
    }

    /// Hands-free: a press starts listening and a pause in your speech ends the command. A press
    /// while it is listening ends it at once. Returns the session to capture for, as `ptt_down`.
    pub fn hands_free_toggle(self: &Arc<Self>) -> VoiceResult<Option<String>> {
        self.toggle(TurnKind::HandsFree)
    }

    /// Dictation: a press starts it, the next press ends it. Each phrase Gnani returns is typed
    /// where the cursor is, in whatever app has the focus.
    pub fn dictation_toggle(self: &Arc<Self>) -> VoiceResult<Option<String>> {
        if self.typer.get().is_none() {
            return Err(VoiceError::new(
                VoiceErrorCode::Core,
                "Dictation isn't available on this computer.",
            ));
        }
        self.toggle(TurnKind::Dictation)
    }

    /// `Some(session)` when a turn started, `None` when a running one was ended.
    fn toggle(self: &Arc<Self>, kind: TurnKind) -> VoiceResult<Option<String>> {
        let running = {
            let inner = self.lock();
            inner
                .listen
                .as_ref()
                .filter(|_| inner.status.state == VoiceState::Listening)
                .map(|l| l.kind)
        };
        if running.is_some() {
            self.ptt_up();
            return Ok(None);
        }
        self.lock().next_kind = kind;
        let started = self.ptt_down();
        if started.is_err() {
            self.lock().next_kind = TurnKind::Held;
        }
        started.map(Some)
    }

    /// Whether the turn listening now is dictation (the UI shows "Dictating").
    pub fn dictating(&self) -> bool {
        let inner = self.lock();
        inner.status.state == VoiceState::Listening
            && inner
                .listen
                .as_ref()
                .is_some_and(|l| l.kind == TurnKind::Dictation)
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn status(&self) -> VoiceStatus {
        self.lock().status.clone()
    }

    pub fn state(&self) -> VoiceState {
        self.lock().status.state
    }

    /// Cloud voice on with a key → `Idle`; off → `Disabled` (ending any turn).
    pub fn set_enabled(self: &Arc<Self>, enabled: bool) {
        let _ = self.apply(
            if enabled {
                Input::Enable
            } else {
                Input::Disable
            },
            None,
        );
    }

    /// Key down (or the talk button). Returns the session the captured frames belong to.
    /// While Listening (a key repeat) it returns the open session again.
    pub fn ptt_down(self: &Arc<Self>) -> VoiceResult<String> {
        match self.apply(Input::PttDown, None) {
            Ok(()) => {}
            Err(Refusal::Ignored) => {}
            Err(Refusal::Busy) => {
                let e = VoiceError::new(VoiceErrorCode::Busy, "Busy: recording a meeting.");
                self.events.error(&e);
                return Err(e);
            }
            Err(Refusal::Disabled) => {
                return Err(VoiceError::new(
                    VoiceErrorCode::CloudVoiceDisabled,
                    "Turn on cloud voice in Settings → Voice & privacy to talk to Pluto.",
                ))
            }
        }
        self.lock()
            .listen
            .as_ref()
            .map(|l| l.session_id.clone())
            .ok_or_else(|| VoiceError::new(VoiceErrorCode::Busy, "Pluto is still thinking."))
    }

    /// Key up (or the talk button again).
    pub fn ptt_up(self: &Arc<Self>) {
        let held = self.lock().listen.as_ref().map(|l| l.pressed.elapsed());
        if let Some(held) = held {
            let _ = self.apply(Input::PttUp { held }, None);
        }
    }

    /// Stops listening, thinking or speaking and goes back to Idle.
    pub fn cancel(self: &Arc<Self>) {
        let _ = self.apply(Input::Cancel, None);
    }

    /// A reply started playing (`voice_speak`); `handle` stops it on barge-in or cancel. A reply
    /// already playing is stopped. Returns the number to pass to [`speaking_done`](Self::speaking_done).
    pub fn speaking(self: &Arc<Self>, handle: AbortHandle) -> u64 {
        let (id, old) = {
            let mut inner = self.lock();
            inner.speak_seq += 1;
            let id = inner.speak_seq;
            (id, inner.speaking.replace((id, handle)))
        };
        if let Some((_, old)) = old {
            old.abort();
        }
        let _ = self.apply(Input::SpeakStart, None);
        id
    }

    /// Reply `id` finished (or was stopped). A reply that was already replaced changes nothing.
    pub fn speaking_done(self: &Arc<Self>, id: u64) {
        {
            let mut inner = self.lock();
            if inner.speaking.as_ref().map(|(n, _)| *n) != Some(id) {
                return;
            }
            inner.speaking = None;
        }
        let _ = self.apply(Input::SpeakEnd, None);
    }

    /// Stops the reply being spoken (`voice_stop_speaking`).
    pub fn stop_speaking(self: &Arc<Self>) {
        let playing = self.lock().speaking.take();
        if let Some((_, h)) = playing {
            h.abort();
            let _ = self.apply(Input::SpeakEnd, None);
        }
    }

    /// A meeting recording started: push-to-talk now answers `Busy`.
    pub fn recording_started(self: &Arc<Self>) -> VoiceResult<()> {
        self.apply(Input::RecordStart, None)
            .map_err(|_| VoiceError::new(VoiceErrorCode::Busy, "Finish talking to Pluto first."))
    }

    /// A recording is about to start. `Ok(true)`: voice is now `Recording` (push-to-talk answers
    /// Busy until `recording_stopped`). `Ok(false)`: voice is off, so there is nothing to hold and
    /// the recording may go ahead. `Err(Busy)`: Pluto is in the middle of a voice turn.
    pub fn try_recording_started(self: &Arc<Self>) -> VoiceResult<bool> {
        if self.state() == VoiceState::Disabled {
            return Ok(false);
        }
        self.recording_started().map(|()| true)
    }

    pub fn recording_stopped(self: &Arc<Self>) {
        let _ = self.apply(Input::RecordStop, None);
    }

    pub fn proposal_resolved(self: &Arc<Self>, approved: bool) {
        let _ = self.apply(Input::ProposalResolved { approved }, None);
    }

    pub fn execution_done(self: &Arc<Self>) {
        let _ = self.apply(Input::ExecutionDone, None);
    }

    /// Shows an error with its recovery (dismiss → Idle).
    pub fn fail(self: &Arc<Self>, error: &VoiceError) {
        self.events.error(error);
        let _ = self.apply(Input::Fail, Some(error.message.clone()));
    }

    pub fn dismiss(self: &Arc<Self>) {
        let _ = self.apply(Input::Dismiss, None);
    }

    /// Runs one input: the transition, then its effects, then `voice://status`.
    fn apply(self: &Arc<Self>, input: Input, detail: Option<String>) -> Result<(), Refusal> {
        let (status, work) = {
            let mut inner = self.lock();
            let (state, effects) = next(inner.status.state, &input, &self.policy)?;
            let mut work = Vec::new();
            for effect in effects {
                match effect {
                    Effect::Open => {
                        let session_id = pluto_core::ids::new_id();
                        let (tx, rx) = mpsc::channel(self.policy.buffer_frames + 2);
                        let stats = Arc::new(CaptureStats::default());
                        let kind = std::mem::take(&mut inner.next_kind);
                        inner.turn_kind = kind;
                        let auto = (kind == TurnKind::HandsFree).then(|| {
                            Arc::new(AutoEnd {
                                endpointer: Mutex::new(Endpointer::new(EndpointPolicy::default())),
                                runtime: Arc::downgrade(self),
                                session_id: session_id.clone(),
                                fired: std::sync::atomic::AtomicBool::new(false),
                            })
                        });
                        let mut sinks: Vec<Arc<dyn FrameSink>> =
                            vec![Arc::new(RealtimeSink(tx.clone(), stats.clone(), auto))];
                        sinks.extend((self.extra_sinks)(&session_id));
                        if let Err(e) = self.router.open(&session_id, sinks) {
                            // Only a duplicate id fails here, which new_id() rules out.
                            tracing::warn!(code = ?e.code, "couldn't open the push-to-talk route");
                        }
                        inner.listen = Some(Listen {
                            session_id: session_id.clone(),
                            pressed: Instant::now(),
                            tx,
                            stats: stats.clone(),
                            kind,
                        });
                        inner.turn_session = Some(session_id.clone());
                        work.push(Work::Listen(session_id, rx, stats));
                    }
                    Effect::Commit => {
                        if let Some(l) = inner.listen.take() {
                            tracing::info!(
                                frames = l.stats.frames(),
                                peak = l.stats.peak(),
                                held_ms = l.pressed.elapsed().as_millis() as u64,
                                "push-to-talk released"
                            );
                            if cfg!(debug_assertions) {
                                // Dev builds: visible in the `pnpm tauri dev` terminal.
                                eprintln!(
                                    "[voice] released after {} ms: {} frames reached Rust, peak {} of 32768",
                                    l.pressed.elapsed().as_millis(),
                                    l.stats.frames(),
                                    l.stats.peak()
                                );
                            }
                            self.router.close(&l.session_id);
                            let _ = l.tx.try_send(RtInput::Commit);
                        }
                    }
                    Effect::Discard => {
                        if let Some(l) = inner.listen.take() {
                            self.router.close(&l.session_id);
                            let _ = l.tx.try_send(RtInput::Close);
                        }
                        inner.turn_session = None;
                    }
                    Effect::StopSpeaking => {
                        if let Some((_, h)) = inner.speaking.take() {
                            h.abort();
                        }
                    }
                    // The listen task that sent `Heard` runs the assistant next.
                    Effect::Think => {}
                }
            }
            if !matches!(state, VoiceState::Listening | VoiceState::Thinking) {
                inner.turn_session = None;
            }
            let status = VoiceStatus {
                state,
                mode: match state {
                    VoiceState::Recording => Some(VoiceMode::Meeting),
                    VoiceState::Disabled | VoiceState::Idle | VoiceState::Error => None,
                    _ => Some(VoiceMode::Assistant),
                },
                detail: match state {
                    VoiceState::Error => detail,
                    VoiceState::Idle if input == Input::NothingHeard => detail.or_else(|| {
                        Some("Didn't catch that. Hold the key while you speak.".into())
                    }),
                    _ => None,
                },
                session_id: inner.listen.as_ref().map(|l| l.session_id.clone()),
            };
            let changed = status != inner.status;
            inner.status = status.clone();
            (changed.then_some(status), work)
        };
        if let Some(s) = status {
            self.events.status(&s);
        }
        for w in work {
            match w {
                Work::Listen(session_id, rx, stats) => {
                    let me = Arc::clone(self);
                    tokio::spawn(async move { me.listen(session_id, rx, stats).await });
                }
            }
        }
        Ok(())
    }

    /// One push-to-talk session: connect, forward frames, and after the commit wait for the
    /// final words, then think.
    async fn listen(
        self: Arc<Self>,
        session_id: String,
        mut rx: mpsc::Receiver<RtInput>,
        stats: Arc<CaptureStats>,
    ) {
        // Held too long: commit for the person.
        let me = Arc::clone(&self);
        let sid = session_id.clone();
        let kind = self.lock().turn_kind;
        let dictating = kind == TurnKind::Dictation;
        // Dictation can run for minutes; a spoken command is short.
        let max = if dictating {
            Duration::from_secs(15 * 60)
        } else {
            self.policy.max_press
        };
        let timer = tokio::spawn(async move {
            tokio::time::sleep(max).await;
            if me.is_listening(&sid) {
                let _ = me.apply(Input::AutoCommit, None);
            }
        });

        let mut config = (self.config)();
        if dictating {
            // Pauses to think are normal while dictating: don't let Gnani hang up on them.
            config.idle_timeout_sec = Some(300);
        }
        let mut session = match self.stt.open(config).await {
            Ok(s) => s,
            Err(e) => {
                timer.abort();
                if self.owns_turn(&session_id) {
                    self.fail(&e);
                }
                return;
            }
        };

        let mut words: Vec<RtTranscript> = Vec::new();
        // An error Gnani reported during the turn: shown instead of "didn't catch that".
        let mut last_error: Option<VoiceError> = None;
        let mut deadline: Option<Instant> = None;
        // When the turn was sent (key released, or the wake word's pause): the start of the
        // wait the person feels.
        let mut committed_at: Option<Instant> = None;
        let mut input_open = true;
        loop {
            let wait = async {
                match deadline {
                    Some(d) => tokio::time::sleep_until(d).await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                msg = rx.recv(), if input_open => match msg {
                    Some(RtInput::Commit) => {
                        let _ = session.input.send(RtInput::Commit).await;
                        committed_at = Some(Instant::now());
                        deadline = Some(Instant::now() + self.policy.final_wait);
                        input_open = false;
                    }
                    Some(RtInput::Close) | None => {
                        // Discarded (short press, cancel, voice turned off).
                        let _ = session.input.send(RtInput::Close).await;
                        timer.abort();
                        return;
                    }
                    Some(frame) => {
                        let _ = session.input.send(frame).await;
                    }
                },
                event = session.events.recv() => match event {
                    Some(RtEvent::Transcript(t)) => {
                        self.events.transcript(&session_id, &t);
                        if dictating {
                            self.type_phrase(&t.text, words.is_empty()).await;
                        }
                        words.push(t);
                        if deadline.is_some() {
                            break; // the words after the commit: that's the end of the turn
                        }
                    }
                    Some(RtEvent::Error(e)) => {
                        tracing::warn!(code = ?e.code, "Realtime error during push-to-talk");
                        if cfg!(debug_assertions) {
                            eprintln!("[voice] realtime error {:?}: {}", e.code, e.message);
                        }
                        last_error = Some(e.clone());
                        if !e.retryable {
                            let _ = session.input.send(RtInput::Close).await;
                            timer.abort();
                            if self.owns_turn(&session_id) {
                                self.fail(&e);
                            }
                            return;
                        }
                    }
                    Some(RtEvent::Closed) | None => {
                        if cfg!(debug_assertions) {
                            eprintln!("[voice] realtime closed");
                        }
                        break;
                    }
                    Some(other) => {
                        if cfg!(debug_assertions) {
                            eprintln!("[voice] realtime {other:?}");
                        }
                    }
                },
                () = wait => break,
            }
        }
        timer.abort();
        let _ = session.input.send(RtInput::Close).await;
        if !self.owns_turn(&session_id) || self.state() != VoiceState::Thinking {
            return;
        }
        if dictating {
            // Everything was typed as it came; there is nothing to think about.
            let _ = self.apply(Input::TurnDone, None);
            return;
        }

        let text = words
            .iter()
            .map(|w| w.text.trim())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if text.is_empty() {
            if let Some(e) = last_error {
                // Gnani said what went wrong: show that, not "didn't catch that".
                self.fail(&e);
                return;
            }
            let detail = nothing_heard_detail(stats.frames(), stats.peak());
            tracing::info!(
                frames = stats.frames(),
                peak = stats.peak(),
                "push-to-talk ended with no words"
            );
            if cfg!(debug_assertions) {
                eprintln!("[voice] no words came back: {detail}");
            }
            let _ = self.apply(Input::NothingHeard, Some(detail));
            return;
        }
        let heard = Heard {
            session_id: session_id.clone(),
            text,
            language: dominant_language(&words),
        };
        if self.apply(Input::Heard, None).is_err() {
            return;
        }
        let heard_at = Instant::now();
        let outcome = self.handler.turn(heard).await;
        if cfg!(debug_assertions) {
            let transcript = committed_at.map(|c| heard_at.duration_since(c).as_secs_f32());
            eprintln!(
                "[voice] timing: words {} after you stopped | reply ready {:.2} s later",
                transcript.map_or("?".into(), |t| format!("{t:.2} s")),
                heard_at.elapsed().as_secs_f32()
            );
        }
        if !self.owns_turn(&session_id) {
            return; // cancelled or a new press while thinking
        }
        match outcome {
            Ok(Turn::Done) => {
                let _ = self.apply(Input::TurnDone, None);
            }
            Ok(Turn::Reply) => {
                let _ = self.apply(Input::TurnReply, None);
            }
            Ok(Turn::Proposal) => {
                let _ = self.apply(Input::TurnProposal, None);
            }
            Err(e) => self.fail(&e),
        }
    }

    /// Types one dictated phrase, after a space unless it is the first. A typing failure is shown
    /// once and dictation carries on (the next phrase may work, for example in another window).
    async fn type_phrase(self: &Arc<Self>, text: &str, first: bool) {
        let phrase = text.trim();
        if phrase.is_empty() {
            return;
        }
        let Some(typer) = self.typer.get().cloned() else {
            return;
        };
        let out = if first {
            phrase.to_owned()
        } else {
            format!(" {phrase}")
        };
        let result = tokio::task::spawn_blocking(move || typer.type_text(&out)).await;
        if let Ok(Err(e)) = result {
            self.events.error(&e);
        }
    }

    fn is_listening(&self, session_id: &str) -> bool {
        self.lock()
            .listen
            .as_ref()
            .is_some_and(|l| l.session_id == session_id)
    }

    fn owns_turn(&self, session_id: &str) -> bool {
        self.lock().turn_session.as_deref() == Some(session_id)
    }
}

enum Work {
    Listen(String, mpsc::Receiver<RtInput>, Arc<CaptureStats>),
}

/// The language spoken longest across the turn's transcripts (by audio, else by count).
fn dominant_language(words: &[RtTranscript]) -> Option<String> {
    let mut by: Vec<(String, u64)> = Vec::new();
    for w in words {
        let Some(l) = &w.language else { continue };
        let weight = u64::from(w.audio_duration_ms.unwrap_or(1).max(1));
        match by.iter_mut().find(|(k, _)| k == l) {
            Some((_, t)) => *t += weight,
            None => by.push((l.clone(), weight)),
        }
    }
    by.into_iter().max_by_key(|(_, t)| *t).map(|(l, _)| l)
}

#[cfg(test)]
mod tests;
