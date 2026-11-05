//! T-301: every transition of ARCHITECTURE §5, and the driver on the mock Realtime with paused
//! time (press → Listening at once, frames, commit, final words, barge-in, Busy, errors).

use std::sync::Mutex as StdMutex;

use pluto_contracts::speech::FRAME_BYTES;
use pluto_gnani::mock::MockStt;

use super::*;

use Effect as E;
use Input as I;
use VoiceState as S;

fn go(state: S, input: I) -> Result<(S, Vec<E>), Refusal> {
    next(state, &input, &RuntimePolicy::default())
}

#[test]
fn every_transition_in_the_table() {
    let long = I::PttUp {
        held: Duration::from_secs(2),
    };
    let short = I::PttUp {
        held: Duration::from_millis(299),
    };
    let cases: Vec<(S, I, S, Vec<E>)> = vec![
        (S::Disabled, I::Enable, S::Idle, vec![]),
        (S::Idle, I::PttDown, S::Listening, vec![E::Open]),
        (S::Idle, I::RecordStart, S::Recording, vec![]),
        (S::Idle, I::SpeakStart, S::Speaking, vec![]),
        (S::Listening, long.clone(), S::Thinking, vec![E::Commit]),
        (S::Listening, I::AutoCommit, S::Thinking, vec![E::Commit]),
        (S::Listening, short, S::Idle, vec![E::Discard]),
        (S::Listening, I::Cancel, S::Idle, vec![E::Discard]),
        (S::Listening, I::Fail, S::Error, vec![E::Discard]),
        (S::Thinking, I::Heard, S::Thinking, vec![E::Think]),
        (S::Thinking, I::NothingHeard, S::Idle, vec![]),
        (S::Thinking, I::TurnDone, S::Idle, vec![]),
        (S::Thinking, I::TurnReply, S::Idle, vec![]),
        (S::Thinking, I::TurnProposal, S::AwaitingApproval, vec![]),
        (S::Thinking, I::Fail, S::Error, vec![]),
        (S::Thinking, I::Cancel, S::Idle, vec![]),
        (
            S::AwaitingApproval,
            I::ProposalResolved { approved: true },
            S::Executing,
            vec![],
        ),
        (
            S::AwaitingApproval,
            I::ProposalResolved { approved: false },
            S::Idle,
            vec![],
        ),
        (S::AwaitingApproval, I::PttDown, S::Listening, vec![E::Open]),
        (
            S::AwaitingApproval,
            I::SpeakStart,
            S::AwaitingApproval,
            vec![],
        ),
        (S::AwaitingApproval, I::Cancel, S::Idle, vec![]),
        (S::Executing, I::ExecutionDone, S::Idle, vec![]),
        (S::Executing, I::Fail, S::Error, vec![]),
        (S::Speaking, I::SpeakEnd, S::Idle, vec![]),
        (
            S::Speaking,
            I::PttDown,
            S::Listening,
            vec![E::StopSpeaking, E::Open],
        ),
        (S::Speaking, I::Cancel, S::Idle, vec![E::StopSpeaking]),
        (S::Speaking, I::SpeakStart, S::Speaking, vec![]),
        (S::Recording, I::RecordStop, S::Idle, vec![]),
        (S::Recording, I::Fail, S::Error, vec![]),
        (S::Error, I::Dismiss, S::Idle, vec![]),
        (S::Error, I::PttDown, S::Listening, vec![E::Open]),
        (S::Idle, I::Fail, S::Error, vec![]),
        // Turning voice off ends whatever was going on.
        (S::Idle, I::Disable, S::Disabled, vec![]),
        (S::Listening, I::Disable, S::Disabled, vec![E::Discard]),
        (S::Thinking, I::Disable, S::Disabled, vec![E::Discard]),
        (S::Speaking, I::Disable, S::Disabled, vec![E::StopSpeaking]),
        (S::Recording, I::Disable, S::Disabled, vec![]),
    ];
    for (from, input, to, effects) in cases {
        assert_eq!(
            go(from, input.clone()),
            Ok((to, effects)),
            "{from:?} + {input:?}"
        );
    }
}

#[test]
fn refusals() {
    assert_eq!(go(S::Recording, I::PttDown), Err(Refusal::Busy));
    assert_eq!(go(S::Disabled, I::PttDown), Err(Refusal::Disabled));
    // A key repeat while held, a press while thinking, stray events: ignored.
    for (state, input) in [
        (S::Listening, I::PttDown),
        (S::Thinking, I::PttDown),
        (
            S::Idle,
            I::PttUp {
                held: Duration::from_secs(1),
            },
        ),
        (S::Idle, I::SpeakEnd),
        (S::Recording, I::SpeakStart),
        (S::Idle, I::Enable),
        (S::Disabled, I::Disable),
        (S::Disabled, I::SpeakStart),
        (S::Executing, I::PttDown),
    ] {
        assert_eq!(
            go(state, input.clone()),
            Err(Refusal::Ignored),
            "{state:?} + {input:?}"
        );
    }
}

// ---- the driver ----------------------------------------------------------------------------

#[derive(Default)]
struct Seen {
    statuses: StdMutex<Vec<VoiceStatus>>,
    transcripts: StdMutex<Vec<String>>,
    errors: StdMutex<Vec<VoiceError>>,
}

impl RuntimeEvents for Seen {
    fn status(&self, s: &VoiceStatus) {
        self.statuses.lock().unwrap().push(s.clone());
    }
    fn transcript(&self, _: &str, t: &RtTranscript) {
        self.transcripts.lock().unwrap().push(t.text.clone());
    }
    fn error(&self, e: &VoiceError) {
        self.errors.lock().unwrap().push(e.clone());
    }
}

impl Seen {
    fn states(&self) -> Vec<S> {
        self.statuses
            .lock()
            .unwrap()
            .iter()
            .map(|s| s.state)
            .collect()
    }
}

struct Assistant {
    reply: Turn,
    heard: StdMutex<Vec<Heard>>,
}

#[async_trait]
impl TurnHandler for Assistant {
    async fn turn(&self, heard: Heard) -> VoiceResult<Turn> {
        self.heard.lock().unwrap().push(heard);
        Ok(self.reply)
    }
}

struct Rig {
    rt: Arc<VoiceRuntime>,
    router: Arc<AudioRouter>,
    seen: Arc<Seen>,
    assistant: Arc<Assistant>,
}

fn rig(stt: MockStt, reply: Turn) -> Rig {
    let router = Arc::new(AudioRouter::new());
    let seen = Arc::new(Seen::default());
    let assistant = Arc::new(Assistant {
        reply,
        heard: StdMutex::default(),
    });
    let rt = Arc::new(VoiceRuntime::new(
        Arc::new(stt),
        router.clone(),
        seen.clone(),
        assistant.clone(),
        true,
    ));
    Rig {
        rt,
        router,
        seen,
        assistant,
    }
}

async fn speak_frames(r: &Rig, session: &str, n: usize) {
    for _ in 0..n {
        r.router.push(session, vec![0u8; FRAME_BYTES]).unwrap();
        tokio::time::sleep(Duration::from_millis(32)).await;
    }
}

/// Lets spawned tasks run until the runtime settles in `state` (paused time advances itself).
async fn until(r: &Rig, state: S) {
    for _ in 0..1000 {
        if r.rt.state() == state {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("never reached {state:?}; now {:?}", r.rt.state());
}

#[tokio::test(start_paused = true)]
async fn a_press_shows_listening_at_once_and_a_release_gives_the_assistant_the_words() {
    let r = rig(
        MockStt::new(["Kal ka standup Friday ko move karo"]).language("hi-IN"),
        Turn::Reply,
    );
    let session = r.rt.ptt_down().unwrap();
    // Listening is reported before anything is awaited: no network in the way.
    let first = r.seen.statuses.lock().unwrap()[0].clone();
    assert_eq!(first.state, S::Listening);
    assert_eq!(first.mode, Some(VoiceMode::Assistant));
    assert_eq!(first.session_id.as_deref(), Some(session.as_str()));
    assert!(r.router.is_open(&session));
    // A key repeat returns the same session.
    assert_eq!(r.rt.ptt_down().unwrap(), session);

    speak_frames(&r, &session, 20).await;
    r.rt.ptt_up();
    assert_eq!(r.rt.state(), S::Thinking);
    assert!(
        !r.router.is_open(&session),
        "the capture route closes on release"
    );
    until(&r, S::Idle).await;

    let heard = r.assistant.heard.lock().unwrap().clone();
    assert_eq!(heard.len(), 1);
    assert_eq!(heard[0].text, "Kal ka standup Friday ko move karo");
    assert_eq!(heard[0].language.as_deref(), Some("hi-IN"));
    assert_eq!(heard[0].session_id, session);
    assert_eq!(
        r.seen.transcripts.lock().unwrap().clone(),
        ["Kal ka standup Friday ko move karo"]
    );
    assert_eq!(r.seen.states(), [S::Listening, S::Thinking, S::Idle]);
}

#[tokio::test(start_paused = true)]
async fn a_short_press_is_ignored() {
    let r = rig(MockStt::new(["haan"]), Turn::Done);
    let session = r.rt.ptt_down().unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    r.rt.ptt_up();
    assert_eq!(r.rt.state(), S::Idle);
    assert!(!r.router.is_open(&session));
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(r.assistant.heard.lock().unwrap().is_empty());
}

#[tokio::test(start_paused = true)]
async fn silence_comes_back_to_idle_after_three_seconds_with_a_hint() {
    let r = rig(MockStt::new(Vec::<String>::new()), Turn::Done);
    let session = r.rt.ptt_down().unwrap();
    speak_frames(&r, &session, 15).await;
    let released = Instant::now();
    r.rt.ptt_up();
    until(&r, S::Idle).await;
    let waited = released.elapsed();
    assert!(
        waited >= Duration::from_secs(3) && waited < Duration::from_millis(3_100),
        "{waited:?}"
    );
    // The frames were all zeros: the message says it was silence, not that Gnani missed it.
    let last = r.rt.status();
    assert!(last.detail.unwrap().starts_with("Pluto only heard silence"));
    assert!(r.assistant.heard.lock().unwrap().is_empty());
}

/// A loud frame (a square wave at about half scale).
fn loud_frame() -> Vec<u8> {
    (0..FRAME_BYTES / 2)
        .flat_map(|i| {
            if (i / 20) % 2 == 0 {
                16_000i16
            } else {
                -16_000
            }
            .to_le_bytes()
        })
        .collect()
}

#[tokio::test(start_paused = true)]
async fn the_hint_says_whether_audio_arrived_was_silent_or_was_not_understood() {
    // Nothing arrived at all (the page never sent frames).
    let r = rig(MockStt::new(Vec::<String>::new()), Turn::Done);
    r.rt.ptt_down().unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    r.rt.ptt_up();
    until(&r, S::Idle).await;
    assert!(r
        .rt
        .status()
        .detail
        .unwrap()
        .starts_with("No sound reached Pluto"));

    // Real sound arrived, but no words came back.
    let session = r.rt.ptt_down().unwrap();
    for _ in 0..15 {
        r.router.push(&session, loud_frame()).unwrap();
        tokio::time::sleep(Duration::from_millis(32)).await;
    }
    r.rt.ptt_up();
    until(&r, S::Idle).await;
    assert!(r
        .rt
        .status()
        .detail
        .unwrap()
        .starts_with("Didn't catch that"));
}

#[test]
fn capture_stats_count_frames_and_the_loudest_sample() {
    let s = CaptureStats::default();
    assert_eq!((s.frames(), s.peak()), (0, 0));
    s.add(&[0, 0, 0x10, 0x00]); // 0, 16
    s.add(&(-300i16).to_le_bytes());
    assert_eq!((s.frames(), s.peak()), (2, 300));
    assert!(nothing_heard_detail(0, 0).starts_with("No sound"));
    assert!(nothing_heard_detail(10, SILENCE_PEAK - 1).starts_with("Pluto only heard silence"));
    assert!(nothing_heard_detail(10, 5_000).starts_with("Didn't catch that"));
}

#[tokio::test(start_paused = true)]
async fn a_press_held_for_a_minute_is_committed_for_you() {
    let r = rig(MockStt::new(["bahut lamba"]), Turn::Done);
    let _session = r.rt.ptt_down().unwrap();
    tokio::time::sleep(Duration::from_secs(61)).await;
    assert_ne!(r.rt.state(), S::Listening);
    until(&r, S::Idle).await;
    assert_eq!(r.assistant.heard.lock().unwrap()[0].text, "bahut lamba");
    r.rt.ptt_up(); // the late release changes nothing
    assert_eq!(r.rt.state(), S::Idle);
}

#[tokio::test(start_paused = true)]
async fn pressing_while_speaking_stops_the_reply() {
    let r = rig(MockStt::new(["ruko"]), Turn::Done);
    let reply = tokio::spawn(tokio::time::sleep(Duration::from_secs(30)));
    let id = r.rt.speaking(reply.abort_handle());
    assert_eq!(r.rt.state(), S::Speaking);

    r.rt.ptt_down().unwrap();
    assert_eq!(r.rt.state(), S::Listening);
    tokio::task::yield_now().await;
    assert!(
        reply.await.unwrap_err().is_cancelled(),
        "barge-in stops the reply"
    );
    // The stopped reply reporting late doesn't end the new turn.
    r.rt.speaking_done(id);
    assert_eq!(r.rt.state(), S::Listening);
    r.rt.cancel();
    assert_eq!(r.rt.state(), S::Idle);
}

#[tokio::test(start_paused = true)]
async fn a_finished_reply_returns_to_idle_and_only_the_newest_counts() {
    let r = rig(MockStt::new(Vec::<String>::new()), Turn::Done);
    let a = r.rt.speaking(tokio::spawn(async {}).abort_handle());
    let b = r.rt.speaking(tokio::spawn(async {}).abort_handle());
    r.rt.speaking_done(a);
    assert_eq!(r.rt.state(), S::Speaking);
    r.rt.speaking_done(b);
    assert_eq!(r.rt.state(), S::Idle);
}

#[tokio::test(start_paused = true)]
async fn push_to_talk_while_recording_is_busy() {
    let r = rig(MockStt::new(["x"]), Turn::Done);
    r.rt.recording_started().unwrap();
    assert_eq!(r.rt.status().mode, Some(VoiceMode::Meeting));
    let err = r.rt.ptt_down().unwrap_err();
    assert_eq!(err.code, VoiceErrorCode::Busy);
    assert_eq!(err.message, "Busy: recording a meeting.");
    assert_eq!(r.seen.errors.lock().unwrap().len(), 1);
    assert_eq!(r.rt.state(), S::Recording);
    r.rt.recording_stopped();
    assert_eq!(r.rt.state(), S::Idle);

    // And recording can't start in the middle of a turn.
    r.rt.ptt_down().unwrap();
    assert_eq!(
        r.rt.recording_started().unwrap_err().code,
        VoiceErrorCode::Busy
    );
}

#[tokio::test(start_paused = true)]
async fn disabled_voice_refuses_and_turning_it_on_makes_it_ready() {
    let router = Arc::new(AudioRouter::new());
    let rt = Arc::new(VoiceRuntime::new(
        Arc::new(MockStt::new(["x"])),
        router,
        Arc::new(Seen::default()),
        Arc::new(NoAssistant),
        false,
    ));
    assert_eq!(rt.state(), S::Disabled);
    assert_eq!(
        rt.ptt_down().unwrap_err().code,
        VoiceErrorCode::CloudVoiceDisabled
    );
    rt.set_enabled(true);
    assert_eq!(rt.state(), S::Idle);
    rt.ptt_down().unwrap();
    rt.set_enabled(false);
    assert_eq!(rt.state(), S::Disabled);
}

#[tokio::test(start_paused = true)]
async fn a_connection_failure_shows_the_error_and_dismiss_recovers() {
    let r = rig(
        MockStt::new(["x"]).failing(VoiceError::new(
            VoiceErrorCode::InvalidApiKey,
            "Gnani didn't accept your API key.",
        )),
        Turn::Done,
    );
    let session = r.rt.ptt_down().unwrap();
    until(&r, S::Error).await;
    assert_eq!(
        r.rt.status().detail.as_deref(),
        Some("Gnani didn't accept your API key.")
    );
    assert!(!r.router.is_open(&session));
    assert_eq!(
        r.seen.errors.lock().unwrap()[0].code,
        VoiceErrorCode::InvalidApiKey
    );
    r.rt.dismiss();
    assert_eq!(r.rt.state(), S::Idle);
}

#[tokio::test(start_paused = true)]
async fn a_proposal_waits_for_approval_then_executes() {
    let r = rig(MockStt::new(["task bana do"]), Turn::Proposal);
    let session = r.rt.ptt_down().unwrap();
    speak_frames(&r, &session, 15).await;
    r.rt.ptt_up();
    until(&r, S::AwaitingApproval).await;
    r.rt.proposal_resolved(true);
    assert_eq!(r.rt.state(), S::Executing);
    r.rt.execution_done();
    assert_eq!(r.rt.state(), S::Idle);
}

#[tokio::test(start_paused = true)]
async fn cancelling_while_thinking_drops_the_late_answer() {
    let r = rig(
        MockStt::new(["der se"]).delay(Duration::from_secs(2)),
        Turn::Proposal,
    );
    let session = r.rt.ptt_down().unwrap();
    speak_frames(&r, &session, 15).await;
    r.rt.ptt_up();
    r.rt.cancel();
    assert_eq!(r.rt.state(), S::Idle);
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(r.rt.state(), S::Idle, "the late transcript is ignored");
    assert!(r.assistant.heard.lock().unwrap().is_empty());
}

#[test]
fn the_dominant_language_is_the_one_spoken_longest() {
    let t = |l: &str, ms: u32| RtTranscript {
        text: "x".into(),
        segment_index: 0,
        language: Some(l.into()),
        audio_duration_ms: Some(ms),
        latency_ms: None,
    };
    assert_eq!(
        dominant_language(&[t("en-IN", 800), t("hi-IN", 2_000), t("en-IN", 900)]).as_deref(),
        Some("hi-IN")
    );
    assert_eq!(dominant_language(&[]), None);
}
