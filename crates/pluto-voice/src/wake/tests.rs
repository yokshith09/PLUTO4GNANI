//! T-705: teaching checks, wake-word stripping, the listening loop (fake detector and runtime),
//! and the real Rustpotter detector on the synthetic fixtures in `test-data/wake/`.

use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

const FRAME: usize = FRAME_BYTES / 2; // 512 samples, 32 ms

fn tone(seconds: f32, amp: f32) -> Vec<i16> {
    (0..(seconds * RATE as f32) as usize)
        .map(|i| ((i as f32 * 0.07).sin() * amp) as i16)
        .collect()
}

fn silence(seconds: f32) -> Vec<i16> {
    vec![0; (seconds * RATE as f32) as usize]
}

fn bytes(s: &[i16]) -> Vec<u8> {
    s.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn fixture(name: &str) -> Vec<i16> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-data/wake")
        .join(name);
    let mut r = hound::WavReader::open(path).unwrap();
    r.samples::<i16>().map(Result::unwrap).collect()
}

// ---- teaching -----------------------------------------------------------------------------

#[test]
fn a_clear_word_is_kept_and_cut_to_the_word() {
    let rec = [silence(0.6), tone(0.6, 8000.), silence(0.6)].concat();
    let word = take_clip(&rec).unwrap();
    let secs = word.len() as f32 / RATE as f32;
    assert!((0.75..=0.85).contains(&secs), "{secs}");
}

#[test]
fn quiet_cut_off_short_or_long_recordings_are_refused_with_what_to_do() {
    let quiet = [silence(0.6), tone(0.6, 900.), silence(0.6)].concat();
    assert!(take_clip(&quiet).unwrap_err().contains("Too quiet"));
    let cut_off = [tone(0.8, 8000.), silence(1.0)].concat();
    assert!(take_clip(&cut_off).unwrap_err().contains("cut off"));
    let short = [silence(0.6), tone(0.1, 8000.), silence(0.6)].concat();
    assert!(take_clip(&short).unwrap_err().contains("Too short"));
    let long = [silence(0.2), tone(1.5, 8000.), silence(0.2)].concat();
    assert!(take_clip(&long).unwrap_err().contains("more than one word"));
    assert!(take_clip(&[]).is_err());
}

#[test]
fn the_store_needs_enough_recordings_then_builds_saves_and_resets() {
    let dir = tempfile::tempdir().unwrap();
    let store = WakeStore::new(dir.path().join("wake"));
    assert!(!store.enrolled());
    let a = fixture("pluto_a.wav");
    store.add_clip(&a).unwrap();
    assert!(store.build().is_err(), "one recording isn't enough");
    for _ in 1..NEEDED {
        store.add_clip(&a).unwrap();
    }
    assert_eq!(store.clips(), NEEDED);
    store.build().unwrap();
    assert!(store.enrolled());
    assert!(store.reference().is_some());
    store.reset().unwrap();
    assert!(!store.enrolled());
    assert_eq!(store.clips(), 0);
}

#[test]
fn the_real_detector_hears_a_taught_pluto_and_not_silence_or_a_tone() {
    let dir = tempfile::tempdir().unwrap();
    let store = WakeStore::new(dir.path());
    let (a, b) = (fixture("pluto_a.wav"), fixture("pluto_b.wav"));
    for k in 0..NEEDED {
        // Slightly louder and quieter takes, as a person gives.
        let g = 0.8 + 0.1 * k as f32;
        let take: Vec<i16> = (if k % 2 == 0 { &a } else { &b })
            .iter()
            .map(|x| (*x as f32 * g) as i16)
            .collect();
        store.add_clip(&take).unwrap();
    }
    store.build().unwrap();
    let mut d = PlutoDetector::new(store.reference().unwrap()).unwrap();
    assert!(!d.feed(&silence(3.)), "silence");
    assert!(!d.feed(&tone(3., 6000.)), "a tone");
    let said = [silence(1.), b.clone(), silence(2.)].concat();
    let heard = said.chunks(FRAME).any(|c| d.feed(c));
    assert!(heard, "a taught \u{201c}Pluto\u{201d}");
}

// ---- the command -------------------------------------------------------------------------

#[test]
fn the_wake_word_is_taken_off_the_command() {
    assert_eq!(strip_wake_word("Pluto, add a task"), "add a task");
    assert_eq!(strip_wake_word("pluto meetings kholo"), "meetings kholo");
    assert_eq!(
        strip_wake_word("Hey Pluto! aaj ke kaam batao"),
        "aaj ke kaam batao"
    );
    assert_eq!(strip_wake_word("प्लूटो, कल का रिमाइंडर"), "कल का रिमाइंडर");
    assert_eq!(strip_wake_word("Plutonium is heavy"), "Plutonium is heavy");
    assert_eq!(
        strip_wake_word("add a task for Pluto"),
        "add a task for Pluto"
    );
    assert_eq!(strip_wake_word("Pluto"), "");
}

// ---- the listening loop --------------------------------------------------------------------

/// "Pluto" is any frame louder than 20000.
struct LoudIsPluto;

impl Detector for LoudIsPluto {
    fn feed(&mut self, pcm: &[i16]) -> bool {
        pcm.iter().any(|x| x.unsigned_abs() > 20_000)
    }
    fn reset(&mut self) {}
}

struct FakeTurns {
    state: Mutex<VoiceState>,
    session: Mutex<Option<String>>,
    pushed: Mutex<Vec<(String, Vec<u8>)>>,
    starts: AtomicUsize,
    stops: AtomicUsize,
}

impl Default for FakeTurns {
    fn default() -> Self {
        Self {
            state: Mutex::new(VoiceState::Disabled),
            session: Mutex::new(None),
            pushed: Mutex::new(Vec::new()),
            starts: AtomicUsize::new(0),
            stops: AtomicUsize::new(0),
        }
    }
}

impl Turns for FakeTurns {
    fn state(&self) -> VoiceState {
        *self.state.lock().unwrap()
    }
    fn listening(&self) -> Option<String> {
        self.session.lock().unwrap().clone()
    }
    fn start(&self) -> Option<String> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        *self.state.lock().unwrap() = VoiceState::Listening;
        *self.session.lock().unwrap() = Some("wake-turn".into());
        Some("wake-turn".into())
    }
    fn stop(&self) {
        self.stops.fetch_add(1, Ordering::SeqCst);
        *self.state.lock().unwrap() = VoiceState::Thinking;
        *self.session.lock().unwrap() = None;
    }
    fn push(&self, session: &str, pcm: Vec<u8>) {
        self.pushed.lock().unwrap().push((session.into(), pcm));
    }
}

impl FakeTurns {
    fn idle() -> Arc<Self> {
        let t = Self::default();
        *t.state.lock().unwrap() = VoiceState::Idle;
        Arc::new(t)
    }
    fn pushed_to(&self, session: &str) -> usize {
        self.pushed
            .lock()
            .unwrap()
            .iter()
            .filter(|(s, _)| s == session)
            .count()
    }
}

/// Feeds `audio` frame by frame, letting the loop handle each before the next.
async fn play(audio: &[i16], turns: Arc<FakeTurns>, before: impl FnOnce(&FakeTurns)) {
    before(&turns);
    let (tx, rx) = mpsc::channel(4096);
    let task = tokio::spawn(run(
        rx,
        Box::new(LoudIsPluto),
        turns.clone(),
        WakePolicy::default(),
    ));
    for c in audio.chunks_exact(FRAME) {
        tx.send(bytes(c)).await.unwrap();
    }
    drop(tx);
    task.await.unwrap();
}

#[tokio::test]
async fn pluto_starts_a_turn_that_gets_the_command_and_ends_after_a_pause() {
    let turns = FakeTurns::idle();
    // Quiet room, "Pluto", the command (1 s of speech), then a pause.
    let audio = [
        silence(1.0),
        tone(0.1, 30_000.),
        tone(1.0, 3000.),
        silence(2.0),
    ]
    .concat();
    play(&audio, turns.clone(), |_| {}).await;
    assert_eq!(turns.starts.load(Ordering::SeqCst), 1);
    assert_eq!(
        turns.stops.load(Ordering::SeqCst),
        1,
        "the pause ended the command"
    );
    let sent = turns.pushed_to("wake-turn");
    // Half a second before the detection, the 1 s command, then up to the 1.2 s pause.
    let expected = 16 + 32 + 38;
    assert!(
        (expected - 4..=expected + 4).contains(&sent),
        "{sent} frames went to the turn"
    );
}

#[tokio::test]
async fn nothing_is_sent_while_waiting_for_pluto() {
    let turns = FakeTurns::idle();
    play(
        &[silence(1.0), tone(2.0, 3000.)].concat(),
        turns.clone(),
        |_| {},
    )
    .await;
    assert_eq!(turns.starts.load(Ordering::SeqCst), 0);
    assert!(
        turns.pushed.lock().unwrap().is_empty(),
        "talk without Pluto stays here"
    );
}

#[tokio::test]
async fn pluto_followed_by_nothing_ends_the_turn_after_four_seconds() {
    let turns = FakeTurns::idle();
    play(
        &[tone(0.1, 30_000.), silence(6.0)].concat(),
        turns.clone(),
        |_| {},
    )
    .await;
    assert_eq!(turns.stops.load(Ordering::SeqCst), 1);
    let sent = turns.pushed_to("wake-turn");
    assert!((125..=135).contains(&sent), "{sent} frames, about 4 s");
}

#[tokio::test]
async fn a_long_command_is_cut_at_fifteen_seconds() {
    let turns = FakeTurns::idle();
    play(
        &[tone(0.1, 30_000.), tone(20.0, 3000.)].concat(),
        turns.clone(),
        |_| {},
    )
    .await;
    assert_eq!(turns.stops.load(Ordering::SeqCst), 1);
    assert!(turns.pushed_to("wake-turn") <= 470 + 16);
}

#[tokio::test]
async fn a_hotkey_turn_gets_the_microphone_and_is_not_ended_by_a_pause() {
    let turns = Arc::new(FakeTurns::default());
    play(
        &[tone(1.0, 3000.), silence(3.0)].concat(),
        turns.clone(),
        |t| {
            *t.state.lock().unwrap() = VoiceState::Listening;
            *t.session.lock().unwrap() = Some("hotkey".into());
        },
    )
    .await;
    assert_eq!(turns.pushed_to("hotkey"), 125, "every frame");
    assert_eq!(turns.stops.load(Ordering::SeqCst), 0, "the key ends it");
    assert_eq!(turns.starts.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn while_pluto_is_busy_nothing_is_detected_or_sent() {
    for busy in [
        VoiceState::Thinking,
        VoiceState::Executing,
        VoiceState::Recording,
        VoiceState::Disabled,
    ] {
        let turns = Arc::new(FakeTurns::default());
        play(
            &[tone(0.1, 30_000.), tone(1.0, 3000.)].concat(),
            turns.clone(),
            |t| {
                *t.state.lock().unwrap() = busy;
            },
        )
        .await;
        assert_eq!(turns.starts.load(Ordering::SeqCst), 0, "{busy:?}");
        assert!(turns.pushed.lock().unwrap().is_empty(), "{busy:?}");
    }
}

#[tokio::test]
async fn pluto_works_wherever_push_to_talk_does() {
    // A proposal waiting for "haan", after an error, and over a reply (push-to-talk stops it).
    for state in [
        VoiceState::AwaitingApproval,
        VoiceState::Error,
        VoiceState::Speaking,
    ] {
        let turns = Arc::new(FakeTurns::default());
        let audio = [tone(0.1, 30_000.), tone(0.8, 3000.), silence(2.0)].concat();
        play(&audio, turns.clone(), |t| {
            *t.state.lock().unwrap() = state;
        })
        .await;
        assert_eq!(turns.starts.load(Ordering::SeqCst), 1, "{state:?}");
        assert_eq!(turns.stops.load(Ordering::SeqCst), 1, "{state:?}");
        assert!(turns.pushed_to("wake-turn") > 0, "{state:?}");
    }
}
