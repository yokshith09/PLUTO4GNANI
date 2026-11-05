//! Contract tests for the Batch client against `wiremock` (job lifecycle, files, transcripts,
//! errors) and unit tests for parsing, assembling and polling. No Gnani key or network needed.

use std::sync::Mutex;

use serde_json::json;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use super::*;
use crate::http::{RetryPolicy, StaticKey, API_KEY_HEADER};

#[derive(Default)]
struct MemoryLog(Mutex<Vec<OutboundRecord>>);
impl OutboundLog for MemoryLog {
    fn record(&self, record: OutboundRecord) {
        self.0.lock().unwrap().push(record);
    }
}

fn client(uri: &str) -> (BatchClient, Arc<MemoryLog>) {
    let log = Arc::new(MemoryLog::default());
    let http = GnaniHttp::new(uri, Arc::new(StaticKey(Some("k-1".into()))), log.clone())
        .unwrap()
        .with_policy(RetryPolicy {
            delays: vec![Duration::from_millis(5); 3],
            max_retry_after: Duration::from_secs(1),
        });
    (BatchClient::new(Arc::new(http), log.clone()).unwrap(), log)
}

fn wav(name: &str, len: usize) -> BatchFile {
    BatchFile {
        name: name.into(),
        bytes: vec![7; len],
    }
}

fn names(n: usize) -> Vec<String> {
    (1..=n).map(|i| format!("P{i}")).collect()
}

// ---- config --------------------------------------------------------------------------------

#[test]
fn config_follows_gnani_limits() {
    let c = BatchConfig::for_meeting(&names(3), &[], None);
    assert_eq!(c.speaker_names, names(3));
    assert_eq!(c.num_speakers, None);
    assert!(c.with_diarization);
    assert_eq!(
        (
            c.model.as_str(),
            c.language_code.as_str(),
            c.format.as_str()
        ),
        ("prisma-v3.0", "indic-auto", "codemix")
    );

    let c = BatchConfig::for_meeting(&names(6), &[], None);
    assert_eq!((c.num_speakers, c.speaker_names.len()), (Some(6), 0));
    let c = BatchConfig::for_meeting(&names(12), &[], None);
    assert_eq!(c.num_speakers, Some(8));

    let terms: Vec<String> = (0..250).map(|i| format!("t{i}")).collect();
    let c = BatchConfig::for_meeting(&[], &terms, Some("too short"));
    assert_eq!((c.keyterms.len(), c.topic.clone()), (200, None));
    let topic = "Weekly project standup for the app team";
    let c = BatchConfig::for_meeting(&[], &[], Some(topic));
    assert_eq!(c.topic.as_deref(), Some(topic));

    // Empty fields are left out of the JSON, so Gnani's defaults apply.
    let v = serde_json::to_value(BatchConfig::for_meeting(&[], &[], None)).unwrap();
    assert!(v.get("speaker_names").is_none() && v.get("keyterms").is_none());
    assert!(v.get("num_speakers").is_none() && v.get("topic").is_none());
}

#[test]
fn files_are_checked_before_upload() {
    assert_eq!(
        check_files(&[]).unwrap_err().code,
        VoiceErrorCode::UnsupportedAudio
    );
    assert_eq!(
        check_files(&[wav("a.wav", MAX_FILE_BYTES as usize + 1)])
            .unwrap_err()
            .code,
        VoiceErrorCode::FileTooLarge
    );
    assert_eq!(
        check_files(&[wav("a.txt", 4)]).unwrap_err().code,
        VoiceErrorCode::UnsupportedAudio
    );
    assert!(check_files(&[wav("A.WAV", 4), wav("b.m4a", 4)]).is_ok());
}

// ---- lifecycle against wiremock ------------------------------------------------------------

#[tokio::test]
async fn create_uploads_config_and_files_and_returns_the_job_id() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(JOBS_PATH))
        .and(header(API_KEY_HEADER, "k-1"))
        .respond_with(
            ResponseTemplate::new(201).set_body_json(json!({"job_id":"g-1","status":"CREATED"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let (c, log) = client(&server.uri());
    let cfg = BatchConfig::for_meeting(&names(2), &["DBMS".into()], None);
    let id = c
        .create(
            &[wav("chunk-0000.wav", 10), wav("chunk-0001.wav", 6)],
            &cfg,
            "m1",
        )
        .await
        .unwrap();
    assert_eq!(id, "g-1");

    let req: &Request = &server.received_requests().await.unwrap()[0];
    let body = String::from_utf8_lossy(&req.body);
    assert!(body.contains("name=\"config\""));
    assert!(body.contains(r#""speaker_names":["P1","P2"]"#));
    assert!(body.contains(r#""keyterms":["DBMS"]"#));
    assert_eq!(body.matches("name=\"files\"").count(), 2);
    assert!(body.contains("filename=\"chunk-0001.wav\""));

    let records = log.0.lock().unwrap().clone();
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].destination,
        pluto_contracts::voice::Destination::GnaniBatch
    );
    assert_eq!(
        (records[0].bytes, records[0].ref_id.as_deref()),
        (16, Some("m1"))
    );
}

#[tokio::test]
async fn start_status_files_and_cancel_hit_the_documented_paths() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/stt/v3/batch/jobs/g-1/start"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status":"STARTING"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/stt/v3/batch/jobs/g-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "job_id":"g-1","status":"PARTIAL_FAILURE",
            "progress":{"total_files":2,"completed_files":1,"failed_files":1},
            "cancel_reason":null
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/stt/v3/batch/jobs/g-1/files"))
        .and(query_param("limit", "100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "job_id":"g-1",
            "data":[
                {"file_id":"f1","original_path":"chunk-0000.wav","status":"COMPLETED",
                 "transcript_url":"https://store.example/t1.json","duration_seconds":"270.0"},
                {"file_id":"f2","original_path":"chunk-0001.wav","status":"FAILED",
                 "transcript_url":null,"error_message":"decode error","error_code":"BAD_AUDIO"}
            ],
            "pagination":{"has_more":false,"next_cursor":null,"total_count":2}
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/stt/v3/batch/jobs/g-1/cancel"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status":"CANCELLING"})))
        .expect(1)
        .mount(&server)
        .await;

    let (c, _) = client(&server.uri());
    assert_eq!(c.start("g-1").await.unwrap(), BatchStatus::Starting);
    let s = c.status("g-1").await.unwrap();
    assert_eq!(s.status, BatchStatus::PartialFailure);
    assert_eq!(
        (s.total_files, s.completed_files, s.failed_files),
        (2, 1, 1)
    );
    let files = c.files("g-1").await.unwrap();
    assert_eq!(files.len(), 2);
    assert!(files[0].is_completed());
    assert!(!files[1].is_completed());
    assert_eq!(files[1].error_message.as_deref(), Some("decode error"));
    c.cancel("g-1").await.unwrap();
}

#[tokio::test]
async fn files_follow_the_cursor() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/stt/v3/batch/jobs/g-1/files"))
        .and(query_param("cursor", "c2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data":[{"original_path":"b.wav","status":"COMPLETED","transcript_url":"https://x/b"}],
            "pagination":{"has_more":false,"next_cursor":null}
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/stt/v3/batch/jobs/g-1/files"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data":[{"original_path":"a.wav","status":"COMPLETED","transcript_url":"https://x/a"}],
            "pagination":{"has_more":true,"next_cursor":"c2"}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let (c, _) = client(&server.uri());
    let files = c.files("g-1").await.unwrap();
    let paths: Vec<_> = files.iter().map(|f| f.original_path.as_str()).collect();
    assert_eq!(paths, ["a.wav", "b.wav"]);
}

#[tokio::test]
async fn the_transcript_link_is_fetched_without_the_api_key() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/t1.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "full_transcript":"…","duration_seconds":12.5,
            "segments":[
                {"segment_id":1,"speaker_id":2,"start_time":5.0,"end_time":9.25,
                 "text":"Friday tak login fix","language_detected":"hi-IN"},
                {"segment_id":0,"speaker_id":"SPEAKER_1","start_time":0.0,"end_time":4.5,
                 "text":"Chalo shuru karte hain","language_detected":"hi-IN"},
                {"segment_id":2,"speaker_id":1,"start_time":10.0,"end_time":10.0,"text":"  "}
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let (c, log) = client(&server.uri());
    let segs = c
        .transcript(&format!("{}/t1.json", server.uri()), "m1")
        .await
        .unwrap();
    assert_eq!(segs.len(), 2, "blank segments are dropped");
    assert_eq!(
        (segs[0].start_ms, segs[0].end_ms, segs[0].speaker),
        (0, 4500, Some(1))
    );
    assert_eq!(
        (segs[1].start_ms, segs[1].end_ms, segs[1].speaker),
        (5000, 9250, Some(2))
    );
    assert_eq!(segs[1].language.as_deref(), Some("hi-IN"));

    let req = &server.received_requests().await.unwrap()[0];
    assert!(
        !req.headers.contains_key(API_KEY_HEADER),
        "the key must never go to the pre-signed link"
    );
    assert_eq!(log.0.lock().unwrap()[0].purpose, "transcript download");
}

#[tokio::test]
async fn an_expired_transcript_link_says_so() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;
    let (c, _) = client(&server.uri());
    let err = c
        .transcript(&format!("{}/t.json", server.uri()), "m1")
        .await
        .unwrap_err();
    assert_eq!(err.code, VoiceErrorCode::GnaniUpstream);
    assert!(err.message.contains("expired"));
    assert!(c.transcript("file:///etc/passwd", "m1").await.is_err());
}

#[tokio::test]
async fn batch_errors_map_to_contract_codes() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/stt/v3/batch/jobs/nope"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(json!({"error":"JOB_NOT_FOUND","message":"Job nope not found"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(JOBS_PATH))
        .respond_with(ResponseTemplate::new(402))
        .mount(&server)
        .await;
    let (c, _) = client(&server.uri());
    let err = c.status("nope").await.unwrap_err();
    assert_eq!(err.code, VoiceErrorCode::BatchJobFailed);
    assert!(
        !err.message.contains("nope"),
        "Gnani's text is never copied"
    );
    let err = c
        .create(
            &[wav("a.wav", 3)],
            &BatchConfig::for_meeting(&[], &[], None),
            "m",
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, VoiceErrorCode::InsufficientCredit);
}

#[tokio::test]
async fn odd_job_ids_are_encoded_into_one_path_segment() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/stt/v3/batch/jobs/a%2F..%2Fb"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status":"QUEUED"})))
        .expect(1)
        .mount(&server)
        .await;
    let (c, _) = client(&server.uri());
    assert_eq!(
        c.status("a/../b").await.unwrap().status,
        BatchStatus::Queued
    );
}

// ---- parsing, assembling, polling ----------------------------------------------------------

#[test]
fn statuses_parse_and_classify() {
    for (s, terminal, results) in [
        ("CREATED", false, false),
        ("in_progress", false, false),
        ("COMPLETED", true, true),
        ("PARTIAL_FAILURE", true, true),
        ("FAILED", true, false),
        ("START_FAILED", true, false),
        ("CANCELLING", false, false),
        ("CANCELLED", true, false),
        ("SOMETHING_NEW", false, false),
    ] {
        let st = BatchStatus::parse(s);
        assert_eq!(
            (st.is_terminal(), st.has_results()),
            (terminal, results),
            "{s}"
        );
    }
}

#[test]
fn an_unreadable_transcript_is_an_error() {
    assert!(parse_transcript(&json!({"nothing":1})).is_err());
    assert!(parse_transcript(&json!({"segments":[]}))
        .unwrap()
        .is_empty());
}

fn seg(start: i64, end: i64, speaker: u8, text: &str) -> BatchSegment {
    BatchSegment {
        start_ms: start,
        end_ms: end,
        speaker: Some(speaker),
        text: text.into(),
        language: Some("hi-IN".into()),
    }
}

#[test]
fn chunks_are_placed_on_the_meeting_timeline_with_gaps() {
    let chunk = |i: u32, off: i64| Chunk {
        index: i,
        name: format!("chunk-{i:04}.wav"),
        offset_ms: off,
        duration_ms: Some(270_000),
    };
    let chunks = [chunk(0, 0), chunk(1, 270_000), chunk(2, 540_000)];
    let outcomes = vec![
        (
            "chunk-0002.wav".to_string(),
            ChunkOutcome::Transcribed(vec![seg(1_000, 2_000, 1, "teen")]),
        ),
        (
            "chunk-0000.wav".to_string(),
            ChunkOutcome::Transcribed(vec![seg(500, 1_500, 2, "ek"), seg(0, 400, 1, "zero")]),
        ),
        (
            "chunk-0001.wav".to_string(),
            ChunkOutcome::Failed("decode error".into()),
        ),
    ];
    let a = assemble(&chunks, &outcomes);
    let texts: Vec<_> = a.segments.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(texts, ["zero", "ek", "teen"]);
    assert_eq!(
        (a.segments[2].start_ms, a.segments[2].end_ms),
        (541_000, 542_000)
    );
    assert_eq!(
        a.gaps,
        vec![Gap {
            chunk_index: 1,
            start_ms: 270_000,
            end_ms: Some(540_000),
            reason: "decode error".into()
        }]
    );
    assert_eq!(
        gaps_message(&a.gaps).unwrap(),
        "Gnani couldn't transcribe part 2 (04:30–09:00). Retry to try again."
    );
    assert_eq!(a.dominant_language().as_deref(), Some("hi-IN"));

    // A chunk Gnani didn't list at all is a gap too.
    let a = assemble(&chunks[..1], &[]);
    assert_eq!(a.gaps.len(), 1);
    assert!(gaps_message(&[]).is_none());
}

#[test]
fn clock_text_formats() {
    assert_eq!(clock_text(0), "00:00");
    assert_eq!(clock_text(270_000), "04:30");
    assert_eq!(clock_text(3_723_000), "1:02:03");
}

#[test]
fn poll_delay_backs_off_to_the_maximum() {
    let p = PollSchedule::default();
    assert_eq!(p.delay(0), Duration::from_secs(10));
    assert_eq!(p.delay(1), Duration::from_millis(12_500));
    assert_eq!(p.delay(2), Duration::from_secs(15));
    assert_eq!(p.delay(50), Duration::from_secs(15));
}

/// Answers `status` from a script; everything else is unused here.
struct Scripted(Mutex<Vec<VoiceResult<JobState>>>);

#[async_trait]
impl BatchApi for Scripted {
    async fn create(&self, _: &[BatchFile], _: &BatchConfig, _: &str) -> VoiceResult<String> {
        unreachable!()
    }
    async fn start(&self, _: &str) -> VoiceResult<BatchStatus> {
        unreachable!()
    }
    async fn status(&self, _: &str) -> VoiceResult<JobState> {
        self.0.lock().unwrap().remove(0)
    }
    async fn files(&self, _: &str) -> VoiceResult<Vec<FileResult>> {
        unreachable!()
    }
    async fn transcript(&self, _: &str, _: &str) -> VoiceResult<Vec<BatchSegment>> {
        unreachable!()
    }
    async fn cancel(&self, _: &str) -> VoiceResult<()> {
        unreachable!()
    }
}

fn state(s: &str) -> VoiceResult<JobState> {
    Ok(JobState {
        status: BatchStatus::parse(s),
        total_files: 1,
        completed_files: 0,
        failed_files: 0,
        cancel_reason: None,
    })
}

#[tokio::test(start_paused = true)]
async fn wait_for_polls_through_blips_until_terminal() {
    let api = Scripted(Mutex::new(vec![
        state("QUEUED"),
        Err(VoiceError::new(VoiceErrorCode::NetworkOffline, "x")),
        state("IN_PROGRESS"),
        state("COMPLETED"),
    ]));
    let mut seen = vec![];
    let t0 = tokio::time::Instant::now();
    let s = wait_for(&api, "g", &PollSchedule::default(), |s| {
        seen.push(s.status.clone())
    })
    .await
    .unwrap();
    assert_eq!(s.status, BatchStatus::Completed);
    assert_eq!(
        seen,
        [
            BatchStatus::Queued,
            BatchStatus::InProgress,
            BatchStatus::Completed
        ]
    );
    // 10 + 12.5 + 15 + 15 s: never faster than Gnani's 10 s guidance.
    assert_eq!(t0.elapsed(), Duration::from_millis(52_500));
}

#[tokio::test(start_paused = true)]
async fn wait_for_gives_up_after_the_deadline_and_stops_on_hard_errors() {
    let schedule = PollSchedule {
        give_up_after: Duration::from_secs(20),
        ..PollSchedule::default()
    };
    let api = Scripted(Mutex::new(vec![
        state("QUEUED"),
        state("QUEUED"),
        state("QUEUED"),
    ]));
    let err = wait_for(&api, "g", &schedule, |_| {}).await.unwrap_err();
    assert_eq!(err.code, VoiceErrorCode::BatchJobFailed);
    assert!(err.retryable);

    let api = Scripted(Mutex::new(vec![Err(VoiceError::new(
        VoiceErrorCode::InvalidApiKey,
        "bad key",
    ))]));
    let err = wait_for(&api, "g", &PollSchedule::default(), |_| {})
        .await
        .unwrap_err();
    assert_eq!(err.code, VoiceErrorCode::InvalidApiKey);
}
