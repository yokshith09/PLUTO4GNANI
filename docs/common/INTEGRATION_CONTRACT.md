# Integration Contract — Pluto Voice

**Types, commands and events:** see `API_CONTRACTS.md`. They are not redefined here.

## 1. Components
| Component | Owner | Input | Output | Dependencies | Mock available | Verification |
|---|---|---|---|---|---|---|
| `core::db` + migrations | D1 | — | Tables | rusqlite | In-memory DB | T-DB-01..05 |
| `core` personal services (tasks, reminders + scheduler, notes, focus, projects) | D1 | Commands | Records, `reminder://due`, `focus://tick` | db | — | T-PP-01/02 |
| Personal UI pages | D1 | Commands | Screens | Generated types | `mockApi.ts` | Vitest states |
| `ui/src/audio` (capture worklet, frame sender, player queue) | D2 | Mic; PCM chunks | 1,024-byte frames → `audio_push`; sound | WebView2 | WAV-file source; sine generator | T-IPC-01; unit tests for downsampling |
| `voice::gnani` (Realtime, REST, Batch, TTS, clone, queues) | D1 | Frames, files, text | Segments, jobs, PCM | Gnani key | Fixture WS server + `wiremock` | Contract + T-GN-01 |
| `voice::runtime` (state machine, recorder, sessions) | D1 | Hotkey, commands, frames | `voice://status`, WAV chunks, captions | gnani | `MockStt`, `MockTts` | Transition unit tests |
| `voice::assistant` (tools, proposals, context) | D1 | `Decision` | `Proposal`, `ToolResult` | core, decide | `MockLlm` | T-AS-05, B1/B2 |
| `voice::llm` (Gemini, Groq, router) | D2 | `LlmRequest` | `LlmResponse` | Keys | `MockLlm` | T-LLM-01, T-FH-07/08 |
| `voice::decide` (rules → LLM) | D2 | Transcript | `Decision` | llm | Rules need none | Rule tests, T-ML-04 |
| `voice::meeting` (speakers, extraction, validation, recall, recap, commitments) | D2 | Segments, participants | `ExtractionResult`, `RecallAnswer`, `Commitment` | llm, db, gnani TTS | AS-1 segment fixture | T-ML-02, M4/M5 DoD |
| `app` shell (commands, events, hotkey, tray, keychain) | D1 | IPC | — | all | Stub commands | Types compile; smoke |
| Meetings, Voice and Settings UI + overlay | D2 | Commands, events | Screens | Generated types | `mockApi.ts` | Vitest states; E2E |

## 2. Frozen before parallel work (Checkpoint C0)
1. `types.rs` in `core` and `voice` with every type in API_CONTRACTS §3, exported by `ts-rs`. **D1 writes, D2 reviews.**
2. Traits:
   ```rust
   #[async_trait] trait Llm { async fn complete_json(&self, req: LlmRequest) -> Result<LlmResponse, AppError>; }   // D2
   trait Decider { fn decide(&self, text: &str, ctx: &Context) -> Decision; }                                    // D2
   #[async_trait] trait SpeechToText { async fn open(&self, cfg: RtConfig) -> Result<RtSession, AppError>; }      // D1
   #[async_trait] trait TextToSpeech { async fn stream(&self, req: TtsRequest, out: PcmSink) -> Result<(), AppError>; } // D1
   ```
3. Every IPC command registered as a **stub** returning fixture data (D1), so the UI uses real `invoke()` from day one.
4. `001_init.sql` agreed.

**After C0:** any contract change is a PR that edits `API_CONTRACTS.md` / `DATA_SCHEMA.md` first and is approved by the other developer.

## 3. Mocks
| Mock | Location | Behaviour |
|---|---|---|
| `mockApi.ts` | `ui/src/lib/` | Same signatures as the generated bindings; drives every UI state, including errors; enabled with `VITE_MOCK=1` |
| Stub commands | `app/src/commands/stub.rs` (feature `stub`) | Fixture responses until the real implementation lands |
| `MockStt` / `MockTts` | `voice/src/gnani/mock.rs` | Fixture transcripts with delays and errors (incl. close code 1001); a 1 s sine wave |
| Batch fixtures | `voice/tests/fixtures/batch/` | create, every status, files, `transcript_as1.json` |
| `MockLlm` | `voice/src/llm/mock.rs` | Fixture JSON per task (`intent_*.json`, `extract_as1.json`, `recall_priya.json`); can simulate 429 and timeouts |

Fixtures follow the documented API shapes only. If a live response differs, fix the fixture **and** API_CONTRACTS in the same PR.

## 4. Shared files and conflict rules
| File / area | Owner | Rule |
|---|---|---|
| `Cargo.toml` (workspace) | D1 | D2 lists the dependencies it needs in its PR |
| `app/src/main.rs`, command registration | D1 | D2 never edits these |
| `core/migrations/*` | D1 | D2 reviews |
| `core/src/types.rs`, `voice/src/types.rs` | D1 | Each change approved by D2 |
| Generated TS types | — | Regenerated only in the PR that changes Rust types; never hand-edited |
| `ui/src/App.tsx`, router, sidebar | D2 | D1 adds personal routes through D2's review |
| `ui/src/lib/mockApi.ts` | Shared | Whoever changes a contract updates it in the same PR |
| `tauri.conf.json` (CSP, permissions) | D1 | The CSP never allows external origins |
| `docs/API_CONTRACTS.md`, `docs/DATA_SCHEMA.md` | Both | Both must approve changes |

**Branches:** one per feature (`feat/<id>`) → `develop` (CI green, reviewed by the other developer) → `main` only at checkpoints C4 and C5.

## 5. Independent verification
**Developer 1:**
1. `cargo test -p core -p voice`.
2. `cargo run -p voice --example rt_smoke -- test-audio/as4_01.wav`.
3. `--example batch_smoke -- test-audio/as1.wav`.
4. `--example tts_smoke -- "Ho gaya" hi-IN`.
5. Personal pages with `VITE_MOCK=1`.

**Developer 2:**
1. `VITE_MOCK=1 pnpm dev` (all meeting, voice and settings states).
2. `cargo test -p voice llm:: decide:: meeting::`.
3. `cargo run -p voice --example extract_smoke -- voice/tests/fixtures/batch/transcript_as1.json` (live Gemini).
4. Audio page with the WAV-file source.

**Joint:** E2E (TESTING_STRATEGY §7).

## 6. Checkpoints
| ID | Content | Exit criterion |
|---|---|---|
| C0 | Scaffold + contracts + stubs | App builds; UI calls stub commands; T-IPC-01 run |
| C1 | Gnani live (Realtime, TTS); LLM router live; task/reminder backend | `rt_smoke`, `tts_smoke`, T-LLM-01 pass |
| C2 | Recording → Batch → segments → extraction on a real transcript | A1 works through to the review screen |
| C3 | Assistant B1/B2 with approval; tasks/reminders UI done | T-AS-05 + B1/B2 manual pass |
| C4 | All P0 | TESTING_STRATEGY §7 passes on the demo machine |
| C5 | P1 done or cut | Cut list recorded in `HACKATHON_PLAN.md` (repo root) |
