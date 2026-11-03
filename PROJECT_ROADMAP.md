# Project Roadmap — Pluto Voice

**Team:** exactly 2 developers (Developer 1 = **D1**, Developer 2 = **D2**).

**Sources:**
- Shared documents: [`docs/common/`](docs/common/).
- Developer 1 specs and tasks: [`docs/dev1/`](docs/dev1/).
- Developer 2 specs and tasks: [`docs/dev2/`](docs/dev2/).
- Schedule and cadence: [`HACKATHON_PLAN.md`](HACKATHON_PLAN.md).

## Current Focus
> **Active milestone: M0 — Foundation and contracts.**
> Exit criteria:
> - the app builds;
> - the shared types are frozen (T-003, T-003b);
> - every command is stubbed (T-006) and the mock API is in place (T-007);
> - the contract CI checks run (T-014, T-015);
> - the Day-1 checks (T-IPC-01, T-LLM-01, T-GN-01) have been run and recorded.
>
> **First hour:** D1 merges T-001a (workspace skeleton). D2 starts T-002, T-008 and T-012, which need nothing from D1.
> Nothing in M1 or later starts until M0 exits, except tasks marked "parallel-safe" in §4.

---

## 1. Audit (as of 3 Nov 2026)
| Area | State | Evidence |
|---|---|---|
| Source code | **Not started** | No application files exist; the workspace is not a git repository |
| Git / CI | **Not started** | `git status` → "not a git repository" |
| Documentation | **Complete** | 11 documents in `docs/common`, `docs/dev1`, `docs/dev2` |
| Decisions | **Approved** | Standalone Tauri v2 hybrid app; Gnani Prisma + Timbre; Gemini/Groq split by job; rules → LLM; full personal scope in tiers (PRD §10) |
| External unknowns | **Open, non-blocking** | G-1..G-4 (Gnani), L-1 (LLM limits), each with a fallback (ARCHITECTURE §13) |
| Blocked work | **None** | — |
| Team skills | **Unknown** | Ownership is assigned by dependency and integration boundaries, not by skill. Re-balance at checkpoint C1 if needed |

## 2. Milestones (ordered by user value, judging impact, dependency order, uncertainty and demo importance)
| # | Milestone | Why it's in this position | Exit checkpoint |
|---|---|---|---|
| **M0** | Foundation and contracts | Everything depends on it. It removes the biggest technical unknowns first (WebView2 mic, LLM JSON, Gnani key) | C0 |
| **M1** | Live voice I/O and services | Proves Gnani is central; needed by both main journeys | C1 |
| **M2** | **Meeting MVP end to end** (record → transcript → speakers → extraction → approved tasks) | Primary feature; strongest Mixed Vibes and Real Deal evidence | C2 |
| **M3** | **Voice Assistant end to end** (push-to-talk → intent → approval → action; meeting recall) | Second demo pillar; depends on M1 and partly on M2 | C3 |
| **M4** | P0 complete and hardened (personal P0 pages, settings and privacy, failure handling, E2E gate) | A reliable demo needs this before any P1 work | C4 |
| **M5** | P1 features (notes, focus, commitments, ticker, comparison, glossary, export) | Polish and extra evidence; cut first | C5 |
| **M6** | Demo: freeze, rehearse, record, post, submit | Shortlist engagement and submission | Submitted |
| **M7** | P2 stretch (optional) | Only after C5 is clean; never risks the demo build | — |

## 3. Epics
| Epic | Name | Milestones | Owner of record |
|---|---|---|---|
| E0 | Repository, CI and contracts | M0 | D1 |
| E1 | Audio I/O (hybrid) | M0–M1 | D2 (UI audio), D1 (Rust side) |
| E2 | Gnani clients | M1–M2 | D1 |
| E3 | LLM layer and decisions | M0–M3 | D2 |
| E4 | Meeting pipeline and UI | M2, M5 | D2 (intelligence + UI), D1 (recording + batch) |
| E5 | Voice Assistant | M3 | D1 (runtime + tools), D2 (UX + recall) |
| E6 | Personal productivity | M2–M5, M7 | D1 |
| E7 | Settings, privacy and reliability | M4 | D1 (backend), D2 (UI) |
| E8 | Demo and submission | M6 | D2 (demo), D1 (build + submission) |

## 4. Board

**Estimates:** S ≤ 2 h · M ≈ half a day · L ≈ 1 day · XL > 1 day (split before starting).
**Status:** every task is **To do**, because no code exists yet.
**Parallel-safe:** the task depends only on frozen contracts or mocks.

### M0 — Foundation and contracts
| ID | Story / Feature | Task → deliverable | Owner | Priority | Dependencies | Est. | Acceptance criteria | Tests | Status |
|---|---|---|---|---|---|---|---|---|---|
| T-001a | E0 Repo | **Workspace skeleton:** root `Cargo.toml` (glob members `app`, `crates/*`), empty crates `types`, `core`, `voice`, `intel`, all `types/src/*.rs` module files with `lib.rs` listing them, `.gitignore`. **Merge within the first hour** | D1 | P0 | — | S | `cargo build` succeeds on the empty workspace; D2 can add code under `crates/intel` without editing any shared file | `cargo build` | To do |
| T-001b | E0 Repo | Tauri v2 `app` shell (`ui/` is **not** touched; the dev server is whatever D2 creates), CI running fmt, clippy, cargo test, lint, typecheck, vitest, no-`tauri`-outside-`app` check → green `main` | D1 | P0 | T-001a, T-002 | M | `pnpm tauri dev` opens a window showing the UI; CI green on the first PR | CI run | To do |
| T-002 | E0 UI shell | Create `ui/`: Vite + React + TS + Tailwind; **port Pluto's theme and 12 base components (PRD D-13)**, add other shadcn components with the CLI; router, sidebar (all pages as placeholders), voice bar placeholder | D2 | P0 | — (parallel-safe; `ui/` is D2-owned and runs in the browser) | M | Every sidebar route renders; lint and typecheck clean | Vitest render test per route | To do |
| T-003 | E0 Contracts | `crates/types`: `ids`, `time`, `error`, `personal`, `voice`, `speech` + traits `SpeechToText`, `TextToSpeech`, `ToolExecutor`; `ts-rs` export script (`pnpm gen:types`) into `ui/src/lib/api/generated/` | D1 | P0 | T-001a | M | Generated TS compiles in `ui`; **D2 approves the PR** | `cargo test` export; `pnpm typecheck` | To do |
| T-003b | E3 Contracts | `crates/types/src/{meeting,llm,decide}.rs`: meeting types (API_CONTRACTS §3.3, §3.4), `LlmRequest`/`LlmResponse`, traits `Llm` and `Decider`, `DecideContext`; each with a mock | D2 | P0 | T-001a | M | Types compile; generated TypeScript compiles in `ui`; **D1 approves the PR** | `cargo test` export; `pnpm typecheck` | To do |
| T-004 | E3 Contracts | Create `crates/intel`; `MockLlm` with fixtures (`intent_*.json`, `extract_as1.json`, `recall_priya.json`) | D2 | P0 | T-001a, T-003b | S | Traits compile; MockLlm returns fixtures and can simulate 429 and timeouts; **D1 approves** | Unit | To do |
| T-005 | E0 Schema | `001_init.sql` per DATA_SCHEMA + migration runner | D1 | P0 | T-001a | M | Applies on an empty DB; `user_version` = 1 | T-DB-01..05 | To do |
| T-006 | E0 Stubs | Register every IPC command (API_CONTRACTS §4) in `app/src/commands/registry.rs` as a stub returning fixture data; emit sample events | D1 | P0 | T-003, T-003b | M | The UI can `invoke` every command and receive every event | Smoke test invoking each command | To do |
| T-007 | E0 Mocks | `ui/src/lib/api/client.ts` (the only `invoke`/`listen` caller) + `ui/src/lib/mock/{meetings,voice,settings,index}.ts` with the same signatures, every state including errors; empty `mock/personal.ts` for D1; `VITE_MOCK=1` switch | D2 | P0 | T-003, T-003b | M | Each screen can be driven through loading, empty, error and success | Vitest | To do |
| T-008 | E8 Test data | Scripts + ground truth for AS-1..AS-4 (synthetic) | D2 | P0 | — (parallel-safe) | M | Ground-truth files list the expected items and intents | Review by D1 | To do |
| T-009 | E1 Rust audio IPC | `audio_push` (raw binary body + `x-session-id`) → in-memory sink + WAV writer; playback `Channel<Vec<u8>>` that sends a test tone | D1 | P0 | T-001a | M | 1,024-byte frames are received in order; the tone plays in the UI | Unit + T-IPC-01 | To do |
| T-010 | E1 UI audio | Capture worklet (device rate → 16 kHz PCM16, 1,024-byte frames) + sender; gapless player queue with `stop()` | D2 | P0 | T-002 | L | Frames hold the correct sample count; playback has no gaps between chunks | Unit test of the downsampler; T-IPC-01 | To do |
| T-011 | E1 Day-1 check | **T-IPC-01** joint run: 10 s capture → Rust → WAV plays back; tone playback. Record the result. **If it fails:** open T-011F (`cpal` capture, D1, L) | D1 | P0 | T-009, T-010 | S | Result recorded in `docs/results/` | T-IPC-01 | To do |
| T-012 | E3 Day-1 check | **T-LLM-01:** minimal Gemini + Groq adapters; schema-valid JSON on 5 prompts each; record free-tier limits; choose model IDs | D2 | P0 | — (parallel-safe; standalone script first, adapters move into `intel` after T-004) | M | 10/10 valid JSON; limits and model IDs recorded in settings defaults | T-LLM-01 | To do |
| T-013 | E2 Day-1 check | **T-GN-01:** `examples/` for Gnani REST clip, Realtime clip and TTS; record credits used | D1 | P0 | T-001a | S | All three succeed with the team key | T-GN-01 | To do |
| T-014 | E0 Contract CI | CI job `contracts`: (a) regenerate TypeScript types and fail on any diff; (b) compare command names in API_CONTRACTS §4, `registry.rs`, `client.ts` and every `mock/*.ts`; (c) fail if a crate other than `app` depends on `tauri`; (d) fail if a fixture's `contract_version` ≠ `CONTRACT_VERSION` | D1 | P0 | T-006, T-007 | M | Deliberately renaming a command makes CI fail | The CI job itself, shown failing then passing | To do |
| T-015 | E3 Fixture tests | Rust tests in `intel` and `core`: every file in `test-data/fixtures/` deserializes into its Rust type; `JSON → type → JSON` round-trip | D2 | P0 | T-003, T-003b, T-008 | S | A fixture with a wrong field name fails `cargo test` | T-CT-01 | To do |

### M1 — Live voice I/O and services
| ID | Story / Feature | Task → deliverable | Owner | Priority | Dependencies | Est. | Acceptance criteria | Tests | Status |
|---|---|---|---|---|---|---|---|---|---|
| T-101 | E2 Realtime | Prisma Realtime client: headers, frames, `commit`, reconnect, rotation at 14 min → `TranscriptSegment` stream | D1 | P0 | T-013, T-003 | L | Live clip gives segments with `detected_language_code`; simulated close code 1001 reconnects | Contract (fixture WS), T-FH-04 | To do |
| T-102 | E2 TTS | Timbre WS client → base64 → PCM 24 kHz → playback Channel; REST mp3 helper | D1 | P0 | T-013, T-009 | M | "Ho gaya" in hi-IN plays within the measured first-audio time | Contract; audible check | To do |
| T-103 | E2 Reliability | Request queues (1 in flight per REST family), retry/backoff, Gnani error → `AppError` mapping, privacy-log write on every call | D1 | P0 | T-005 | M | A 429 → 200 sequence succeeds silently; 402 → `InsufficientCredit` | T-FH-02, T-FH-03, T-PR-02 | To do |
| T-104 | E3 Router | LLM router per task (Intent: Groq → Gemini; others: Gemini → Groq), timeouts, one JSON-repair attempt, `fell_back` flag | D2 | P0 | T-012, T-004 | M | A mocked 429 on the primary is answered by the fallback; both failing → `LlmUnavailable` | T-FH-07, T-FH-08 | To do |
| T-105 | E7 Keys and settings | Keychain (Gnani, Gemini, Groq) + settings service + `voice_settings_*` / `voice_key_*` commands | D1 | P0 | T-005 | M | Keys survive a restart and never appear in the DB or logs | T-PR-01 | To do |
| T-106 | E6 Tasks backend | Task service + commands (`tasks_list`, `task_create`, `task_update`, `task_delete`) | D1 | P0 | T-005 | M | Views Today/Upcoming/All/Done return the correct sets | T-PP-01 (backend part) | To do |
| T-107 | E6 Reminders | Reminder service + scheduler + OS notification + missed-on-startup; commands | D1 | P0 | T-005 | M | Fires within 2 s of `remind_at`; a missed reminder fires once on startup | T-PP-01 | To do |

### M2 — Meeting MVP end to end
| ID | Story / Feature | Task → deliverable | Owner | Priority | Dependencies | Est. | Acceptance criteria | Tests | Status |
|---|---|---|---|---|---|---|---|---|---|
| T-201 | E4 Recording | Recorder: frames → WAV chunks (≤ 4.5 min, flush 30 s), consent enforcement, pause/resume/stop, crash recovery; `meeting_rec_*` | D1 | P0 | T-011, T-005 | L | A 6-min recording gives 2 chunks with correct offsets; no consent → `ConsentRequired` | T-PR-03, T-FH-09 | To do |
| T-202 | E4 Captions | Recorder fans frames out to Realtime → `meeting://caption` | D1 | P0 | T-101, T-201 | S | Captions appear while recording; with the network off, recording continues | T-FH-01 | To do |
| T-203 | E4 Batch | Batch client: create/start/poll/files/transcript → segments with chunk offsets → status `NEEDS_SPEAKERS`; `meeting_import` (≤ 10 MB); `meeting_job_retry` | D1 | P0 | T-103, T-005 | L | AS-1 gives speaker-labelled segments in the DB; `PARTIAL_FAILURE` fixture gives gap markers | Contract (wiremock lifecycle), T-ML-01, T-FH-06 | To do |
| T-204 | E4 UI record | Meetings list, Record dialog (consent required), live view with captions, level meter, job progress | D2 | P0 | T-007 | L | Works on mocks in every state; consent checkbox gates Start | Vitest states | To do |
| T-205 | E4 Speakers | `meeting_transcript_get`, `meeting_speakers_set` (Rust) + Transcript and Speakers tabs | D2 | P0 | T-003; T-203 for live data (fixture until then) | M | Mapping updates every segment with that label; status → `NEEDS_REVIEW` | Unit + Vitest | To do |
| T-206a | E4 Relevance | Relevance prompt + schema; batches of ≤ 40 segments (Gemini) | D2 | P0 | T-104 | M | Every AS-1 segment gets a label | Unit with MockLlm; live smoke | To do |
| T-206b | E4 Extraction | Extraction prompt + schema + Rust validator (segment ref, owner ∈ participants, date resolution, duplicates via FTS) → `meeting_extract` | D2 | P0 | T-206a, T-005 | L | Items without a valid segment are dropped; "Friday tak" resolves against the meeting date | Unit (validator, date resolver); T-ML-02 | To do |
| T-207 | E4 Apply | Review tab (edit/accept/reject, quote → transcript jump, duplicate badge) + `meeting_extraction_apply` creating tasks via the task service | D2 | P0 | T-206b, T-106 | L | Applying AS-1 creates tasks linked through `source_action_item_id`; re-applying never duplicates | Unit + Vitest | To do |
| T-208 | E4 Recap | `meeting_recap` (speak) from applied items in the chosen language | D2 | P0 | T-102, T-104 | S | Hindi recap plays | Live smoke | To do |
| T-209 | Integration **C2** | Run journey A1 on a real AS-1 recording; record result | D2 | P0 | T-201..T-208 | S | A1 passes up to tasks created | E2E steps 2–4 | To do |

### M3 — Voice Assistant end to end
| ID | Story / Feature | Task → deliverable | Owner | Priority | Dependencies | Est. | Acceptance criteria | Tests | Status |
|---|---|---|---|---|---|---|---|---|---|
| T-301 | E5 Runtime | State machine, global hotkey, `voice_ptt_start/stop`, `commit`, barge-in, `Busy` while recording, `voice://status` | D1 | P0 | T-101, T-102, T-011 | L | Every transition in ARCHITECTURE §5 is covered; hotkey → `Listening` shown ≤ 100 ms | Transition unit tests; T-LAT-01 | To do |
| T-302 | E3 Decisions | Rules (hi/ta/kn/en yes/no/cancel/stop/focus) + Intent prompt/schema → `Decision` | D2 | P0 | T-104 | M | Rule set covers the listed phrases; AS-4 routing ≥ 90% | Unit; T-ML-04 | To do |
| T-303 | E5 Tools | Tool registry (P0 tools), proposals (store, 30 s expiry), approval via rules or click, execution, 6-turn context, `voice_interactions` timings | D1 | P0 | T-301, T-302, T-106, T-107 | L | No write without approval; "usse Friday kar do" resolves the previous task | T-AS-05; B1 manual | To do |
| T-304 | E5 Overlay | Voice bar/overlay states, proposal card (approve/reject/edit, countdown), Voice history page | D2 | P0 | T-007 | M | All states render on mocks; voice and click approval both work live | Vitest; B1 manual | To do |
| T-305 | E5 Recall | `meeting_search` (FTS + filters, top 8) + grounded answer with citations + follow-up proposal hook | D2 | P0 | T-206b, T-104 | L | Answer cites a real segment; no match → "Not found in your meetings" | Unit (grounding); B2 manual | To do |
| T-306 | Integration **C3** | Run journeys B1 and B2 live; record latency | D1 | P0 | T-303..T-305 | S | B1/B2 pass; T-LAT-03/04 recorded | B1, B2, T-LAT | To do |

### M4 — P0 complete and hardened
| ID | Story / Feature | Task → deliverable | Owner | Priority | Dependencies | Est. | Acceptance criteria | Tests | Status |
|---|---|---|---|---|---|---|---|---|---|
| T-402 | E6 Pages | Tasks page (tabs, drawer) + Reminders page + Today page | D1 | P0 | T-106, T-107, T-002 | L | All states on mocks, then live; voice-created items appear without reload | Vitest | To do |
| T-403 | E7 Settings UI | Opt-in disclosure, keys, languages, caption style, voice preview, hotkey, model IDs | D2 | P0 | T-105 | M | Cloud voice off → assistant disabled; keys show "Saved" only | Vitest; T-PR-04 | To do |
| T-404 | E7 Safety tests | T-PR-01..04, T-AS-05 automated; consent at UI, command and DB levels | D1 | P0 | T-303, T-201 | S | All pass | Listed | To do |
| T-405 | E7 Failure suite | T-FH-01..09 run and fixed | D1 | P0 | C3 | M | All pass or have a documented fallback | Listed | To do |
| T-406 | E8 Measurement | Run T-ML-01..04, T-NZ, T-LAT; write `docs/results/` | D2 | P0 | C3 | M | Results recorded with commit and machine | Listed | To do |
| T-407 | E8 Build | Windows installer build, demo-machine setup, rollback installer kept | D1 | P0 | C3 | M | Fresh install on the demo machine passes the voice test | Manual | To do |
| T-408 | Integration **C4** | E2E acceptance (TESTING_STRATEGY §7) on the demo machine | D2 | P0 | T-402..T-407 | S | All 8 steps pass | E2E §7 | To do |

### M5 — P1 (cut in the order given in HACKATHON_PLAN §6)
| ID | Story / Feature | Task → deliverable | Owner | Priority | Dependencies | Est. | Acceptance criteria | Tests | Status |
|---|---|---|---|---|---|---|---|---|---|
| T-501 | E6 Notes (**may port from Pluto after C4**; see PRD D-12) | Notes service (FTS) + page + `note_create` tool + dictation (`codemix`/`transliterate`) | D1 | P1 | C4 | L | Dictated Hinglish note saved and searchable | Unit + Vitest | To do |
| T-502 | E6 Focus (**may port from Pluto after C4**; see PRD D-12) | Focus service + `focus://tick` + page + voice tools | D1 | P1 | C4 | M | Timer accurate within 1 s over 5 min | T-PP-02 | To do |
| T-503 | E4 Export | Recap mp3 export per language (Timbre REST) | D1 | P1 | T-102 | S | mp3 plays externally | Manual | To do |
| T-504 | E7 Privacy UI backend | Privacy log + usage counters commands | D1 | P1 | T-103 | S | Counters move after each call | Unit | To do |
| T-505 | E4 Warnings | Quality warnings (banner + spoken) | D2 | P1 | T-206b | S | AS-1 variant with no owner shows the warning | Unit | To do |
| T-506 | E4 Commitments | `commitments_list` + Voice → Commitments tab | D2 | P1 | T-207 | M | Overdue / Done / NoTask computed correctly | Unit + Vitest | To do |
| T-507 | E4 Ticker | Live ticker (relevance over caption windows) | D2 | P1 | T-202, T-206a | M | Items labelled "draft" appear during recording | Manual | To do |
| T-508 | E4 Comparison | `compare_transcribe` + diff view; choose and document the baseline | D2 | P1 | T-101 | M | Reproducible diff on a named clip | T-CMP-01 | To do |
| T-509 | E7 UI | Glossary, privacy log and usage UI | D2 | P1 | T-504 | S | All render live | Vitest | To do |
| T-510 | E5 Tags | Audio-tag allow-list in reply composer (expressive voice) | D2 | P1 | T-102 | S | Only allow-listed tags are sent | Unit | To do |
| T-511 | Integration **C5** | Re-run E2E §7 with P1 features; record the cut list | D2 | P1 | T-501..T-510 or cuts | S | E2E still passes | E2E §7 | To do |

### M6 — Demo and submission
| ID | Story / Feature | Task → deliverable | Owner | Priority | Dependencies | Est. | Acceptance criteria | Tests | Status |
|---|---|---|---|---|---|---|---|---|---|
| T-601 | E8 Freeze | Freeze the demo build; tag `demo-v1`; keep the rollback installer | D1 | P0 | C4 (C5 if done) | S | Tag exists; installer archived | — | To do |
| T-602 | E8 Rehearsal | Demo run-through ×2 on the frozen build (quiet + noisy take) | D2 | P0 | T-601 | M | Two clean runs | E2E §7 | To do |
| T-603 | E8 Video + post | Record the 60 s video; post on LinkedIn + X with the required hashtags and disclosures | D2 | P0 | T-602 | M | Posts live; links saved | — | To do |
| T-604 | E8 Submission | Written explanation, models used, each member's contribution, nominated post links | D1 | P0 | T-603 | S | Form submitted; confirmation saved | — | To do |

### M7 — P2 stretch (only after C5; separate branches; never merged into `demo-v1`)
| ID | Task | Owner | Est. | Dependencies |
|---|---|---|---|---|
| T-701 | Projects service + page + voice tools (PP-05, VA-05) | D1 | L | C5 |
| T-702 | Own-voice recap via voice cloning, with consent (MA-16) | D1 | M | T-102 |
| T-703 | Folder watch (MA-19) | D1 | M | T-203 |
| T-704 | WASAPI loopback capture (MA-20) | D1 | L | T-201 |
| T-705 | Always-on wake word on device (VA-10) | D1 | XL | T-301 |
| T-706 | Pre-meeting briefing (MA-15) | D2 | M | T-506 |
| T-707 | Follow-up drafts (MA-17) | D2 | S | T-207 |
| T-708 | Meeting analytics (MA-18) | D2 | S | T-205 |
| T-709 | Daily briefing (VA-09) | D2 | S | T-402 |

## 5. Dependency graph (critical path in bold)
```
FIRST HOUR
  T-001a (D1, skeleton) ──┬── T-003 (D1 types) ───┐
                          ├── T-003b (D2 types) ──┼── T-006 stubs (D1) ─┐
                          ├── T-005 schema (D1)   │   T-007 client+mocks (D2) ── T-014 contract CI (D1)
                          └── T-004 MockLlm (D2)  │
  parallel-safe from minute 0:  T-002 UI shell (D2) ── T-010 audio (D2) · T-008 test data (D2) · T-012 LLM check (D2)
                                T-009 audio IPC (D1) · T-013 Gnani check (D1) · T-001b app shell (D1, needs T-002)

CRITICAL PATH
  T-009 + T-010 ── **T-011** mic/IPC check ── **T-201** recorder ── T-202 captions
  T-013 ── **T-101** Realtime ── **T-203** Batch (needs T-103) ── T-205 speakers
  T-012 ── T-104 router ── T-206a ── **T-206b** extraction ── **T-207** apply (needs T-106) ── T-209 (C2)
  T-101 + T-102 ── **T-301** runtime ── **T-303** tools (needs T-302, T-106, T-107) ── T-306 (C3)
  T-206b ── T-305 recall ── T-306 (C3)
  C3 ── T-402..T-407 ── **T-408** (C4) ── **T-601** freeze ── T-602 ── T-603 ── T-604
```

## 6. Definition of Done (every task)
1. Code merged to `develop` through a reviewed PR with all merge checks green (`docs/common/WORKING_AGREEMENT.md` §4).
2. The task's tests are written and passing. UI tasks are checked on mocks in every state.
3. The acceptance criteria are verified by the owner. Live-service tasks are verified with a real call once.
4. Contracts are unchanged, or a contract PR (`WORKING_AGREEMENT.md` §5) was merged first and approved by both developers.
   Fixtures and mocks match the contract (CI-checked).
5. No secrets in code, logs or fixtures. Synthetic data only.
6. Board status updated.

## 7. Official criteria → evidence
| Criterion (official) | Feature | Test / evidence | Point in the 60-second demo |
|---|---|---|---|
| Genuine use of Gnani technology (eligibility) | Realtime captions, Batch transcript, Timbre replies | T-GN-01, T-ML-01 | 0–10 s captions; 10–25 s transcript; every spoken reply |
| Mixed Vibes (primary) | Code-mixed meeting → items; Hindi recall answer | T-ML-01..04, T-CMP-01 | 10–25 s extraction from Hinglish; 30–45 s spoken Hindi answer with quote |
| Real Deal | Meeting → approved tasks; voice reminder | T-ML-02, E2E §7 | 25–30 s tasks created; 45–55 s reminder |
| Noise Canceller (secondary) | Noisy take | T-NZ-01/02 | Shown only if results support it |
| Jugaad Genius (secondary) | Expressive voice (audio tags) | T-510 | Final confirmation line |
| Demo Day Drop | Whole P0 flow | E2E §7, T-602 | Entire video |
| Shortlist (verified engagement) | Public post | T-603 | Post live early (HACKATHON_PLAN §3) |
| Rules (synthetic data, no misleading demo) | Consent, opt-in, disclosures | T-PR-01..04 | Disclosure text in the post |

Presentation material is out of scope for this roadmap. The only presentation file is the placeholder [`docs/common/PPT_FRAMEWORK.md`](docs/common/PPT_FRAMEWORK.md).
