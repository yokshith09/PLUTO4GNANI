# Technical Architecture — Pluto Voice

**Contracts:** all types, commands and events are in `API_CONTRACTS.md`. All tables are in `DATA_SCHEMA.md`. This file covers structure and behaviour only.

## 1. Overview
A standalone **Tauri v2** desktop app (Windows is the target):
- The **web layer** (React/TypeScript in WebView2) handles UI, **mic capture** and **audio playback**.
- The **native layer** (Rust) handles Gnani, the LLMs, the database, recording files, tool execution and all network I/O.

```
┌──────────── Web layer: React + TypeScript (WebView2) ───────────┐
│ audio/capture: getUserMedia → AudioWorklet → 16 kHz PCM16 mono   │
│                → 1,024-byte frames ──────────┐                   │
│ audio/player : Web Audio gapless queue ◄─────┼── PCM chunks      │
│ features/    : tasks, reminders, notes, focus, projects,         │
│                meetings (record, captions, review, recall),      │
│                voice (overlay, history), settings                │
└──────────── invoke() / Channel / listen() (API_CONTRACTS) ──────┘
┌──────────── app (src-tauri): thin shell ────────────────────────┐
│ command + event registration, hotkey, tray, notifications,       │
│ keychain                                                         │
└──────────────────────────────┬──────────────────────────────────┘
┌──────── crates/voice (D1) ───────┐  ┌──────── crates/intel (D2) ───────┐
│ gnani   : realtime, rest, batch, │  │ llm     : GeminiClient, GroqClient│
│           tts, clone, queues,    │  │           router + fallback       │
│           retry                  │  │ decide  : rules → LLM             │
│ runtime : state machine,         │  │ meeting : speakers, relevance,    │
│           sessions, recorder     │  │           extraction, validation, │
│ assistant: tools, proposals,     │  │           recall, recap,          │
│           execution, context     │  │           commitments, dates      │
│ privacy : opt-in, outbound log,  │  │                                   │
│           usage counters         │  │                                   │
└───────────────┬──────────────────┘  └───────────────┬───────────────────┘
        talk only through traits in crates/types (Llm, Decider,
        SpeechToText, TextToSpeech, ToolExecutor); app wires them
┌──────────── crates/core (D1, no tauri dep) ─────────────────────┐
│ db (SQLite, migrations), tasks, reminders (+scheduler), notes,   │
│ focus, projects, meetings, settings, search (FTS5)               │
└──────────────────────────────┬──────────────────────────────────┘
┌──────────── crates/types (shared contracts, no logic) ──────────┐
│ ids, time, error, personal, voice, speech, meeting, llm, decide  │
└─────────────────────────────────────────────────────────────────┘
```

## 2. Repository layout
Full ownership, hot-file and branch rules are in `WORKING_AGREEMENT.md`.
```
/app            Tauri shell: main.rs, commands/ (registry.rs, personal.rs, voice.rs, meetings.rs, settings.rs), events.rs, keychain.rs   D1
/crates/types   shared contract types and traits (no logic)                                                                    D1 + D2 (file split)
/crates/core    domain + SQLite + migrations/001_init.sql …                                                                    D1
/crates/voice   Gnani clients, runtime, recorder, assistant, privacy                                                           D1
/crates/intel   LLM layer, decisions, meeting intelligence                                                                     D2
/ui             React + Vite app (src/audio, src/features, src/lib/api, src/lib/mock)                                          D2 (features/personal: D1)
/test-data      audio-scripts/, ground-truth/, fixtures/                                                                       D2 (some fixtures: D1)
/docs           this pack
```
Dependency direction: `types ← core ← voice` and `types ← core ← intel`. `voice` and `intel` never depend on each other. Only `app` depends on `tauri`.

## 3. Tech stack
| Layer | Technology | Purpose | Status |
|---|---|---|---|
| Desktop shell | **Tauri v2** (Rust stable) | Windows app, IPC, tray, packaging | Approved |
| UI | **React 18 + TypeScript + Vite** | All screens | Approved |
| Styling / components | Tailwind CSS + shadcn/ui (Radix) | Fast, accessible UI | Proposed |
| UI data | TanStack Query | Caching of IPC calls | Proposed |
| Mic capture | **Browser `getUserMedia` + `AudioWorklet`** (in WebView2) | Capture and convert to 16 kHz PCM16 mono | Approved (hybrid) |
| Playback | **Web Audio API** (scheduled `AudioBufferSourceNode` queue) | Gapless streaming TTS | Approved (hybrid) |
| Async runtime | `tokio` | Network and background jobs | Proposed |
| HTTP | `reqwest` (rustls, multipart) | Gnani REST/Batch, Gemini, Groq | Proposed |
| WebSocket | `tokio-tungstenite` | Prisma Realtime, Timbre streaming | Proposed |
| Database | SQLite via `rusqlite` (bundled) + FTS5 | Local data and transcript search | Proposed (FTS5 verified by test T-DB-03) |
| WAV files | `hound` | Crash-safe recording chunks | Proposed |
| Secrets | `keyring` (Windows Credential Manager) | API keys | Proposed |
| Type sharing | `ts-rs` | Rust → TypeScript types | Proposed |
| Tauri plugins | global-shortcut, notification, dialog | Hotkey, reminders, file picker | Proposed |
| Speech | **Gnani Prisma v3.0, Timbre v3.0** | STT and TTS | Approved |
| Reasoning | **Gemini API + Groq API** (free tiers) | Extraction, recall, intent | Approved |
| Tests | `cargo test`, `wiremock`, Vitest + Testing Library | Unit, contract, UI | Proposed |
| Fallback capture | `cpal` (Rust) | Only if WebView2 mic fails the Day-1 test | Proposed |

## 4. Hybrid audio path
**Capture (web → Rust):**
1. `navigator.mediaDevices.getUserMedia({audio:{echoCancellation:true, noiseSuppression:true, channelCount:1}})`.
2. An `AudioWorklet` downsamples the device rate (usually 48 kHz) to 16 kHz and converts float to Int16.
3. Audio is packed into **1,024-byte frames** (512 samples; Gnani's recommended frame size).
4. Each frame is sent to Rust with `invoke('audio_push', <Uint8Array>)` (raw binary body). The rate is about 31 frames/s, 32 KB/s.
5. Rust forwards each frame to the Prisma Realtime socket and, when recording, to the WAV writer.

**Playback (Rust → web):**
1. Rust receives Timbre WebSocket `audio` messages (base64), decodes them to PCM16 at 24 kHz, and pushes the bytes over a Tauri `Channel`.
2. The UI turns them into `AudioBuffer`s and schedules them back to back.
3. A stop command clears the queue (barge-in).

**Why Gnani calls never run in the web layer [Certain]:**
- The browser WebSocket API can't set the `x-api-key-id` header that Prisma Realtime requires.
- Keys in the UI would be readable through developer tools.

**Day-1 verification (T-IPC-01):**
- WebView2 shows the mic permission prompt and capture works.
- Raw binary `invoke` and binary `Channel` work.
- **If either fails:** capture moves to Rust with `cpal`. The contracts are unchanged, because Rust receives the same frames either way.

## 5. Voice runtime (shared by both modes)
| State | Meaning | Next |
|---|---|---|
| `Disabled` | Cloud voice off or keys missing | `Idle` |
| `Idle` | Ready | `Listening`, `Recording` |
| `Listening` | Push-to-talk held; Realtime session open | `Thinking`, `Idle` |
| `Thinking` | Rules → LLM → proposal or answer | `AwaitingApproval`, `Speaking`, `Error` |
| `AwaitingApproval` | Proposal shown and spoken; 30 s timeout | `Executing`, `Speaking` |
| `Executing` | Domain service call | `Speaking` |
| `Speaking` | TTS playback; the hotkey stops it (barge-in) | `Idle`, `Listening` |
| `Recording` | Meeting capture → WAV (+ Realtime captions) | `Idle` |
| `Error` | Error with one recovery action | `Idle` |

**Rules:**
- While `Recording`, the assistant hotkey answers "Busy: recording", unless G-2 confirms a second Realtime session is allowed.
- One capture stream feeds both the WAV writer and Realtime.

## 6. Gnani integration
Exact contracts are in API_CONTRACTS §2.

| Use | API | Key settings |
|---|---|---|
| Assistant input, live captions | Prisma Realtime `wss://api.vachana.ai/stt/v3/stream` | `model: prisma-v3.0`, `language_code: indic-auto`, `format: codemix`, `x-sample-rate: 16000`, `keyterms`. `commit` on hotkey release. Rotate the connection at 14 min (cap 15). Reconnect with backoff |
| Meeting transcript | Prisma Batch `/stt/v3/batch/jobs` | `with_diarization: true`, `num_speakers` = participant count (1–8), `speaker_names` (≤4), `keyterms` (≤200), `format: codemix`. Poll every 5 s backing off to 15 s. Recordings in ≤ 4.5-min WAV chunks (16 kHz mono ≈ 1.9 MB/min, under the 10 MB upload limit). Segment times offset per chunk |
| Short clips, comparison | Prisma REST `POST /stt/v3` | ≤ 30 s |
| Spoken replies | Timbre WS `wss://api.vachana.ai/api/v1/tts` | `timbre-v3.0`, `container: raw`, `encoding: linear_pcm`, 24 kHz. Language = detected language |
| Recap files | Timbre REST `POST /api/v1/tts/inference` | `container: mp3` |
| Own-voice recap (P2) | Clone embeddings + `vachana-vc-v1` | User's own voice only |

**Concurrency:**
- Each REST family runs through a queue that allows 1 request in flight. The docs show a 429 "Only 1 active request is allowed at a time".
- Retries: 0.5 / 1 / 2 s backoff, 3 attempts, on connection errors and 429/500/503/504. Honour `Retry-After`.
- A 402 (insufficient credit) disables voice and shows a banner.

## 7. LLM layer (Gemini + Groq, split by job)
- **`Llm` trait:** `complete_json(task, system, messages, json_schema) -> JSON`. Implementations: `GeminiClient`, `GroqClient`, `MockLlm`.
- **Router:**

| Task | Primary | Fallback | Why |
|---|---|---|---|
| `intent` (voice command → tool + arguments) | Groq | Gemini | Short prompts; speed sets the reply time |
| `relevance` (meeting segments, batches of ≤ 40) | Gemini | Groq | Long input; Indian-language text |
| `extract` (meeting items) | Gemini | Groq | Long context |
| `recall_answer` | Gemini | Groq | Grounded multilingual answer |
| `recap` | Gemini | Groq | Multilingual generation |

- **Fallback triggers:** timeout (intent 4 s, others 30 s), 429, 5xx, or invalid JSON after one repair attempt.
- **Free-tier limits (third-party, March 2026; re-check on Day 1, L-1):**
  - Groq: 30–60 requests/min. Token limits per minute range from 6,000 to 30,000 depending on the model (e.g. `llama-3.3-70b-versatile`: 1,000 requests/day, 12,000 tokens/min).
  - Gemini: limits shown in AI Studio. Not verified here.
- **Model IDs:** chosen on Day 1 from each provider's current list. Candidates on Groq: `llama-3.3-70b-versatile`, `openai/gpt-oss-120b`. On Gemini: the current Flash model.
- **Grounding (enforced in Rust):**
  - Every extracted item must reference an existing segment id; otherwise it is dropped.
  - Recall answers may quote only retrieved segments; otherwise the answer is "Not found in your meetings."
- **Dates:** the LLM returns the phrase ("Friday tak") plus a hint. Rust resolves the actual date against the meeting date and local timezone.

## 8. Decisions (rules → LLM)
- **Rules** (zero cost): yes/no/cancel/stop in Hindi, Tamil, Kannada and English, plus fixed commands like "focus 25 minutes".
- **LLM:** everything else. One call returns intent + tool + arguments.
- **Meeting relevance** is batched (one call per ≤ 40 segments).

## 9. Memory and retrieval
- **Meeting memory:** `transcript_segments` plus the FTS5 index.
- **Recall:**
  1. The LLM extracts the people, topic and time range in the question.
  2. FTS + filters → top 8 segments.
  3. Grounded answer.
  4. Spoken via TTS.
- **Conversation context:** the last 6 turns in memory (resolves "it", "that task"). Reset after 5 min idle. Turns persisted in `voice_interactions`.

## 10. Tool execution
- Tools map to `crates/core` service functions (list in API_CONTRACTS §3.5). `voice::assistant` implements the `ToolExecutor` trait.
- **Reads** run directly. **Writes** create a `Proposal` and run only after `voice_proposal_respond(Approve)`.
- Execution happens in Rust. The UI is never involved in authorising writes, except to show the proposal and send the response.

## 11. Interruptions and recovery
| Event | Behaviour |
|---|---|
| Mic lost / permission revoked | Recording pauses. Error with "Check microphone". Resume continues in a new chunk |
| Network drop during recording | WAV continues. Captions show "offline". The batch job is queued until online |
| Realtime 15-min cap | Rotate at 14 min |
| App crash while recording | Chunks are flushed every ≤ 30 s. "Recover recording?" on the next launch |
| Batch `FAILED` / `PARTIAL_FAILURE` | Per-chunk error. Retry failed parts |
| LLM provider down or rate-limited | Router falls back to the other provider. If both fail: error + Retry |
| 402 credits | Voice disabled; banner |
| Proposal unanswered for 30 s | Expired; "Cancelled" |

## 12. Privacy and permissions
- **Cloud voice** is off by default. The disclosure says: audio → Gnani; transcript text → Gemini / Groq.
- **[Likely]** Free-tier LLM data may be used by the provider to improve its products. The disclosure says so, and the demo uses synthetic data only.
- Every mic recording needs a stored consent confirmation.
- Keys live in Windows Credential Manager via `keyring` and are never logged.
- The CSP allows IPC only; the UI makes no external requests.
- Recordings are stored in the app data folder, with a delete action.

## 13. Gnani unknowns and their fallbacks (non-blocking)
| Unknown | Fallback built in |
|---|---|
| G-1 credits | Usage counters; push-to-talk default; cached fixed TTS phrases |
| G-2 concurrency | Request queues; "Busy" while recording |
| G-3 Realtime hints | `keyterms` only on Realtime |
| G-4 presigned upload | ≤ 4.5-min WAV chunks; imports ≤ 10 MB |

## 14. Deployment
- **Build:** `pnpm tauri build` → Windows installer (NSIS/MSI). Unsigned for the hackathon (Windows SmartScreen warning is expected).
- **Configuration:** keys entered in Settings → keychain.

## 15. Technical risks
| Risk | Mitigation |
|---|---|
| WebView2 mic or binary IPC doesn't work as expected | Day-1 test T-IPC-01; `cpal` fallback |
| Free-tier LLM limits hit during the demo | Router fallback; record the demo early; keep prompts small |
| Code-mixed accuracy not better than the baseline | Show the comparison honestly; lean on extraction and recall |
| Full personal scope too large | Tiered P0/P1/P2; cut rules in `HACKATHON_PLAN.md` (repo root) |
| Merge conflicts between the two developers | Ownership rules in INTEGRATION_CONTRACT §4 |
