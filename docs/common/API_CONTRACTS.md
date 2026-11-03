# API Contracts — Pluto Voice

**Single authority** for:
- external APIs (§2);
- shared types (§3);
- IPC commands (§4);
- events and channels (§5);
- errors (§6).

Other documents link here and never redefine these.

**Conventions:**
- Rust types live in `crates/voice/src/types.rs` and `crates/core/src/types.rs` and are exported to TypeScript with `ts-rs`.
- Fields are `snake_case`. Timestamps are ms since the Unix epoch (UTC). Dates are `YYYY-MM-DD`.

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

### 3.1 Personal domain (`crates/core`)
```rust
enum TaskStatus { Todo, InProgress, Done, Cancelled }
enum Priority { Low, Medium, High }
struct Task { id: String, title: String, notes: String, status: TaskStatus, priority: Option<Priority>,
  due_date: Option<String>, project_id: Option<String>, source_action_item_id: Option<String>,
  created_at: i64, updated_at: i64, completed_at: Option<i64> }
struct TaskInput { title: String, notes: Option<String>, priority: Option<Priority>, due_date: Option<String>, project_id: Option<String> }
struct TaskPatch { title: Option<String>, notes: Option<String>, status: Option<TaskStatus>, priority: Option<Priority>,
  due_date: Option<Option<String>>, project_id: Option<Option<String>> }
struct TaskFilter { view: TaskView /* Today | Upcoming | All | Done */, project_id: Option<String>, query: Option<String> }

struct Reminder { id: String, title: String, remind_at: i64, task_id: Option<String>, done_at: Option<i64>, fired_at: Option<i64>, created_at: i64 }
struct ReminderInput { title: String, remind_at: i64, task_id: Option<String> }

struct Note { id: String, title: String, body: String /* Markdown */, project_id: Option<String>, created_at: i64, updated_at: i64 }
struct NoteInput { title: String, body: String, project_id: Option<String> }

struct FocusSession { id: String, task_id: Option<String>, planned_minutes: u32, started_at: i64,
  paused_ms: i64, ended_at: Option<i64>, state: FocusState /* Running | Paused | Completed | Stopped */ }

struct Project { id: String, name: String, color: Option<String>, archived: bool, created_at: i64 }
```

### 3.2 Voice runtime (`crates/voice`)
```rust
enum VoiceState { Disabled, Idle, Listening, Thinking, AwaitingApproval, Executing, Speaking, Recording, Error }
enum VoiceMode { Assistant, Meeting }
struct VoiceStatus { state: VoiceState, mode: Option<VoiceMode>, detail: Option<String>, session_id: Option<String> }
enum SttFormat { Codemix, Transliterate, Verbatim, Transcribe }
struct VoiceSettings { cloud_voice_enabled: bool, languages: Vec<String>, stt_format: SttFormat,
  tts_voice: String, tts_model: String, hotkey: String, audio_tags: bool, always_on_enabled: bool,
  has_gnani_key: bool, has_gemini_key: bool, has_groq_key: bool }
```

### 3.3 Transcript and meetings
```rust
struct Meeting { id: String, title: String, starts_at: i64, participants: Vec<String>, status: MeetingStatus, created_at: i64 }
enum MeetingStatus { Recording, Transcribing, NeedsSpeakers, NeedsReview, Done, Failed }

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

### 3.4 Decisions
```rust
enum Intent { MeetingQuery, Task, Reminder, Note, Focus, Project, Approval, Reject, Cancel, Noise }
enum DecisionSource { Rules, Llm }
struct Decision { intent: Intent, tool: Option<String>, args: serde_json::Value, confidence: Option<f32>, source: DecisionSource, latency_ms: u32 }
```

### 3.5 Tools and proposals
| Tool | Write? | Priority |
|---|---|---|
| `tasks_list` | | P0 |
| `task_create` | W | P0 |
| `task_update` | W | P0 |
| `task_complete` | W | P0 |
| `task_delete` | W | P0 |
| `reminders_list` | | P0 |
| `reminder_create` | W | P0 |
| `reminder_done` | W | P0 |
| `meeting_search` | | P0 |
| `commitments_list` | | P1 |
| `notes_search` | | P1 |
| `note_create` | W | P1 |
| `focus_start` | W | P1 |
| `focus_stop` | W | P1 |
| `projects_list` | | P2 |
| `project_create` | W | P2 |

**W** = write (needs approval).

```rust
struct Proposal { id: String, interaction_id: String, tool: String, args: serde_json::Value, summary_text: String, language: String, expires_at: i64 }
enum ProposalResponse { Approve, Reject, Edit { args: serde_json::Value } }
struct ToolResult { tool: String, ok: bool, message: String, record_id: Option<String> }
struct AssistantTurn { interaction_id: String, transcript: String, language: Option<String>, decision: Decision,
  reply_text: Option<String>, proposal: Option<Proposal>, result: Option<ToolResult> }
```

### 3.6 Privacy and usage
```rust
enum Destination { GnaniRealtime, GnaniRest, GnaniBatch, GnaniTts, GnaniClone, Gemini, Groq }
struct PrivacyLogEntry { id: i64, at: i64, destination: Destination, purpose: String, bytes: i64, ref_id: Option<String> }
struct UsageCounters { stt_seconds_today: f64, batch_seconds_today: f64, tts_chars_today: i64, gemini_calls_today: i64, groq_calls_today: i64 }
```

---

## 4. IPC commands

All commands return `Result<T, AppError>` (§6).

**Owners:**
- §4.1 and §4.2: Developer 1 (backend and personal UI).
- §4.3: Developer 1, except commands marked *(D2)*, which Developer 2 implements in `crates/voice::meeting`.
- Meeting, voice and settings UI: Developer 2.

### 4.1 Personal (P0: tasks, reminders · P1: notes, focus · P2: projects)
| Command | Args | Returns |
|---|---|---|
| `tasks_list` | `filter: TaskFilter` | `Vec<Task>` |
| `task_create` | `input: TaskInput` | `Task` |
| `task_update` | `id: String, patch: TaskPatch` | `Task` |
| `task_delete` | `id: String` | `()` |
| `reminders_list` | `include_done: bool` | `Vec<Reminder>` |
| `reminder_create` | `input: ReminderInput` | `Reminder` |
| `reminder_done` | `id: String` | `Reminder` |
| `reminder_delete` | `id: String` | `()` |
| `notes_list` | `query: Option<String>` | `Vec<Note>` |
| `note_create` | `input: NoteInput` | `Note` |
| `note_update` | `id, input: NoteInput` | `Note` |
| `note_delete` | `id` | `()` |
| `focus_start` | `task_id: Option<String>, minutes: u32` | `FocusSession` |
| `focus_pause` / `focus_resume` / `focus_stop` | — | `FocusSession` |
| `focus_current` | — | `Option<FocusSession>` |
| `focus_history` | `days: u32` | `Vec<FocusSession>` |
| `projects_list` | — | `Vec<Project>` |
| `project_create` | `name, color` | `Project` |
| `project_update` | `id, name, color, archived` | `Project` |

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
| `meetings_list` | — | `Vec<Meeting>` |
| `meeting_get` | `id` | `Meeting` |
| `meeting_delete` | `id` | `()` |
| `meeting_rec_start` | `title, participants: Vec<String>, live_captions: bool, consent: bool` | `RecordingSession` |
| `meeting_rec_pause` / `meeting_rec_resume` / `meeting_rec_stop` | `session_id` | `RecordingSession` (stop also queues the batch job) |
| `meeting_import` | `title, participants, path` | `TranscriptionJob` |
| `meeting_job_get` | `meeting_id` | `Option<TranscriptionJob>` |
| `meeting_job_retry` | `meeting_id` | `TranscriptionJob` |
| `meeting_transcript_get` *(D2)* | `meeting_id` | `Vec<TranscriptSegment>` |
| `meeting_speakers_set` *(D2)* | `meeting_id, mapping: Vec<SpeakerMapping>` | `()` |
| `meeting_extract` *(D2)* | `meeting_id` | `ExtractionResult` |
| `meeting_extraction_apply` *(D2)* | `input: ApplyExtraction` | `Meeting` |
| `meeting_items_get` *(D2)* | `meeting_id` | `Vec<ExtractedItem>` (applied) |
| `meeting_recall` *(D2)* | `question, language: Option<String>` | `RecallAnswer` |
| `meeting_recap` *(D2)* | `meeting_id, language, speak: bool, export: bool, playback: Option<Channel<Vec<u8>>>` | `Option<String>` (mp3 path) |
| `commitments_list` *(D2)* | `owner: Option<String>, status: Option<CommitmentStatus>` | `Vec<Commitment>` |
| `glossary_list` / `glossary_add` / `glossary_delete` | `term` / `id` | `Vec<GlossaryTerm>` |
| `compare_transcribe` (P1) *(D2)* | `path` | `{ prisma: String, baseline: Option<String> }` |

---

## 5. Events and channels (Rust → UI)
| Name | Payload | Source |
|---|---|---|
| `voice://status` | `VoiceStatus` | Runtime, on every transition |
| `voice://transcript` | `TranscriptSegment` | Prisma Realtime (assistant) |
| `voice://turn` | `AssistantTurn` | Assistant |
| `voice://proposal` | `Proposal` | Assistant |
| `voice://error` | `AppError` | Any |
| `meeting://caption` | `TranscriptSegment` | Recorder (Realtime) |
| `meeting://ticker` (P1) | `ExtractedItem` | Live relevance |
| `meeting://job` | `TranscriptionJob` | Batch poller |
| `reminder://due` | `Reminder` | Reminder scheduler (also triggers an OS notification) |
| `focus://tick` | `FocusSession` (every 1 s while running) | Focus timer |
| Channel `playback` | PCM16 24 kHz bytes | TTS streaming |

---

## 6. Errors
```rust
struct AppError { code: ErrorCode, message: String, retryable: bool }
enum ErrorCode { NotFound, Validation, Db,
  CloudVoiceDisabled, MissingApiKey, InvalidApiKey, InsufficientCredit, RateLimited, MicUnavailable, MicPermissionDenied,
  NetworkOffline, GnaniUpstream, GnaniBadRequest, BatchJobFailed, FileTooLarge, UnsupportedAudio,
  LlmUnavailable, LlmInvalidOutput, ProposalExpired, ToolFailed, ConsentRequired, Busy }
```

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
