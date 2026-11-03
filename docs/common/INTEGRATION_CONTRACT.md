# Integration Contract — Pluto Voice

**Types, traits, commands and events:** `API_CONTRACTS.md`. **Ownership, shared files, branches, PR rules, contract changes:** `WORKING_AGREEMENT.md`. Neither is repeated here.

## 1. Components
| Component | Owner | Input | Output | Dependencies | Mock available | Verification |
|---|---|---|---|---|---|---|
| `core::db` + migrations | D1 | — | Tables | rusqlite | In-memory DB | T-DB-01..05 |
| `core` personal services (tasks, reminders + scheduler, notes, focus, projects) | D1 | Commands | Records, `reminder://due`, `focus://tick` | db | — | T-PP-01/02 |
| `ui/src/features/personal` pages | D1 | Commands | Screens | Generated types | `mock/personal.ts` | Vitest states |
| `ui/src/audio` (capture worklet, frame sender, player queue) | D2 | Mic; PCM chunks | 1,024-byte frames → `audio_push`; sound | WebView2 | WAV-file source; sine generator | T-IPC-01; downsampler unit tests |
| `voice::gnani` (Realtime, REST, Batch, TTS, clone, queues) | D1 | Frames, files, text | Segments, jobs, PCM | Gnani key | `MockStt`, `MockTts`, fixture WS server, `wiremock` | Contract tests + T-GN-01 |
| `voice::runtime` (state machine, recorder, sessions) | D1 | Hotkey, commands, frames | `voice://status`, WAV chunks, captions | gnani | `MockStt`, `MockTts` | Transition unit tests |
| `voice::assistant` (tools, proposals, context) | D1 | `Decision` from a `Decider` | `Proposal`, `ToolResult` | core, `Decider` | `MockDecider` | T-AS-05, B1/B2 |
| `intel::llm` (Gemini, Groq, router) | D2 | `LlmRequest` | `LlmResponse` | Keys | `MockLlm` | T-LLM-01, T-FH-07/08 |
| `intel::decide` (rules → LLM) | D2 | Transcript | `Decision` | llm | Rules need none | Rule tests, T-ML-04 |
| `intel::meeting` (speakers, relevance, extraction, validation, dates, recall, recap, commitments) | D2 | Segments, participants | `ExtractionResult`, `RecallAnswer`, `Commitment` | llm, core, `TextToSpeech` | AS-1 segment fixture, `MockTts` | T-ML-02, M4/M5 DoD |
| `app` shell (registry, hotkey, tray, keychain) | D1 | IPC | — | all crates | Stub commands | Types compile; smoke |
| Meetings, Voice and Settings UI + overlay | D2 | Commands, events | Screens | Generated types | `mock/{meetings,voice,settings}.ts` | Vitest states; E2E |

## 2. What is frozen before parallel work (checkpoint C0)
In this order. Each item is merged to `develop` before anything that depends on it starts.

| Step | Task | Content | Writer → approver |
|---|---|---|---|
| 1 | T-001a | Workspace skeleton: root `Cargo.toml` with glob members, empty crates `types`, `core`, `voice`, `intel`, all `types/src/*.rs` module files (empty) with `lib.rs` listing them, `.gitignore`, CI placeholder | D1 → D2 |
| 2 | T-003 | `ids`, `time`, `error`, `personal`, `voice`, `speech` types + traits `SpeechToText`, `TextToSpeech`, `ToolExecutor` | D1 → D2 |
| 2 | T-003b | `meeting`, `llm`, `decide` types + traits `Llm`, `Decider`, with `MockLlm` | D2 → D1 |
| 3 | T-005 | `001_init.sql` + migration runner | D1 → D2 |
| 4 | T-006 | All IPC commands registered as stubs returning fixtures; sample events | D1 → D2 |
| 5 | T-007 | `client.ts`, `mock/*.ts` skeleton (`personal.ts` left empty for D1) | D2 → D1 |
| 6 | T-014 / T-015 | Contract CI checks; fixture round-trip tests | D1 / D2 |

**After C0:** every change goes through a contract PR (`WORKING_AGREEMENT.md` §5).

## 3. Mocks and fixtures
| Mock | Location | Behaviour |
|---|---|---|
| Client wrapper | `ui/src/lib/api/client.ts` | The only code that calls `invoke` / `listen`. Screens import from here |
| Mock API | `ui/src/lib/mock/` | Same function signatures as `client.ts`; every UI state including errors; enabled with `VITE_MOCK=1` |
| Stub commands | `app/src/commands/*.rs` (feature `stub`) | Return fixture data until the real implementation lands |
| `MockStt` / `MockTts` | `voice/src/gnani/mock.rs` | Fixture transcripts with delays and errors (incl. close code 1001); a 1 s sine wave |
| `MockLlm` | `intel/src/llm/mock.rs` | Fixture JSON per task; can simulate 429 and timeouts |
| `MockDecider` | `voice/src/assistant/mock.rs` | Returns scripted `Decision`s so the assistant can be tested without `intel` |
| Fixtures | `test-data/fixtures/` | One shared set. Named `<type>.<case>.json`, with `contract_version`. Used by UI mocks **and** Rust tests (T-015) |

Fixtures follow the documented API shapes only. If a live response differs, fix the fixture **and** `API_CONTRACTS.md` in a contract PR.

## 4. Independent verification
**Developer 1, without D2:**
1. `cargo test -p types -p core -p voice`.
2. `cargo run -p voice --example rt_smoke -- test-data/audio/as4_01.wav`.
3. `cargo run -p voice --example batch_smoke -- test-data/audio/as1.wav`.
4. `cargo run -p voice --example tts_smoke -- "Ho gaya" hi-IN`.
5. Personal pages with `VITE_MOCK=1`.

**Developer 2, without D1:**
1. `VITE_MOCK=1 pnpm --dir ui dev` (all meeting, voice and settings states).
2. `cargo test -p intel`.
3. `cargo run -p intel --example extract_smoke -- test-data/fixtures/meetings/segments.as1.json` (live Gemini).
4. The audio page with the WAV-file source (in a browser, no desktop shell needed).

**Joint:** E2E (`TESTING_STRATEGY.md` §7).

## 5. Checkpoints
| ID | Content | Exit criterion |
|---|---|---|
| C0 | Skeleton + contracts + stubs + mocks | App builds; UI calls stub commands; T-IPC-01 run |
| C1 | Gnani live (Realtime, TTS); LLM router live; task/reminder backend | `rt_smoke`, `tts_smoke`, T-LLM-01 pass |
| C2 | Recording → Batch → segments → extraction on a real transcript | A1 works through to the review screen |
| C3 | Assistant B1/B2 with approval; tasks/reminders UI done | T-AS-05 + B1/B2 manual pass |
| C4 | All P0 | `TESTING_STRATEGY.md` §7 passes on the demo machine |
| C5 | P1 done or cut | Cut list recorded in `HACKATHON_PLAN.md` |
