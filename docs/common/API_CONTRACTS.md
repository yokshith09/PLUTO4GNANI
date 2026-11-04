# API Contracts — Pluto Voice

**Single authority** for:
- external APIs (§2);
- shared types (§3);
- IPC commands (§4);
- events and channels (§5);
- errors (§6).

Other documents link here and never redefine these.

**Contract version:** `1` (`CONTRACT_VERSION` in `crates/pluto-contracts/src/lib.rs`). Changes follow `WORKING_AGREEMENT.md` §5.

## 0. Conventions (apply to every type, command, event and fixture)
| Topic | Rule |
|---|---|
| Where types live | **Existing (personal, meetings, errors, events):** `crates/pluto-core`, already exported to `src/ipc/generated/` — **not redefined here**. **New:** `crates/pluto-contracts/src/`: `voice.rs`, `speech.rs`, `error.rs` (D1); `meeting.rs`, `llm.rs`, `decide.rs` (D2). Exported with `ts-rs` to `src/ipc/generated/` (set by `.cargo/config.toml`) |
| Field names | `snake_case` in Rust, JSON and TypeScript |
| Enums on the wire | **Existing enums keep the base's spelling:** `SCREAMING_SNAKE_CASE` (`"IN_PROGRESS"`). **New enums are `snake_case`** (`#[serde(rename_all = "snake_case")]`, e.g. `"action_item"`). The database stores exactly the wire spelling, so there is no conversion layer |
| IDs | Strings generated in Rust by `pluto_core::ids::new_id()` (UUID v7). The UI never creates IDs |
| Timestamps | `i64` milliseconds since the Unix epoch, UTC |
| Dates | `"YYYY-MM-DD"` in the user's local timezone. Computed in Rust, never in the UI |
| Gnani time values | Gnani returns **seconds** (`start_time`, `end_time`). Our types use **milliseconds**. The conversion happens once, in `voice::gnani`, and nowhere else |
| Optional vs null | A missing optional value is JSON `null`, never an empty string or `0`. Patch fields that can be cleared use `Option<Option<T>>` (outer = "not provided", inner = "set to null") |
| Language codes | `hi-IN`, `en-IN`, `ta-IN`, `te-IN`, `kn-IN`, `ml-IN`, `mr-IN`, `pa-IN`, `bn-IN`, `gu-IN`. `hi-en` and `auto` are TTS-only. `detected_language_code` may be absent; treat it as unknown |
| Speakers | `speaker_label` stores Gnani's `speaker_id` exactly as returned (the docs' example starts at 1). The UI shows "Speaker N". `participant` is the mapped name or null |
| Segment order | `index` is our own 0-based order across all chunks of a meeting, after chunk offsets are applied. Gnani's `segment_id` is not stored |
| Audio formats | **Mic → Rust:** PCM16 little-endian, mono, 16 kHz, frames of exactly 1,024 bytes. **Rust → speaker:** PCM16 little-endian, mono, 24 kHz. These never change without a contract PR |
| Natural-language dates | Resolved in `intel` before they leave it. `AssistantDecision.args` therefore carries resolved fields (`due_date` as `YYYY-MM-DD`, `remind_at` as ms) alongside the original phrase (`due_phrase`, `when_phrase`) for display. `voice` and `core` accept only resolved values |
| Errors | **Existing commands** return core's `AppError {code, message, field}`. **New voice and meeting commands** return `VoiceError` (§6) because core's `ErrorCode` has only 7 fixed values. Crates use their own error types internally and convert at the command boundary |
| Fixtures | One shared set in `test-data/fixtures/` (the UI mock in `src/ipc/` imports them). Each file is named `<type>.<case>.json` and includes `"contract_version": 1`. The UI mocks and the Rust tests use the same files |

---

## 1. Authentication
| Service | Auth | Key storage (keychain account) |
|---|---|---|
| Gnani (`https://api.vachana.ai`) | Header `X-API-Key-ID` (Realtime docs spell it `x-api-key-id`) **[Verified]** | `gnani_api_key` |
| Gemini API | API key | `gemini_api_key` |
| Groq API | Bearer key | `groq_api_key` |

Keychain service name: `pluto-voice`.

---

## 2. External APIs

### 2.1 Gnani Prisma Realtime: `wss://api.vachana.ai/stt/v3/stream` [Verified, docs 2 Nov 2026]

**Handshake headers we send:**

| Header | Value |
|---|---|
| `x-api-key-id` | API key |
| `model` and `x-model` | `prisma-v3.0` (send both; the docs use both spellings) |
| `language_code` | `indic-auto`, or 1–4 comma-separated codes |
| `format` | `codemix` or `transliterate` |
| `x-sample-rate` | `16000` |
| `keyterms` | JSON array, non-ASCII escaped |
| `x-idle-timeout-sec` | 5–900 |
| Optional | `x-min-silence-ms` |

**Client frames:**
- Binary PCM16 LE mono, 1,024 bytes.
- `{"type":"commit"}` forces the current segment to close.

**Server messages consumed:**

| `type` | Fields used |
|---|---|
| `connected` | `request_id` |
| `speech_start` | — |
| `processing` | — |
| `transcript` | `text`, `segment_id`, `segment_index`, `detected_language_code`, `audio_duration_ms`, `latency` (always final) |
| `error` | `message` |

**Limits:** 15-min session (close code 1001); 60 s idle timeout by default; per-organisation session cap (number not published).

### 2.2 Gnani Prisma REST: `POST /stt/v3` (multipart)
- **Fields:** `audio_file`, `model=prisma-v3.0`, `language_code`, `format`, `keyterms`, `topic`, `speaker_names`, `previous_text`.
- **Response:** `{success, request_id, transcript, model, detected_language_code, processing_time, end_to_end_latency}`.
- **Limits:** clips ≤ 30 s. "Only 1 active request is allowed at a time" (429).

### 2.3 Gnani Prisma Batch
| Operation | Method and path |
|---|---|
| Create (multipart: `files`, `source`, `config`) | `POST /stt/v3/batch/jobs` |
| Start | `POST /stt/v3/batch/jobs/{job_id}/start` |
| Status | `GET /stt/v3/batch/jobs/{job_id}` |
| Files (`transcript_url` per file) | `GET /stt/v3/batch/jobs/{job_id}/files?status=COMPLETED` |
| Cancel | `POST /stt/v3/batch/jobs/{job_id}/cancel` |
| Presigned upload | Unresolved (G-4). **Not used** |

**`config`:**
```json
{ "model":"prisma-v3.0", "language_code":"indic-auto", "format":"codemix", "with_diarization":true,
  "num_speakers":3, "speaker_names":["Priya","Rahul","Asha"], "keyterms":["login fix","DBMS"],
  "topic":"Weekly project standup for the app team" }
```
**Status values:** `CREATED`, `STARTING`, `QUEUED`, `IN_PROGRESS`, `COMPLETED`, `PARTIAL_FAILURE`, `FAILED`, `START_FAILED`, `CANCELLING`, `CANCELLED`.

**Segment (inside the JSON at `transcript_url`):**
`{segment_id, start_time, end_time, text, speaker_id, confidence, language_detected}` (times in seconds).

**Limits:** 10 MB per direct upload; ≤100 files per job; 4 h per file; `num_speakers` 1–8; `speaker_names` ≤4; `keyterms` ≤200; `topic` 30–1,600 characters.

### 2.4 Gnani Timbre
**REST:** `POST /api/v1/tts/inference` (JSON) returns binary audio.
```json
{ "text":"…", "voice":"Shivani", "model":"timbre-v3.0", "language":"hi-IN", "speed":1.0,
  "audio_config":{ "sample_rate":24000, "num_channels":1, "sample_width":2, "encoding":"linear_pcm", "container":"mp3", "bitrate":"128k" } }
```
- `language`: `auto`, `hi-IN`, `en-IN`, `ta-IN`, `te-IN`, `kn-IN`, `ml-IN`, `mr-IN`, `pa-IN`, `bn-IN`, `gu-IN`, `hi-en`.
- `speed`: 0.85–1.15.

**WebSocket:** `wss://api.vachana.ai/api/v1/tts`.
- Send the same JSON body with `container:"raw"`.
- The server sends `start` → `audio {data:{chunk_index, audio(base64), is_final}}` → `complete`, or `error`.
- Never use `ogg`: it arrives as a single chunk at the end.

**Audio tag allow-list (v3.0):** `[laughs] [giggles] [sigh] [exhale] [cheer] [whisper] [silence]`. Tags are billable characters.

### 2.5 Gnani voice cloning (P2)
- **Embeddings:** `POST /api/v1/tts/voice-clone/embeddings` (multipart `audio_file`, 5–30 s) → `data.voice_clone_embedding {embedding, shape, dtype}`.
- **Synthesis:** `POST /api/v1/tts/inference` with `model:"vachana-vc-v1"`, `speaker_embedding`, `text`, `audio_config`.

### 2.6 LLM providers
These are adapters. **Confirm the exact request shape against each provider's current docs on Day 1 (T-LLM-01)**.

| Provider | Endpoint | Structured output |
|---|---|---|
| Groq **[Likely]** | OpenAI-compatible `POST https://api.groq.com/openai/v1/chat/completions` | `response_format` JSON (support varies by model) |
| Gemini **[Likely]** | `POST https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent` | `generationConfig.responseMimeType: application/json` + `responseSchema` |

**Internal adapter contract (authoritative):**
```rust
enum LlmTask { Intent, Relevance, Extract, RecallAnswer, Recap }
struct LlmRequest { task: LlmTask, system: String, messages: Vec<LlmMessage>, json_schema: serde_json::Value, timeout_ms: u32 }
struct LlmResponse { json: serde_json::Value, provider: LlmProvider, model: String, latency_ms: u32, fell_back: bool }
enum LlmProvider { Gemini, Groq, Mock }
```

---

## 3. Shared types

### 3.1 Personal domain — **exists in the base; not redefined**
Tasks, reminders, notes, focus sessions, meetings (participants, decisions, action items) and their inputs are Rust types in `crates/pluto-core` and generated TypeScript in `src/ipc/generated/`. Notable spellings the new code must use:
- `TaskStatus`: `PLANNED`, `IN_PROGRESS`, `PENDING`, `COMPLETED`, `CANCELLED` (**"complete" a task = transition to `COMPLETED`**).
- `Task.project` is free text (a project is the set of tasks sharing it).
- `Meeting` has `participants`, `decisions`, `action_items` (`assignee`, `task_id`), `follow_up_date`, `notes`, `transcript`.

### 3.2 Voice runtime (`crates/pluto-contracts/src/voice.rs`)
```rust
enum VoiceState { Disabled, Idle, Listening, Thinking, AwaitingApproval, Executing, Speaking, Recording, Error }
enum VoiceMode { Assistant, Meeting }
struct VoiceStatus { state: VoiceState, mode: Option<VoiceMode>, detail: Option<String>, session_id: Option<String> }
enum SttFormat { Codemix, Transliterate, Verbatim, Transcribe }
struct VoiceSettings { cloud_voice_enabled: bool, languages: Vec<String>, stt_format: SttFormat,
  tts_voice: String, tts_model: String, hotkey: String, audio_tags: bool, always_on_enabled: bool,
  has_gnani_key: bool, has_gemini_key: bool, has_groq_key: bool }
// Argument of voice_settings_set. Every field optional; a missing field is unchanged.
// Keys are set with voice_key_set, so the has_*_key flags are not patchable.
struct VoiceSettingsPatch { cloud_voice_enabled: Option<bool>, languages: Option<Vec<String>>, stt_format: Option<SttFormat>,
  tts_voice: Option<String>, tts_model: Option<String>, hotkey: Option<String>, audio_tags: Option<bool>, always_on_enabled: Option<bool> }
```

### 3.3 Transcript and meetings (`crates/pluto-contracts/src/meeting.rs`)
```rust
struct MeetingAiStatus { meeting_id: String, status: AiStatus, dominant_language: Option<String>, updated_at: i64 }   // table meeting_ai
enum AiStatus { Recording, Transcribing, NeedsSpeakers, NeedsReview, Done, Failed }

struct TranscriptSegment { id: String, meeting_id: Option<String>, source: SegmentSource /* Realtime | Batch | Rest */,
  index: u32, start_ms: Option<i64>, end_ms: Option<i64>, speaker_label: Option<u8>, participant: Option<String>,
  text: String, language: Option<String>, relevance: Option<Relevance> }
enum Relevance { Decision, ActionItem, Question, Risk, Chitchat }

struct RecordingSession { id: String, meeting_id: String, started_at: i64, live_captions: bool, chunk_count: u32 }
struct TranscriptionJob { id: String, meeting_id: String, gnani_job_id: Option<String>, status: JobStatus, error: Option<String>, updated_at: i64 }
enum JobStatus { Queued, Uploading, Created, InProgress, Completed, PartialFailure, Failed, Cancelled }
struct SpeakerMapping { speaker_label: u8, participant: Option<String> }

struct ExtractedItem { id: String, kind: Relevance, body: String, owner: Option<String>, due_phrase: Option<String>,
  due_date: Option<String>, priority: Option<Priority>, source_segment_id: String, quote: String,
  duplicate_of_task_id: Option<String> }
enum QualityWarning { NoDecisions, ActionItemWithoutOwner { item_id: String }, UnresolvedDueDate { item_id: String } }
struct ExtractionResult { meeting_id: String, items: Vec<ExtractedItem>, warnings: Vec<QualityWarning>, dominant_language: Option<String> }
struct EditedItem { item_id: String, body: String, owner: Option<String>, due_date: Option<String>, priority: Option<Priority>, create_task: bool }
struct ApplyExtraction { meeting_id: String, accepted: Vec<EditedItem> }

struct Citation { meeting_id: String, meeting_title: String, segment_id: String, speaker: Option<String>, quote: String, starts_at: i64 }
struct RecallAnswer { answer_text: String, language: String, citations: Vec<Citation>, proposal: Option<Proposal> }
struct Commitment { action_item_id: String, meeting_id: String, meeting_title: String, owner: Option<String>, body: String,
  due_date: Option<String>, task_id: Option<String>, status: CommitmentStatus, days_overdue: Option<i64> }
enum CommitmentStatus { Open, Done, Overdue, NoTask }
```

### 3.4 Decisions (`crates/pluto-contracts/src/decide.rs`)
```rust
enum Intent { MeetingQuery, Task, Reminder, Note, Focus, Project, Approval, Reject, Cancel, Noise }
enum DecisionSource { Rules, Llm }
struct AssistantDecision { intent: Intent, tool: Option<String>, args: serde_json::Value, confidence: Option<f32>, source: DecisionSource, latency_ms: u32 }
```

### 3.5 Tools and proposals
Each tool calls an existing `pluto-core` function. **W** = write (needs an approved proposal).

| Tool | W | Calls | Priority |
|---|---|---|---|
| `tasks_list` | | `tasks::query` | P0 |
| `task_create` | W | `tasks::create` / `tasks::quick_add` | P0 |
| `task_complete` | W | `tasks::transition` → `COMPLETED` | P0 |
| `task_update` | W | `tasks::update` (title, due date), `tasks::defer` | P0 |
| `reminders_list` | | `reminders::list` | P0 |
| `reminder_create` | W | `reminders::create` | P0 |
| `reminder_done` | W | `reminders::set_done` | P0 |
| `meeting_search` | | `pluto-intel` recall (FTS over `transcript_segments`) | P0 |
| `commitments_list` | | `pluto-intel` | P1 |
| `notes_search` | | `notes::search` | P1 |
| `note_create` | W | `notes::create` | P1 |
| `focus_start` | W | `focus::start` | P1 |
| `focus_stop` | W | `focus::stop` | P1 |
| `projects_list` | | `tasks::query` grouped by `project` | P2 |

```rust
struct Proposal { id: String, interaction_id: String, tool: String, args: serde_json::Value, summary_text: String, language: String, expires_at: i64 }
// Tagged by "kind": {"kind":"approve"} | {"kind":"reject"} | {"kind":"edit","args":{…}}
enum ProposalResponse { Approve, Reject, Edit { args: serde_json::Value } }
struct ToolResult { tool: String, ok: bool, message: String, record_id: Option<String> }
struct AssistantTurn { interaction_id: String, transcript: String, language: Option<String>, decision: AssistantDecision,
  reply_text: Option<String>, proposal: Option<Proposal>, result: Option<ToolResult> }   // in voice.rs since T-007; emitted as voice://turn
// Rust only: what a caller asks ToolExecutor::propose to create.
struct ProposalRequest { interaction_id: Option<String> /* None → a new interaction is logged */, tool: String,
  args: serde_json::Value, summary_text: String, language: String }
```
Tool names are constants in `pluto_contracts::voice::tools`, with `tools::WRITE` and `tools::READ` listing the two groups.

### 3.6 Privacy and usage
```rust
enum Destination { GnaniRealtime, GnaniRest, GnaniBatch, GnaniTts, GnaniClone, Gemini, Groq }
struct PrivacyLogEntry { id: i64, at: i64, destination: Destination, purpose: String, bytes: i64, ref_id: Option<String> }
struct UsageCounters { stt_seconds_today: f64, batch_seconds_today: f64, tts_chars_today: i64, gemini_calls_today: i64, groq_calls_today: i64 }
```

### 3.7 Cross-crate traits (`crates/pluto-contracts`)
These are the only way `voice` and `intel` talk to each other. `app` creates the implementations and passes them in as `Arc<dyn …>`.

| Trait | File (writer) | Implemented in | Used by |
|---|---|---|---|
| `Llm` | `llm.rs` (D2) | `pluto-intel::llm` (`GeminiClient`, `GroqClient`, `Router`, `MockLlm`) | `pluto-intel` (meeting, decide) |
| `Decider` | `decide.rs` (D2) | `pluto-intel::decide` (rules → LLM) | `pluto-voice::assistant` |
| `SpeechToText` | `speech.rs` (D1) | `pluto-gnani::realtime` (`MockStt` for tests) | `pluto-voice::runtime` |
| `TextToSpeech` | `speech.rs` (D1) | `pluto-gnani::tts` (`MockTts` for tests) | `pluto-voice::assistant`, `pluto-intel::meeting` (recap) |
| `FrameSink` | `speech.rs` (D1) | `pluto-voice` (WAV writer) and `pluto-intel::meeting::recorder` | `src/audio` frames arriving through `audio_push` |
| `ToolExecutor` | `voice.rs` (D1) | `pluto-voice::assistant` | `pluto-intel` (recall follow-up proposals) |

```rust
#[async_trait] trait Llm { async fn complete_json(&self, req: LlmRequest) -> VoiceResult<LlmResponse>; }
#[async_trait] trait Decider { async fn decide(&self, text: &str, ctx: &DecideContext) -> VoiceResult<AssistantDecision>; }   // rules first, then Llm; can fail with llm_unavailable
```
**D1's traits (agreed in T-004, `speech.rs` / `voice.rs`).** They return `VoiceError` (`VoiceResult<T>`), not `AppError`, so Gnani failures such as `InvalidApiKey`, `RateLimited` or `InsufficientCredit` reach the commands unchanged. These types are Rust only (not exported to TypeScript).
```rust
const CAPTURE_SAMPLE_RATE: u32 = 16_000; const FRAME_BYTES: usize = 1_024; const PLAYBACK_SAMPLE_RATE: u32 = 24_000;

struct AudioFrame { session_id: String, seq: u64, pcm: Vec<u8> /* exactly FRAME_BYTES */ }
trait FrameSink: Send + Sync {          // called ~31×/s on the command thread: must not block
  fn name(&self) -> &'static str;
  fn push(&self, frame: &AudioFrame) -> VoiceResult<()>;   // an error is reported; other sinks still get the frame
  fn end(&self, session_id: &str) -> VoiceResult<()> { Ok(()) }
}

struct RtConfig { language_code: String /* "indic-auto" or 1–4 codes */, format: SttFormat, keyterms: Vec<String>,
  idle_timeout_sec: Option<u16>, min_silence_ms: Option<u32> }          // model and 16 kHz are fixed by the client
enum RtInput { Frame(Vec<u8>), Commit, Close }
struct RtTranscript { text: String, segment_index: u32, language: Option<String>, audio_duration_ms: Option<u32>, latency_ms: Option<u32> }
enum RtEvent { Connected { request_id: Option<String> }, SpeechStart, Processing, Transcript(RtTranscript),
  Reconnecting { attempt: u32 }, Error(VoiceError), Closed }
struct RtSession { id: String, input: mpsc::Sender<RtInput>, events: mpsc::Receiver<RtEvent> }   // events end with Closed
#[async_trait] trait SpeechToText: Send + Sync { async fn open(&self, config: RtConfig) -> VoiceResult<RtSession>; }

struct TtsRequest { text: String, language: String, voice: Option<String>, model: Option<String>, speed: Option<f32> }
enum AudioContainer { Mp3, Wav }
type PcmSink = mpsc::Sender<Vec<u8>>;   // PCM16 24 kHz chunks; a closed receiver = barge-in, ends the stream without error
#[async_trait] trait TextToSpeech: Send + Sync {
  async fn stream(&self, request: TtsRequest, out: PcmSink) -> VoiceResult<()>;
  async fn synthesize(&self, request: TtsRequest, container: AudioContainer) -> VoiceResult<Vec<u8>>;   // recap export
}

#[async_trait] trait ToolExecutor: Send + Sync {
  async fn run_read(&self, tool: &str, args: &Value) -> VoiceResult<ToolResult>;   // a write tool → ToolFailed, nothing runs
  async fn propose(&self, request: ProposalRequest) -> VoiceResult<Proposal>;      // stores a pending proposal; never executes
}
```
**Mocks for D1's traits:** `MockStt`, `MockTts` in `pluto-gnani::mock` (feature `mock`); `MockToolExecutor`, `RecordingSink` in `pluto_contracts::mock` (feature `mock`; here because `pluto-intel` never depends on `pluto-voice`). Enable the feature from `[dev-dependencies]`.
`DecideContext` holds the last 6 turns, today's open task titles, participant names, the user's timezone and "now". Each trait ships with a mock implementation in its owner's crate in the same PR that defines it.

---

## 4. IPC commands

**Existing commands (99, in `src-tauri/src/ipc.rs`) are not redefined here.** The ones the new work relies on: `tasks_query`, `task_quick_add`, `task_update`, `task_transition`, `task_defer`, `reminders_list`, `reminder_create`, `reminder_set_done`, `notes_search`, `note_create`, `focus_start`, `focus_stop`, `meeting_create`, `meeting_update`, `meetings_list`, `meeting_get`, `meeting_delete`, `action_item_convert`, `participants_suggest`, `setting_set`, `prefs_get`, `prefs_set`.

**New commands** (§4.2, §4.3) are registered together in `generate_handler!` at task T-007. Existing commands return `AppError`; **new commands return `VoiceError`** (§6).

**Owners:**
- §4.2: Developer 1 (`src-tauri/src/voice.rs`).
- §4.3: Developer 1 writes the stubs (T-007); commands marked *(D2)* are then implemented by Developer 2 in `crates/pluto-intel`, with thin wrappers in `src-tauri/src/meeting_ai.rs` (WORKING_AGREEMENT §2).

### 4.2 Voice
| Command | Args | Returns |
|---|---|---|
| `voice_settings_get` | — | `VoiceSettings` |
| `voice_settings_set` | `patch: VoiceSettingsPatch` | `VoiceSettings` |
| `voice_key_set` | `account: String, secret: String` | `()` |
| `voice_key_clear` | `account` | `()` |
| `voice_status` | — | `VoiceStatus` |
| `voice_ptt_start` | — | `String` (session id) |
| `voice_ptt_stop` | — | `()` (sends commit) |
| `voice_cancel` | — | `()` |
| `audio_push` | **raw body:** PCM16 frame (`Uint8Array`), header `x-session-id` | `()` |
| `voice_proposal_respond` | `proposal_id: String, response: ProposalResponse` | `ToolResult` |
| `voice_speak` | `text: String, language: Option<String>, playback: Channel<Vec<u8>>` | `()` |
| `voice_stop_speaking` | — | `()` |
| `voice_usage` | — | `UsageCounters` |
| `voice_history` | `limit: u32` | `Vec<AssistantTurn>` |
| `privacy_log_list` | `limit: u32` | `Vec<PrivacyLogEntry>` |
| `privacy_log_clear` | — | `()` |

### 4.3 Meetings
| Command | Args | Returns |
|---|---|---|
| `meeting_rec_start` | `title, participants: Vec<String>, live_captions: bool, consent: bool` | `RecordingSession` |
| `meeting_rec_pause` / `meeting_rec_resume` / `meeting_rec_stop` | `session_id` | `RecordingSession` (stop also queues the batch job) |
| `meeting_audio_pick` | — | `Option<String>` (a path chosen in the system "open" dialog; `None` if cancelled. The path comes only from the dialog, never from the webview) |
| `meeting_import` | `title, participants, path` | `TranscriptionJob` (also creates the meeting) |
| `meeting_job_get` | `meeting_id` | `Option<TranscriptionJob>` |
| `meeting_job_retry` | `meeting_id` | `TranscriptionJob` |
| `meeting_transcript_get` *(D2)* | `meeting_id` | `Vec<TranscriptSegment>` |
| `meeting_speakers_set` *(D2)* | `meeting_id, mapping: Vec<SpeakerMapping>` | `()` |
| `meeting_extract` *(D2)* | `meeting_id` | `ExtractionResult` |
| `meeting_extraction_apply` *(D2)* | `input: ApplyExtraction` | `Meeting` |
| `meeting_items_get` *(D2)* | `meeting_id` | `Vec<ExtractedItem>` (applied) |
| `meeting_recall` *(D2)* | `question, language: Option<String>` | `RecallAnswer` |
| `meeting_recap_text` *(D2)* | `meeting_id, language` | `String` (a 60–90 word recap written only from the applied items; fails if nothing is applied) |
| `meeting_recap` *(D2)* | `meeting_id, language, speak: bool, export: bool, playback: Option<Channel<Vec<u8>>>` | `Option<String>` (mp3 path) |
| `commitments_list` *(D2)* | `owner: Option<String>, status: Option<CommitmentStatus>` | `Vec<Commitment>` |
| `glossary_list` / `glossary_add` / `glossary_delete` | `term` / `id` | `Vec<GlossaryTerm>` |
| `compare_transcribe` (P1) *(D2)* | `path` | `{ prisma: String, baseline: Option<String> }` |

---

## 5. Events and channels (Rust → UI)
The base's events (`ReminderDue`, `FocusChanged`, `TaskCreated`, …) keep flowing through the single `pluto://event` channel and are not redefined. **New events** use separate Tauri event names:

| Name | Payload | Source |
|---|---|---|
| `voice://status` | `VoiceStatus` | Runtime, on every transition |
| `voice://transcript` | `TranscriptSegment` | Prisma Realtime (assistant) |
| `voice://turn` | `AssistantTurn` | Assistant |
| `voice://proposal` | `Proposal` | Assistant |
| `voice://error` | `VoiceError` | Any |
| `meeting://caption` | `TranscriptSegment` | Recorder (Realtime) |
| `meeting://ticker` (P1) | `ExtractedItem` | Live relevance |
| `meeting://job` | `TranscriptionJob` | Batch poller |
| Channel `playback` | PCM16 24 kHz bytes | TTS streaming |

---

## 6. Errors
Existing commands keep core's `AppError { code, message, field }` with `ErrorCode` = `VALIDATION | NOT_FOUND | INVALID_TRANSITION | CONFLICT | DB | IO | INTERNAL` (fixed, 7 values). **New commands return `VoiceError`**, defined in `crates/pluto-contracts/src/error.rs`:
```rust
struct VoiceError { code: VoiceErrorCode, message: String, retryable: bool }
enum VoiceErrorCode {   // snake_case on the wire
  CloudVoiceDisabled, MissingApiKey, InvalidApiKey, InsufficientCredit, RateLimited, MicUnavailable, MicPermissionDenied,
  NetworkOffline, GnaniUpstream, GnaniBadRequest, BatchJobFailed, FileTooLarge, UnsupportedAudio,
  LlmUnavailable, LlmInvalidOutput, ProposalExpired, ToolFailed, ConsentRequired, Busy, Core }
```
`Core` wraps an `AppError` from a core function that failed during a tool call (`message` carries its text). The UI's `src/ipc/voiceClient.ts` and `meetingAi.ts` handle `VoiceError`; the base's `errorMessage()` helper keeps handling `AppError`.

**Mapping from external responses:**

| External response | Maps to |
|---|---|
| Gnani STT REST `{success:false, error:{type}}` | `GnaniBadRequest` |
| Gnani auth / rate-limit body `{detail:{error_code}}` | `InvalidApiKey` / `MissingApiKey` / `RateLimited` |
| Gnani 402 | `InsufficientCredit` |
| Gnani 5xx / 504 | `GnaniUpstream` (retryable) |
| Gnani Batch `{error, message}` | `BatchJobFailed` |
| LLM 429 / 5xx / timeout | Fallback provider; if both fail, `LlmUnavailable` |
| LLM invalid JSON after one repair attempt | `LlmInvalidOutput` |
