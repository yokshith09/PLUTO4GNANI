# Technical Architecture — Pluto Voice

**Contracts:** all types, commands and events are in `API_CONTRACTS.md`. All tables are in `DATA_SCHEMA.md`. This file covers structure and behaviour only.

## 1. Overview
A **Tauri v2** desktop app (Windows is the target), built on the ported Pluto base (`PORTING_FROM_PLUTO.md`):
- The **web layer** (React/TypeScript in WebView2) is Pluto's UI plus the new Meetings, Voice and Settings parts, and **mic capture and audio playback**.
- The **native layer** (Rust) is Pluto's shell and core plus four new crates. It handles Gnani, the LLMs, the database, recording files, tool execution and all network I/O.

```
┌──────────── Web layer: React + TypeScript (WebView2) — src/ ────────────┐
│ existing pages: Today, Tasks, Notes, Projects, Meetings, Settings …       │
│ NEW  src/audio/ : getUserMedia → AudioWorklet → 16 kHz PCM16 mono         │
│                   → 1,024-byte frames ──────────────┐   (D1)              │
│                   Web Audio gapless player ◄─────────┼── PCM chunks       │
│ NEW  src/features/meetings (record, import, transcript, review, recall)   │
│      src/features/voice (bar, proposal card, history) · settings (D2)     │
│ src/ipc/: typed client + mock (existing) + voiceClient.ts, meetingAi.ts   │
└───────────── invoke() / Channel / listen() (API_CONTRACTS) ──────────────┘
┌──────────── src-tauri (Tauri shell, D1) ────────────────────────────────┐
│ existing: 99 commands, tray, shortcuts, notifications, pet               │
│ NEW: voice.rs, meeting_ai.rs, hotkey press+release, voice:// events       │
└──────────────────────────────┬──────────────────────────────────────────┘
┌──────── pluto-voice (D1) ────────┐  ┌──────── pluto-intel (D2) ────────────┐
│ runtime : state machine, sessions │  │ llm     : Gemini, Groq, router        │
│ assistant: tools, proposals,      │  │ decide  : rules → LLM                 │
│           approval, context       │  │ meeting : extraction, validation,     │
│ privacy : opt-in, usage           │  │           dates, recall, recap,       │
│                                   │  │           recorder, commitments       │
└───────────────┬───────────────────┘  └──────────────────┬───────────────────┘
┌──────── pluto-gnani ─────────────────────────────────────────────────────┐
│ http (queue, retry) · realtime · tts · batch (D1) · rest (D2)              │
└──────────────────────────────┬───────────────────────────────────────────┘
        voice and intel talk only through traits in pluto-contracts
┌──────── pluto-contracts (no logic): voice, speech, error | meeting, llm, decide ┐
┌──────── pluto-core (existing, no tauri dep) ─────────────────────────────┐
│ db + migrations 001–013, tasks, reminders (+scheduler), notes, focus,     │
│ meetings, settings … + meeting_ai_store (012, D2), voice_store (013, D1)  │
└──────────────────────────────────────────────────────────────────────────┘
```

## 2. Repository layout
Ownership and shared-file rules: `WORKING_AGREEMENT.md` §1–§3.
```
src-tauri/                existing shell; new: voice.rs (D1), meeting_ai.rs (D2)
src/                      existing UI; new: audio/ (D1), features/meetings, voice, settings (D2), ipc/voiceClient.ts, meetingAi.ts
crates/pluto-core/        existing core; new store files for migrations 012 (D2) and 013 (D1)
crates/pluto-contracts/   NEW  shared types and traits (voice, speech, error: D1; meeting, llm, decide: D2)
crates/pluto-gnani/       NEW  Gnani clients (http, realtime, tts, batch: D1; rest: D2)
crates/pluto-voice/       NEW  runtime, assistant, privacy (D1)
crates/pluto-intel/       NEW  LLM, decisions, meeting intelligence, recorder (D2)
test-data/                NEW  audio scripts, ground truth, fixtures
```
Dependency direction: `pluto-core ← pluto-contracts ← pluto-gnani ← pluto-voice` and `pluto-gnani ← pluto-intel`. `pluto-voice` and `pluto-intel` never depend on each other. Only `src-tauri` depends on `tauri`.

## 3. Tech stack
| Layer | Technology | Purpose | Status |
|---|---|---|---|
| Desktop shell | **Tauri v2** (Rust 1.97.0, pinned) | Windows app, IPC, tray, packaging | Exists (base) |
| UI | **React 18 + TypeScript + Vite** | All screens | Exists (base) |
| Styling / components | Tailwind CSS 4 + Radix UI | Pluto's theme and base components | Exists (base) |
| UI data | TanStack Query | Caching of IPC calls | Exists (base) |
| Mic capture | **Browser `getUserMedia` + `AudioWorklet`** (in WebView2) | Capture and convert to 16 kHz PCM16 mono | Approved (hybrid) |
| Playback | **Web Audio API** (scheduled `AudioBufferSourceNode` queue) | Gapless streaming TTS | Approved (hybrid) |
| Async runtime | `tokio` | Network and background jobs | Exists (base) |
| HTTP | `reqwest` (rustls, multipart) | Gnani REST/Batch, Gemini, Groq | Proposed |
| WebSocket | `tokio-tungstenite` | Prisma Realtime, Timbre streaming | Proposed |
| Database | SQLite via `rusqlite` (bundled) | Local data. **FTS5 is new** for transcript search (verified by T-DB-03); notes keep their `LIKE` search | Exists (base); FTS5 proposed |
| WAV files | `hound` | Crash-safe recording chunks | Proposed |
| Secrets | `keyring` (Windows Credential Manager) | API keys | Proposed |
| Type sharing | `ts-rs` → `src/ipc/generated` | Rust → TypeScript types (`pnpm gen:types`) | Exists (base); extended to the new crates |
| Tauri plugins | global-shortcut, notification, dialog, autostart, single-instance, window-state | Already used by the base | Exists (base) |
| Speech | **Gnani Prisma v3.0, Timbre v3.0** | STT and TTS | Approved |
| Reasoning | **Gemini API + Groq API** (free tiers) | Extraction, recall, intent | Approved |
| Tests | `cargo test`, Vitest + Testing Library (base); `wiremock` for Gnani contract tests (new) | Unit, contract, UI | Exists (base) / proposed |
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
  - Groq, measured on the team account for `openai/gpt-oss-120b`: 1,000 requests/day and 8,000 tokens/minute (response headers). A short voice command uses about 500 tokens, so about 15 a minute; longer prompts hit the token limit first, and the router then falls back to Gemini.
  - Gemini: limits shown in AI Studio. Not verified here.
- **Model IDs (chosen 4 Nov 2026, `docs/results/d2-llm-check.md`):** Gemini `gemini-2.5-flash`, Groq `openai/gpt-oss-120b` (`llama-3.3-70b-versatile` is not available on the team's Groq account). Both are settings (`llm.gemini_model`, `llm.groq_model`), so a model can be swapped without a release.
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
- Tools map to `pluto-core` functions (list in API_CONTRACTS §3.5). `voice::assistant` implements the `ToolExecutor` trait.
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
