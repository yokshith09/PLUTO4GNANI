//! Spoken replies that never change ("Opening Meetings.", "Done. Task added.") are kept as audio
//! on this computer the first time Timbre speaks them, and replayed from disk after that: no
//! network, no wait (Timbre's first audio is about 0.5–0.6 s warm, `gnani-live-checks.md`).
//!
//! Owner: **D1**. Only fixed phrases are kept: short, with no digits and no quotes, so task
//! titles, times and counts (the user's own content) are never stored. One file per text, voice,
//! model and language; at most [`MAX_FILES`], oldest removed first.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use pluto_contracts::error::VoiceResult;
use pluto_contracts::speech::{AudioContainer, PcmSink, TextToSpeech, TtsRequest};

pub const MAX_FILES: usize = 150;
/// Longest text kept (a fixed reply is short).
const MAX_CHARS: usize = 60;
/// Replayed in 0.1 s pieces (PCM16, 24 kHz mono), as Timbre sends them.
const CHUNK: usize = 4800;

/// A reply worth keeping: short, and nothing in it that came from the user's data.
pub fn cacheable(text: &str) -> bool {
    let t = text.trim();
    !t.is_empty()
        && t.chars().count() <= MAX_CHARS
        // The assistant puts the user's own words (titles, quotes) in quotation marks, and
        // times and counts are digits: either means it isn't a fixed reply.
        && !t
            .chars()
            .any(|c| c.is_ascii_digit() || matches!(c, '"' | '\u{201c}' | '\u{201d}'))
}

/// FNV-1a: a stable file name for a text and how it is spoken.
fn key(r: &TtsRequest) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let parts = [
        r.text.trim(),
        &r.language,
        r.voice.as_deref().unwrap_or(""),
        r.model.as_deref().unwrap_or(""),
    ];
    for part in parts {
        for b in part.bytes().chain([0u8]) {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{h:016x}.pcm")
}

/// Timbre with a disk cache of fixed replies in front of it.
pub struct CachedTts {
    inner: Arc<dyn TextToSpeech>,
    dir: PathBuf,
}

impl CachedTts {
    pub fn new(inner: Arc<dyn TextToSpeech>, dir: impl Into<PathBuf>) -> Self {
        Self {
            inner,
            dir: dir.into(),
        }
    }

    fn load(&self, r: &TtsRequest) -> Option<Vec<u8>> {
        std::fs::read(self.dir.join(key(r)))
            .ok()
            .filter(|b| !b.is_empty())
    }

    fn save(&self, r: &TtsRequest, pcm: &[u8]) {
        if std::fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        let path = self.dir.join(key(r));
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, pcm).is_ok() && std::fs::rename(&tmp, &path).is_ok() {
            self.prune();
        } else {
            let _ = std::fs::remove_file(&tmp);
        }
    }

    /// Keeps the newest [`MAX_FILES`].
    fn prune(&self) {
        let Ok(dir) = std::fs::read_dir(&self.dir) else {
            return;
        };
        let mut files: Vec<(std::time::SystemTime, PathBuf)> = dir
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "pcm"))
            .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
            .collect();
        if files.len() <= MAX_FILES {
            return;
        }
        files.sort();
        for (_, p) in &files[..files.len() - MAX_FILES] {
            let _ = std::fs::remove_file(p);
        }
    }
}

fn dev_log(first_audio: Option<Instant>, started: Instant, source: &str) {
    if cfg!(debug_assertions) {
        if let Some(first) = first_audio {
            eprintln!(
                "[voice] timing: first audio after {:.2} s ({source})",
                first.duration_since(started).as_secs_f32()
            );
        }
    }
}

#[async_trait]
impl TextToSpeech for CachedTts {
    async fn stream(&self, request: TtsRequest, out: PcmSink) -> VoiceResult<()> {
        let started = Instant::now();
        let keep = cacheable(&request.text);
        if keep {
            if let Some(pcm) = self.load(&request) {
                dev_log(Some(Instant::now()), started, "saved on this computer");
                for piece in pcm.chunks(CHUNK) {
                    if out.send(piece.to_vec()).await.is_err() {
                        break; // stopped (barge-in) or the page went away
                    }
                }
                return Ok(());
            }
        }
        if !keep {
            let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(16);
            let produce = self.inner.stream(request, tx);
            let forward = async {
                let mut first = None;
                while let Some(chunk) = rx.recv().await {
                    first.get_or_insert_with(Instant::now);
                    if out.send(chunk).await.is_err() {
                        break;
                    }
                }
                first
            };
            let (spoken, first) = tokio::join!(produce, forward);
            dev_log(first, started, "Timbre");
            return spoken;
        }
        // Speak it as it arrives, and keep a copy.
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(16);
        let produce = self.inner.stream(request.clone(), tx);
        let forward = async {
            let mut all = Vec::new();
            let mut first = None;
            let mut complete = true;
            while let Some(chunk) = rx.recv().await {
                first.get_or_insert_with(Instant::now);
                all.extend_from_slice(&chunk);
                if out.send(chunk).await.is_err() {
                    complete = false;
                    break;
                }
            }
            (all, first, complete)
        };
        let (spoken, (all, first, complete)) = tokio::join!(produce, forward);
        dev_log(first, started, "Timbre");
        // Only a whole reply that played to the end is kept.
        if spoken.is_ok() && complete && !all.is_empty() {
            self.save(&request, &all);
        }
        spoken
    }

    async fn synthesize(
        &self,
        request: TtsRequest,
        container: AudioContainer,
    ) -> VoiceResult<Vec<u8>> {
        self.inner.synthesize(request, container).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    /// Timbre stand-in: 0.25 s of audio per call, counted.
    #[derive(Default)]
    struct CountingTts(AtomicUsize);

    #[async_trait]
    impl TextToSpeech for CountingTts {
        async fn stream(&self, _r: TtsRequest, out: PcmSink) -> VoiceResult<()> {
            self.0.fetch_add(1, Ordering::SeqCst);
            for i in 0..3u8 {
                let _ = out.send(vec![i; 4000]).await;
            }
            Ok(())
        }
        async fn synthesize(&self, _r: TtsRequest, _c: AudioContainer) -> VoiceResult<Vec<u8>> {
            Ok(vec![])
        }
    }

    fn req(text: &str, voice: &str) -> TtsRequest {
        TtsRequest {
            text: text.into(),
            language: "en-IN".into(),
            voice: Some(voice.into()),
            model: None,
            speed: None,
        }
    }

    async fn speak(tts: &CachedTts, r: TtsRequest) -> Vec<u8> {
        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
        tts.stream(r, tx).await.unwrap();
        let mut all = Vec::new();
        while let Ok(c) = rx.try_recv() {
            all.extend(c);
        }
        all
    }

    #[test]
    fn only_fixed_replies_are_kept() {
        for text in [
            "Opening Meetings.",
            "Done. Task added.",
            "Meetings khol raha hoon.",
            "Okay, I didn't change anything.",
        ] {
            assert!(cacheable(text), "{text}");
        }
        for text in [
            "Add \u{201c}Call Ravi\u{201d} for Tue, 10 Nov?",
            "You have 3 tasks today: Budget, Rent, Gym.",
            "Reminder at 5:00 PM?",
            "",
            &"Long reply ".repeat(10),
        ] {
            assert!(!cacheable(text), "{text}");
        }
    }

    #[tokio::test]
    async fn a_fixed_reply_is_fetched_once_then_played_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let inner = Arc::new(CountingTts::default());
        let tts = CachedTts::new(inner.clone(), dir.path());
        let first = speak(&tts, req("Opening Meetings.", "Shivani")).await;
        let second = speak(&tts, req("Opening Meetings.", "Shivani")).await;
        assert_eq!(first.len(), 12_000);
        assert_eq!(second, first, "the same audio");
        assert_eq!(inner.0.load(Ordering::SeqCst), 1, "Timbre was asked once");
        // Another voice is another recording.
        speak(&tts, req("Opening Meetings.", "Avinash")).await;
        assert_eq!(inner.0.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn the_users_content_is_never_stored() {
        let dir = tempfile::tempdir().unwrap();
        let inner = Arc::new(CountingTts::default());
        let tts = CachedTts::new(inner.clone(), dir.path());
        let text = "Add \u{201c}Call Ravi\u{201d} for Tue, 10 Nov?";
        speak(&tts, req(text, "Shivani")).await;
        speak(&tts, req(text, "Shivani")).await;
        assert_eq!(inner.0.load(Ordering::SeqCst), 2);
        assert_eq!(
            std::fs::read_dir(dir.path())
                .map(|d| d.count())
                .unwrap_or(0),
            0
        );
    }

    #[tokio::test]
    async fn a_reply_cut_off_part_way_is_not_kept() {
        let dir = tempfile::tempdir().unwrap();
        let inner = Arc::new(CountingTts::default());
        let tts = CachedTts::new(inner.clone(), dir.path());
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        drop(rx); // the listener went away (barge-in)
        tts.stream(req("Opening Notes.", "Shivani"), tx)
            .await
            .unwrap();
        speak(&tts, req("Opening Notes.", "Shivani")).await;
        assert_eq!(
            inner.0.load(Ordering::SeqCst),
            2,
            "fetched again: the first was cut off"
        );
    }

    #[test]
    fn at_most_max_files_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let tts = CachedTts::new(Arc::new(CountingTts::default()), dir.path());
        for i in 0..MAX_FILES + 5 {
            let r = req(
                &format!("Reply number {}", "x".repeat(i % 50 + 1)),
                &format!("v{i}"),
            );
            tts.save(&r, &[1, 2, 3]);
        }
        let n = std::fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(n, MAX_FILES);
    }
}
