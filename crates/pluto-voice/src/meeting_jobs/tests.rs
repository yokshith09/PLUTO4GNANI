//! T-201 pipeline tests against a scripted Batch API (no network): import → job → segments in
//! the database, `PARTIAL_FAILURE` gap markers (T-FH-06), retry, and the import checks.

use std::sync::Mutex as StdMutex;
use std::time::Duration;

use async_trait::async_trait;
use chrono_tz::Tz;
use pluto_contracts::error::VoiceErrorCode;
use pluto_core::bus::EventBus;
use pluto_core::clock::FakeClock;
use pluto_gnani::batch::{BatchSegment, FileResult, JobState};

use super::*;

const START: i64 = 1_762_070_000_000;

/// A fake Gnani: every job ends with `end`; files listed in `failed` come back FAILED.
#[derive(Default)]
struct FakeGnani {
    end: StdMutex<String>,
    failed: StdMutex<Vec<String>>,
    uploads: StdMutex<Vec<(Vec<String>, BatchConfig)>>,
    polls: StdMutex<u32>,
}

impl FakeGnani {
    fn ending(end: &str) -> Arc<Self> {
        let f = Self::default();
        *f.end.lock().unwrap() = end.into();
        Arc::new(f)
    }
}

fn segs(names: &str) -> Vec<BatchSegment> {
    // Two speakers taking turns, 4 s each.
    names
        .split('|')
        .enumerate()
        .map(|(i, t)| BatchSegment {
            start_ms: i as i64 * 4_000,
            end_ms: i as i64 * 4_000 + 3_500,
            speaker: Some((i % 2) as u8 + 1),
            text: t.into(),
            language: Some("hi-IN".into()),
        })
        .collect()
}

#[async_trait]
impl BatchApi for FakeGnani {
    async fn create(&self, files: &[BatchFile], c: &BatchConfig, _: &str) -> VoiceResult<String> {
        let mut u = self.uploads.lock().unwrap();
        u.push((files.iter().map(|f| f.name.clone()).collect(), c.clone()));
        Ok(format!("g-{}", u.len()))
    }
    async fn start(&self, _: &str) -> VoiceResult<BatchStatus> {
        Ok(BatchStatus::Starting)
    }
    async fn status(&self, _: &str) -> VoiceResult<JobState> {
        let mut n = self.polls.lock().unwrap();
        *n += 1;
        let status = if *n % 2 == 1 {
            BatchStatus::InProgress
        } else {
            BatchStatus::parse(&self.end.lock().unwrap())
        };
        Ok(JobState {
            status,
            total_files: 1,
            completed_files: 0,
            failed_files: 0,
            cancel_reason: None,
        })
    }
    async fn files(&self, _: &str) -> VoiceResult<Vec<FileResult>> {
        let uploads = self.uploads.lock().unwrap();
        let (names, _) = uploads.last().unwrap();
        let failed = self.failed.lock().unwrap();
        Ok(names
            .iter()
            .map(|n| {
                let bad = failed.contains(n);
                FileResult {
                    file_id: None,
                    original_path: n.clone(),
                    status: if bad { "FAILED" } else { "COMPLETED" }.into(),
                    transcript_url: (!bad).then(|| format!("https://store.example/{n}.json")),
                    error_message: bad.then(|| "decode error".into()),
                    error_code: None,
                }
            })
            .collect())
    }
    async fn transcript(&self, url: &str, _: &str) -> VoiceResult<Vec<BatchSegment>> {
        Ok(if url.contains("chunk-0000") {
            segs("Chalo shuru karte hain|Login fix Friday tak|Theek hai")
        } else {
            segs("Doosra hissa")
        })
    }
    async fn cancel(&self, _: &str) -> VoiceResult<()> {
        Ok(())
    }
}

#[derive(Default)]
struct Events(StdMutex<Vec<TranscriptionJob>>);
impl JobEvents for Events {
    fn job(&self, job: &TranscriptionJob) {
        self.0.lock().unwrap().push(job.clone());
    }
}
impl Events {
    fn statuses(&self) -> Vec<JobStatus> {
        self.0.lock().unwrap().iter().map(|j| j.status).collect()
    }
}

struct Rig {
    dir: tempfile::TempDir,
    db: Arc<Database>,
    jobs: Arc<MeetingJobs>,
    events: Arc<Events>,
}

fn rig(api: Arc<FakeGnani>) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(Database::open(&dir.path().join("pluto.db"), EventBus::default()).unwrap());
    let clock = Arc::new(FakeClock::new(START, Tz::Asia__Kolkata));
    let events = Arc::new(Events::default());
    let jobs = MeetingJobs::new(
        db.clone(),
        clock,
        api,
        dir.path().join("meetings"),
        events.clone(),
    )
    .with_schedule(PollSchedule {
        first: Duration::from_millis(1),
        step: Duration::ZERO,
        max: Duration::from_millis(1),
        give_up_after: Duration::from_secs(10),
    });
    Rig {
        dir,
        db,
        jobs: Arc::new(jobs),
        events,
    }
}

/// A short 16 kHz mono WAV (`secs` long).
fn wav_file(dir: &Path, name: &str, secs: u32) -> PathBuf {
    let path = dir.join(name);
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(&path, spec).unwrap();
    for _ in 0..16_000 * secs {
        w.write_sample(0i16).unwrap();
    }
    w.finalize().unwrap();
    path
}

/// Waits until the meeting's job is no longer running.
async fn settle(r: &Rig, meeting_id: &str) -> TranscriptionJob {
    for _ in 0..500 {
        let job = r.jobs.job(meeting_id).await.unwrap().unwrap();
        if matches!(
            job.status,
            JobStatus::Completed
                | JobStatus::PartialFailure
                | JobStatus::Failed
                | JobStatus::Cancelled
        ) && !lock(&r.jobs.running).contains(meeting_id)
        {
            return job;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the job never finished");
}

fn people() -> Vec<String> {
    vec!["Priya".into(), "Rahul".into(), "Asha".into()]
}

#[tokio::test(flavor = "multi_thread")]
async fn an_import_ends_with_speaker_labelled_segments_in_the_database() {
    let api = FakeGnani::ending("COMPLETED");
    let r = rig(api.clone());
    let src = wav_file(r.dir.path(), "standup.WAV", 2);

    let queued = r.jobs.import("Standup", &people(), &src).await.unwrap();
    assert_eq!(queued.status, JobStatus::Queued);
    let job = settle(&r, &queued.meeting_id).await;
    assert_eq!(job.status, JobStatus::Completed);
    assert_eq!(
        (job.gnani_job_id.as_deref(), job.error.as_deref()),
        (Some("g-1"), None)
    );
    assert_eq!(
        r.events.statuses(),
        [
            JobStatus::Queued,
            JobStatus::Uploading,
            JobStatus::Created,
            JobStatus::InProgress,
            JobStatus::Completed
        ]
    );

    // The meeting exists with its participants, and the audio was copied into the data folder.
    let m = meetings::get(&r.db, &queued.meeting_id).await.unwrap();
    assert_eq!(
        (m.title.as_str(), m.participants.clone()),
        ("Standup", people())
    );
    let mid = m.id.clone();
    let recs =
        r.db.read(move |c| store::list_recordings(c, &mid))
            .await
            .unwrap();
    assert_eq!(recs.len(), 1);
    assert_eq!(
        (recs[0].source.as_str(), recs[0].duration_ms),
        ("import", Some(2_000))
    );
    assert!(recs[0].file_path.ends_with("import-0000.wav"));
    assert!(Path::new(&recs[0].file_path).exists());

    // What went to Gnani: one file named for its chunk, the participants as speaker names.
    let uploads = api.uploads.lock().unwrap().clone();
    assert_eq!(uploads[0].0, ["chunk-0000.wav"]);
    assert_eq!(uploads[0].1.speaker_names, people());

    // Speaker-labelled segments, in order, and the meeting waits for the speaker mapping.
    let mid = m.id.clone();
    let rows =
        r.db.read(move |c| store::list_segments(c, &mid))
            .await
            .unwrap();
    let got: Vec<_> = rows
        .iter()
        .map(|s| (s.index, s.start_ms, s.speaker_label, s.text.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            (0, 0, Some(1), "Chalo shuru karte hain"),
            (1, 4_000, Some(2), "Login fix Friday tak"),
            (2, 8_000, Some(1), "Theek hai")
        ]
    );
    let mid = m.id.clone();
    let st =
        r.db.read(move |c| store::get_status(c, &mid))
            .await
            .unwrap()
            .unwrap();
    assert_eq!(
        (st.status.as_str(), st.dominant_language.as_deref()),
        ("needs_speakers", Some("hi-IN"))
    );
}

/// Two chunks as the recorder (T-207) will store them; the second fails (T-FH-06).
async fn two_chunk_meeting(r: &Rig) -> String {
    let m = meetings::create(
        &r.db,
        &FakeClock::new(START, Tz::Asia__Kolkata),
        MeetingInput {
            title: "Long sync".into(),
            participants: people(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    for i in 0..2u32 {
        let path = wav_file(r.dir.path(), &format!("c{i}.wav"), 1);
        let row = RecordingRow {
            id: new_id(),
            meeting_id: m.id.clone(),
            source: "mic".into(),
            chunk_index: i,
            offset_ms: i64::from(i) * 270_000,
            file_path: path.to_string_lossy().into_owned(),
            duration_ms: Some(270_000),
            consent_confirmed_at: Some(START),
            created_at: START,
        };
        r.db.write(move |tx| {
            store::insert_recording(tx, &row)?;
            Ok(((), vec![]))
        })
        .await
        .unwrap();
    }
    m.id
}

#[tokio::test(flavor = "multi_thread")]
async fn partial_failure_keeps_finished_parts_marks_the_gap_and_retries() {
    let api = FakeGnani::ending("PARTIAL_FAILURE");
    api.failed.lock().unwrap().push("chunk-0001.wav".into());
    let r = rig(api.clone());
    let mid = two_chunk_meeting(&r).await;

    r.jobs.queue(&mid).await.unwrap();
    let job = settle(&r, &mid).await;
    assert_eq!(job.status, JobStatus::PartialFailure);
    assert_eq!(
        job.error.as_deref(),
        Some("Gnani couldn't transcribe part 2 (04:30–09:00). Retry to try again.")
    );
    let m = mid.clone();
    let rows =
        r.db.read(move |c| store::list_segments(c, &m))
            .await
            .unwrap();
    assert_eq!(rows.len(), 3, "the first part is kept");

    // Retry: this time everything comes back, the second part lands at its offset.
    api.failed.lock().unwrap().clear();
    *api.end.lock().unwrap() = "COMPLETED".into();
    let again = r.jobs.retry(&mid).await.unwrap();
    assert_ne!(again.id, job.id, "a retry is a new job");
    let job = settle(&r, &mid).await;
    assert_eq!((job.status, job.error), (JobStatus::Completed, None));
    let m = mid.clone();
    let rows =
        r.db.read(move |c| store::list_segments(c, &m))
            .await
            .unwrap();
    assert_eq!(rows.len(), 4);
    assert_eq!(
        (rows[3].start_ms, rows[3].text.as_str()),
        (270_000, "Doosra hissa")
    );
    assert_eq!(
        api.uploads.lock().unwrap()[1].0,
        ["chunk-0000.wav", "chunk-0001.wav"]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_job_fails_the_meeting_with_a_plain_message() {
    let r = rig(FakeGnani::ending("FAILED"));
    let src = wav_file(r.dir.path(), "a.wav", 1);
    let queued = r.jobs.import("A", &[], &src).await.unwrap();
    let job = settle(&r, &queued.meeting_id).await;
    assert_eq!(job.status, JobStatus::Failed);
    assert!(job.error.unwrap().starts_with("Gnani couldn't transcribe"));
    let mid = queued.meeting_id.clone();
    let st =
        r.db.read(move |c| store::get_status(c, &mid))
            .await
            .unwrap()
            .unwrap();
    assert_eq!(st.status, "failed");
}

#[tokio::test(flavor = "multi_thread")]
async fn imports_are_checked_before_anything_is_created() {
    let r = rig(FakeGnani::ending("COMPLETED"));
    let txt = r.dir.path().join("notes.txt");
    std::fs::write(&txt, b"hello").unwrap();
    let err = r.jobs.import("X", &[], &txt).await.unwrap_err();
    assert_eq!(err.code, VoiceErrorCode::UnsupportedAudio);

    let empty = r.dir.path().join("empty.mp3");
    std::fs::write(&empty, b"").unwrap();
    assert_eq!(
        r.jobs.import("X", &[], &empty).await.unwrap_err().code,
        VoiceErrorCode::UnsupportedAudio
    );

    let big = r.dir.path().join("big.mp3");
    let f = std::fs::File::create(&big).unwrap();
    f.set_len(MAX_FILE_BYTES + 1).unwrap();
    assert_eq!(
        r.jobs.import("X", &[], &big).await.unwrap_err().code,
        VoiceErrorCode::FileTooLarge
    );

    let missing = r.dir.path().join("gone.wav");
    assert_eq!(
        r.jobs.import("X", &[], &missing).await.unwrap_err().code,
        VoiceErrorCode::Core
    );

    let list = pluto_core::meetings::list(&r.db, Default::default())
        .await
        .unwrap();
    assert!(list.is_empty(), "no meeting is created for a refused file");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_job_for_the_same_meeting_is_refused_while_one_runs() {
    let r = rig(FakeGnani::ending("COMPLETED"));
    let mid = two_chunk_meeting(&r).await;
    assert!(r.jobs.claim(&mid), "simulate a running job");
    assert_eq!(
        r.jobs.retry(&mid).await.unwrap_err().code,
        VoiceErrorCode::Busy
    );
    r.jobs.release(&mid);

    // A meeting without audio can't be transcribed.
    let m = meetings::create(
        &r.db,
        &FakeClock::new(START, Tz::Asia__Kolkata),
        MeetingInput {
            title: "Empty".into(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(r.jobs.retry(&m.id).await.is_err());
    assert!(r.jobs.job(&m.id).await.unwrap().is_none());
    assert!(!lock(&r.jobs.running).contains(&m.id));
}
