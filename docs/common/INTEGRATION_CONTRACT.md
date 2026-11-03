# Integration Contract — Pluto Voice

**Types, traits, commands and events:** `API_CONTRACTS.md`. **Ownership, shared files, branches, PR rules, contract changes:** `WORKING_AGREEMENT.md`. Neither is repeated here.

## 1. Components
| Component | Owner | Input | Output | Dependencies | Mock available | Verification |
|---|---|---|---|---|---|---|
| Base (core, shell, UI) | — (ported) | — | Tasks, reminders, notes, focus, meetings | — | The base's `src/ipc/mock.ts` | Base tests; T-001, T-002 |
| `src/audio` (capture worklet, sender, player) | D1 | Mic; PCM chunks | 1,024-byte frames → `audio_push`; sound | WebView2 | WAV-file source; sine generator | T-IPC-01; downsampler unit tests |
| `audio_push`, `FrameSink` fan-out, WAV writer (Rust) | D1 | Frames | Frames to every sink | — | WAV source | Unit; T-IPC-01 |
| `pluto-gnani::{http,realtime,tts}` | D1 | Frames, text | Segments, PCM | Gnani key | `MockStt`, `MockTts`, fixture WS server, `wiremock` | Contract tests; T-GN-01 |
| `pluto-gnani::batch` | D1 | Files, config | Segments in the database; `meeting://job` | http base, migration 012 | `wiremock` job lifecycle | Contract; T-ML-01 |
| `pluto-voice::runtime` | D1 | Hotkey, commands, frames | `voice://status` | gnani, contracts | `MockStt`, `MockTts` | Transition tests |
| `pluto-voice::assistant` | D1 | `Decision` from a `Decider` | `Proposal`, `ToolResult` | core, contracts | `MockDecider` | T-AS-05; B1/B2 |
| `pluto-intel::llm` | D2 | `LlmRequest` | `LlmResponse` | Keys | `MockLlm` | T-LLM-01; T-FH-07/08 |
| `pluto-intel::decide` | D2 | Transcript | `Decision` | llm | Rules need none | Rule tests; T-ML-04 |
| `pluto-intel::meeting` (extraction, dates, recall, recap, recorder) | D2 | Segments, participants | `ExtractionResult`, `RecallAnswer` | llm, core, `TextToSpeech` | AS-1 segment fixture; `MockTts` | T-ML-02; flows A1, B2 |
| Migrations 012 (D2) and 013 (D1) | D2 / D1 | — | Tables | `pluto-core` db | In-memory DB | T-DB-01..05 |
| `src-tauri` new commands | D1 (stubs), D2 (`meeting_ai.rs`) | IPC | Results, `voice://` / `meeting://` events | all crates | Stub commands | Smoke test |
| UI: Meetings, Voice, Settings, bar | D2 | Commands, events | Screens | Generated types | New mock handlers | Vitest; E2E |

## 2. What is frozen before parallel work (checkpoint C0)
In this order. Each step is merged to `main` before anything that depends on it starts.

| Step | Task | Content | Writer → approver |
|---|---|---|---|
| 1 | T-003 | Skeleton: four crates in the workspace, module placeholders, `gen:types` extended, migrations 012/013 pre-registered | D1 → D2 |
| 2 | T-004 | Voice, speech, error types; traits `SpeechToText`, `TextToSpeech`, `ToolExecutor`, `FrameSink` | D1 → D2 |
| 2 | T-005 | Meeting, LLM, decision types; traits `Llm`, `Decider`; `MockLlm` | D2 → D1 |
| 3 | T-007 | All new commands registered as stubs in one edit | D1 → D2 |
| 4 | T-006 | `voiceClient.ts`, `meetingAi.ts`, mock handlers, `voice` page entry, fixtures | D2 → D1 |

**After C0:** every change goes through a contract PR (`WORKING_AGREEMENT.md` §5).

## 3. Mocks and fixtures
| Mock | Location | Behaviour |
|---|---|---|
| UI mock | `src/ipc/mock.ts` (existing) + handlers for the new commands | Runs when the UI is opened outside Tauri (`vite dev`) or with `VITE_MOCK`; covers every new command and every state, including errors |
| Stub commands | `src-tauri/src/voice.rs`, `meeting_ai.rs` | Return fixture data until the real implementation lands |
| `MockStt` / `MockTts` | `pluto-gnani` | Fixture transcripts with delays and errors (incl. close code 1001); a 1 s sine wave |
| `MockLlm` | `pluto-intel::llm` | Fixture JSON per task; can simulate 429 and timeouts |
| `MockDecider` | `pluto-voice::assistant` | Scripted `Decision`s so the assistant is tested without `pluto-intel` |
| Fixtures | `test-data/fixtures/` | One shared set, `<type>.<case>.json` with `contract_version`; used by the UI mock **and** by Rust tests |

Fixtures follow the documented API shapes. If a live response differs, fix the fixture **and** `API_CONTRACTS.md` in a contract PR.

## 4. Independent verification
**Developer 1, without D2:**
1. `cargo test -p pluto-core -p pluto-contracts -p pluto-gnani -p pluto-voice`.
2. `cargo run -p pluto-gnani --example rt_smoke -- test-data/audio/as4_01.wav` (live key).
3. `cargo run -p pluto-gnani --example batch_smoke -- test-data/audio/as1.wav`.
4. `cargo run -p pluto-gnani --example tts_smoke -- "Ho gaya" hi-IN`.

**Developer 2, without D1:**
1. `pnpm dev` in a browser (the mock serves every command; no desktop shell needed).
2. `cargo test -p pluto-intel`.
3. `cargo run -p pluto-intel --example extract_smoke -- test-data/fixtures/meetings/segments.as1.json` (live Gemini).

**Joint:** E2E (`TESTING_STRATEGY.md` §7).

## 5. Checkpoints
| ID | Day | Content | Exit criterion |
|---|---|---|---|
| C0 | 1 | Skeleton, contracts, stubs, plumbing | `main` builds; the UI calls every stub; T-IPC-01, T-LLM-01, T-GN-01 recorded |
| C1 | 2 | Gnani live (Realtime, TTS, http base); LLM router live; velocity check | `rt_smoke`, `tts_smoke`, T-LLM-01 pass |
| C2 | 3 | Import → transcript → extraction → approved tasks | Journey A1 passes up to tasks created |
| C3 | 4 | Assistant B1/B2 with approval; recall | T-AS-05; B1 and B2 pass |
| C4 | 5 noon | All P0a | `TESTING_STRATEGY.md` §7 passes on the demo machine |
| C5 | after C4 | P1 done or cut | Cut list recorded in `HACKATHON_PLAN.md` |
