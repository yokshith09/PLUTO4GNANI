//! T-207: chunked recording, pause and resume, consent, crash recovery. No microphone needed:
//! frames are made up (silence) at the real size and rate.

use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono_tz::Tz;
use pluto_contracts::error::VoiceErrorCode;
use pluto_contracts::speech::{AudioFrame, FrameSink};
use pluto_core::bus::EventBus;
use pluto_core::clock::{Clock, FakeClock};
use pluto_core::db::Database;
use pluto_core::meeting_ai_store as store;
use pluto_intel::meeting::recorder::{
    repair_wav, ChunkEvent, Recorder, RecordingSink, Segmenter, BYTES_PER_MS,
};

fn frame(seq: u64) -> AudioFrame {
    AudioFrame {
        session_id: "s".into(),
        seq,
        pcm: vec![0u8; 1024],
    }
}

/// (riff size, data size, total length) from a WAV file.
fn sizes(path: &Path) -> (u32, u32, u64) {
    let b = std::fs::read(path).unwrap();
    assert_eq!(&b[0..4], b"RIFF");
    assert_eq!(&b[8..16], b"WAVEfmt ");
    assert_eq!(u16::from_le_bytes([b[20], b[21]]), 1, "PCM");
    assert_eq!(
        u32::from_le_bytes([b[24], b[25], b[26], b[27]]),
        16_000,
        "16 kHz"
    );
    (
        u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
        u32::from_le_bytes([b[40], b[41], b[42], b[43]]),
        b.len() as u64,
    )
}

#[test]
fn chunks_rotate_at_the_limit_with_cumulative_offsets_and_valid_headers() {
    let dir = tempfile::tempdir().unwrap();
    let mut seg = Segmenter::with_limits(dir.path(), "m", 4096, u64::MAX);
    let mut events = Vec::new();
    for _ in 0..10 {
        events.extend(seg.write(&[0u8; 1024]).unwrap());
    }
    events.extend(seg.close_current().unwrap());
    let kinds: Vec<_> = events
        .iter()
        .map(|e| match e {
            ChunkEvent::Opened {
                index, offset_ms, ..
            } => ("open", *index, *offset_ms),
            ChunkEvent::Closed { index, duration_ms } => ("close", *index, *duration_ms),
        })
        .collect();
    assert_eq!(
        kinds,
        [
            ("open", 0, 0),
            ("close", 0, 128),
            ("open", 1, 128),
            ("close", 1, 128),
            ("open", 2, 256),
            ("close", 2, 64)
        ]
    );
    for (i, bytes) in [(0, 4096u32), (1, 4096), (2, 2048)] {
        let (riff, data, len) = sizes(&dir.path().join(format!("m-{i:03}.wav")));
        assert_eq!(
            (data, riff, len),
            (bytes, 36 + bytes, 44 + u64::from(bytes))
        );
    }
}

#[test]
fn a_six_minute_recording_gives_two_chunks_with_correct_offsets() {
    let dir = tempfile::tempdir().unwrap();
    let mut seg = Segmenter::new(dir.path(), "meet");
    let frames = 360_000 * BYTES_PER_MS / 1024; // 6 minutes
    let mut closed = Vec::new();
    let mut opened = Vec::new();
    for _ in 0..frames {
        for e in seg.write(&[0u8; 1024]).unwrap() {
            match e {
                ChunkEvent::Closed { duration_ms, .. } => closed.push(duration_ms),
                ChunkEvent::Opened { offset_ms, .. } => opened.push(offset_ms),
            }
        }
    }
    closed.extend(seg.close_current().unwrap().map(|e| match e {
        ChunkEvent::Closed { duration_ms, .. } => duration_ms,
        ChunkEvent::Opened { .. } => unreachable!(),
    }));
    assert_eq!(closed.len(), 2, "two chunks");
    assert!(
        (270_000..270_100).contains(&closed[0]),
        "first chunk is about 4.5 min, got {}",
        closed[0]
    );
    assert_eq!(
        opened,
        [0, closed[0]],
        "the second chunk starts where the first ended"
    );
    assert!(
        (closed[0] + closed[1] - 360_000).abs() < 64,
        "no audio lost"
    );
}

#[test]
fn the_header_is_kept_current_while_recording_and_a_crash_can_be_repaired() {
    let dir = tempfile::tempdir().unwrap();
    // Flush every 2 KB; write 5 KB; never close (a crash).
    let mut seg = Segmenter::with_limits(dir.path(), "crash", u64::MAX, 2048);
    for _ in 0..5 {
        seg.write(&[0u8; 1024]).unwrap();
    }
    let path = dir.path().join("crash-000.wav");
    let (_, data_mid, len) = sizes(&path);
    assert_eq!(len, 44 + 5120);
    assert_eq!(data_mid, 4096, "the header was last updated at the 4 KB flush, so a crash loses under 2 KB of header, not the audio");
    drop(seg); // as if the process died
    let ms = repair_wav(&path).unwrap();
    assert_eq!(ms, 160);
    let (riff, data, _) = sizes(&path);
    assert_eq!((data, riff), (5120, 36 + 5120));
    // A stray odd byte at the end is trimmed to whole samples.
    {
        use std::io::Write;
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(&[1])
            .unwrap();
    }
    assert_eq!(repair_wav(&path).unwrap(), 160);
    // Not a recording at all.
    std::fs::write(dir.path().join("junk.wav"), b"tiny").unwrap();
    assert!(repair_wav(&dir.path().join("junk.wav")).is_err());
}

#[test]
fn pausing_drops_frames_and_resuming_starts_a_new_chunk() {
    let dir = tempfile::tempdir().unwrap();
    let log = Arc::new(Mutex::new(Vec::new()));
    let l = log.clone();
    let sink = RecordingSink::new(Segmenter::new(dir.path(), "p"), move |e| {
        l.lock().unwrap().push(e)
    });
    for i in 0..4 {
        sink.push(&frame(i)).unwrap();
    }
    sink.pause().unwrap();
    assert!(sink.is_paused());
    for i in 4..10 {
        sink.push(&frame(i)).unwrap();
    } // dropped
    assert_eq!(sink.frames(), 4);
    sink.resume();
    for i in 10..12 {
        sink.push(&frame(i)).unwrap();
    }
    sink.end("s").unwrap();
    let events = log.lock().unwrap().clone();
    assert_eq!(events.len(), 4, "open, close, open, close: {events:?}");
    assert!(
        matches!(
            events[2],
            ChunkEvent::Opened {
                index: 1,
                offset_ms: 128,
                ..
            }
        ),
        "the second chunk starts after the first one's 128 ms of audio, not after the pause"
    );
    assert_eq!(sizes(&dir.path().join("p-001.wav")).1, 2048);
}

struct Rig {
    _dir: tempfile::TempDir,
    db: Arc<Database>,
    rec: Recorder,
    audio: std::path::PathBuf,
    clock: Arc<FakeClock>,
}

fn rig() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(Database::open(&dir.path().join("pluto.db"), EventBus::default()).unwrap());
    let clock = Arc::new(FakeClock::new(1_793_600_000_000, Tz::Asia__Kolkata));
    let audio = dir.path().join("recordings");
    let rec = Recorder::new(db.clone(), clock.clone() as Arc<dyn Clock>, &audio);
    Rig {
        _dir: dir,
        db,
        rec,
        audio,
        clock,
    }
}

#[tokio::test]
async fn no_consent_means_no_meeting_and_no_recording() {
    let r = rig();
    let err = r
        .rec
        .start("Standup", vec![], false, false)
        .await
        .map(|_| ())
        .unwrap_err();
    assert_eq!(err.code, VoiceErrorCode::ConsentRequired);
    assert!(meetings_count(&r).await == 0);
    assert!(!r.audio.exists(), "nothing was written to disk");
}

async fn meetings_count(r: &Rig) -> usize {
    pluto_core::meetings::list(&r.db, Default::default())
        .await
        .unwrap()
        .len()
}

#[tokio::test]
async fn a_recording_is_saved_chunk_by_chunk_and_queues_the_transcription_on_stop() {
    let r = rig();
    let a = r
        .rec
        .start("  Launch sync ", vec!["Asha".into()], true, true)
        .await
        .unwrap();
    assert_eq!((a.session.live_captions, a.session.chunk_count), (true, 0));
    let mid = a.session.meeting_id.clone();
    let status = |db: Arc<Database>, m: String| async move {
        db.read(move |c| store::get_status(c, &m))
            .await
            .unwrap()
            .unwrap()
            .status
    };
    assert_eq!(status(r.db.clone(), mid.clone()).await, "recording");

    for i in 0..40 {
        a.sink.push(&frame(i)).unwrap();
    }
    r.rec.pause(&a).await.unwrap();
    for i in 40..50 {
        a.sink.push(&frame(i)).unwrap();
    } // while paused
    r.rec.resume(&a);
    for i in 50..70 {
        a.sink.push(&frame(i)).unwrap();
    }
    let done = r.rec.stop(&a).await.unwrap();
    assert_eq!(done.chunk_count, 2);

    let rows =
        r.db.read({
            let m = mid.clone();
            move |c| store::list_recordings(c, &m)
        })
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        (rows[0].offset_ms, rows[0].duration_ms),
        (0, Some(40 * 1024 / 32))
    );
    assert_eq!(
        (rows[1].offset_ms, rows[1].duration_ms),
        (1280, Some(20 * 1024 / 32))
    );
    assert!(rows.iter().all(|x| x.source == "mic"
        && x.consent_confirmed_at.is_some()
        && Path::new(&x.file_path).exists()));
    assert_eq!(status(r.db.clone(), mid.clone()).await, "transcribing");
    let job =
        r.db.read({
            let m = mid.clone();
            move |c| store::latest_job(c, &m)
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(job.status, "queued", "Gnani Batch (T-201) picks this up");
}

#[tokio::test]
async fn an_unfinished_recording_is_repaired_on_the_next_start() {
    let r = rig();
    let a = r.rec.start("Crashy", vec![], false, true).await.unwrap();
    let mid = a.session.meeting_id.clone();
    for i in 0..30 {
        a.sink.push(&frame(i)).unwrap();
    }
    // Let the row writer finish, then "crash": no stop, no end.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let before = r.db.read(store::unfinished_recordings).await.unwrap();
    assert_eq!(before.len(), 1);
    drop(a);

    let fresh = Recorder::new(r.db.clone(), r.clock.clone() as Arc<dyn Clock>, &r.audio);
    let found = fresh.recover().await.unwrap();
    assert_eq!(found, std::slice::from_ref(&mid));
    let rows =
        r.db.read(move |c| store::list_recordings(c, &mid))
            .await
            .unwrap();
    assert_eq!(rows[0].duration_ms, Some(30 * 1024 / 32));
    assert!(
        fresh.recover().await.unwrap().is_empty(),
        "nothing left to recover"
    );
    let (_, data, _) = sizes(Path::new(&rows[0].file_path));
    assert_eq!(u64::from(data), 30 * 1024);
}
