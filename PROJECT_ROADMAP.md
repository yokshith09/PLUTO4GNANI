# Project Roadmap — Pluto Voice

**Team:** exactly 2 developers (Developer 1 = **D1**, Developer 2 = **D2**).
**Branches:** `main`, `D1`, `D2` only (`docs/common/WORKING_AGREEMENT.md` §4).
**Schedule, cadence, risks, cut rules:** [`HACKATHON_PLAN.md`](HACKATHON_PLAN.md). **Per-developer lists:** [`docs/dev1/TASKS_DEV1.md`](docs/dev1/TASKS_DEV1.md), [`docs/dev2/TASKS_DEV2.md`](docs/dev2/TASKS_DEV2.md).

## Current Focus
> **Active milestone: M0 — Base verified, skeleton and contracts.**
> - **D2:** T-001 (verify on Windows) tonight, then T-009, T-008, T-005, T-006, T-105, T-106.
> - **D1:** T-002 tonight if possible; then **T-003 (merge within the first hour of Day 1)** → T-004 → T-007 / T-013 / T-010 / T-012.
> - Exit criteria (C0): the shared contracts are on `main`, every new command is stubbed and callable from the UI, the Day-1 checks (T-IPC-01, T-LLM-01, T-GN-01) are recorded.
> Nothing in M1 or later starts until C0, except tasks marked parallel-safe.

---

## 1. Audit — what exists now (3 Nov 2026)
**Source of the code:** the ported Pluto base (`docs/common/PORTING_FROM_PLUTO.md`). Verified in a Linux sandbox: `pluto-core` 492 tests pass; UI typecheck clean, 486 tests pass. The Tauri shell has **not** been built or run (needs Windows).

### Implemented (in the base, tested, to be reused as-is)
| Area | What exists |
|---|---|
| Tasks | Create, edit, quick-add, status transitions (`PLANNED / IN_PROGRESS / PENDING / COMPLETED / CANCELLED`), defer, repeat, plans, history, bin; `project` field (Projects page is derived from it) |
| Reminders | Create, list, done, delete; scheduler; OS notification; shown on the pet |
| Notes | CRUD, labels, pin, archive, bin; search is `LIKE` (no full-text index) |
| Focus | Start, pause, extend, stop, finish; per-task length |
| Meetings (manual) | `meetings`, participants, decisions, action items, `follow_up_date`; `convert_action_item` turns an item into a task (title = item, project = meeting title, due = the meeting's follow-up date) |
| Settings | Settings and prefs service; data export/import/delete; retention |
| Shell | Tray, notifications, autostart, single instance, window state, global shortcuts (3 actions, **key press only**), 99 IPC commands registered in one list in `src-tauri/src/lib.rs` |
| UI | Pages: Today, Tasks, Time, Notes, Projects, Study, Diary, Meetings, Archive, Bin, Character, MCP, Settings; page switching by state (no router); typed IPC client with an in-memory **mock** (`src/ipc/mock.ts`); one event channel `pluto://event` |
| Pluto-only features | Desktop pet, characters, diary, courses and exams, app/website usage tracking (off by default) |

### Not present (everything below is new work)
Audio capture or playback; speech-to-text; text-to-speech; any LLM; HTTP or WebSocket client; OS keychain; full-text search; recordings, transcripts, segments, provenance (quotes); extraction; voice runtime; approval gate; hotkey key-release handling; CI (not ported); the `voice` and `meeting_ai` commands.

### In progress
Nothing. No new feature code has been written.

### Blocked
Nothing. Open Gnani questions (credits, request limits, streaming hints, presigned upload) have fallbacks and do not block (ARCHITECTURE §13).

### Unverified
The Tauri shell, tray, hotkeys, notifications, autostart and tracking on Windows. Pluto's own status file lists 42 such items.

## 2. Milestones
| # | Milestone | Why here | Checkpoint |
|---|---|---|---|
| M0 | Base verified, skeleton, contracts | Everything depends on it; removes the biggest unknowns first (mic in WebView2, LLM JSON, Gnani key) | C0 (Day 1) |
| M1 | Voice I/O and LLM live | Proves Gnani is central | C1 (Day 2) |
| M2 | Meeting MVP end to end | Primary feature; strongest Mixed Vibes and Real Deal evidence | C2 (Day 3) |
| M3 | Voice Assistant end to end | Second pillar; needs M1 and part of M2 | C3 (Day 4) |
| M4 | P0 hardened, demo build | Reliable demo before any P1 | C4 (Day 5 noon) |
| M5 | P1 | Polish; cut first | C5 |
| M6 | Demo, post, submission | Shortlist engagement and submission | Submitted (Day 7) |
| M7 | P2 | Optional; never in the demo build | — |

## 3. Epics
| Epic | Name | Milestones | Owner |
|---|---|---|---|
| E0 | Verification, skeleton, contracts | M0 | D1 (skeleton, contracts A), D2 (contracts B, UI plumbing) |
| E1 | Audio I/O (hybrid) | M0–M1 | D1 |
| E2 | Gnani clients | M1–M2 | D1 |
| E3 | LLM layer and decisions | M1–M3 | D2 |
| E4 | Meeting pipeline and UI | M2, M5 | D2 (intelligence, UI, recorder), D1 (batch) |
| E5 | Voice Assistant | M3 | D1 (runtime, tools), D2 (decisions, UI, recall) |
| E6 | Settings, privacy, reliability | M4 | D1 (safety, build), D2 (settings UI) |
| E7 | Demo and submission | M6 | D2 (demo), D1 (freeze, submission) |

## 4. Board
**Estimates:** S ≈ 1 h, M ≈ 2.5 h, L ≈ 5.5 h of focused work; XL is split before starting. These are estimates, not measurements.
**Priority:** **P0a** = demo spine, **P0b** = needed for the full demo but cut first, **P1**, **P2**.
**Status:** every task is **To do**.
**Parallel-safe:** depends on no one else's unmerged work.

### M0 — Base verified, skeleton and contracts (Day 0–1)
| ID | Story / Feature | Task → deliverable | Owner | Priority | Dependencies | Est. | Acceptance criteria | Tests | Status |
|---|---|---|---|---|---|---|---|---|---|
| T-001 | Base | Verify the base on Windows: `pnpm install`, `pnpm tauri dev`, walk the checklist, record results in `docs/results/base-verification.md`, list every failure with its cause | D2 | P0a | — | S | App opens; Today, Tasks, Notes, Meetings load; a reminder notification fires; failures listed | Manual checklist | To do |
| T-002 | Base | Verify on D1's machine. Read the first runs of the imported workflows (`ci.yml`, `e2e.yml`, `release.yml` from Pluto; CI also runs on pushes to `D1` and `D2`) and fix any failure. Keep the base's floors green: `pluto-core` ≥ 90% lines; frontend 80 / 80 / 70 / 55 | D1 | P0a | — | M | CI and Windows e2e green on `main`; D1's machine passes `pnpm tauri dev` | CI and e2e runs | To do |
| T-003 | Skeleton | New crates `pluto-contracts`, `pluto-gnani`, `pluto-voice`, `pluto-intel` in the workspace; module placeholders (one file per owner); extend `gen:types` to the new crates; workspace deps (reqwest, tokio-tungstenite, hound, keyring, async-trait); pre-register migrations 012 and 013 in `migrate.rs`; add the `mod` lines for `meeting_ai_store` and `voice_store` to `pluto-core/src/lib.rs`. **Merge to `main` within the first hour of Day 1** | D1 | P0a | — | M | `cargo build --workspace` and `pnpm gen:types` succeed; D2 can add files in `pluto-intel` and `pluto-gnani/src/{batch,rest}.rs` without editing any shared file | cargo build; CI | To do |
| T-004 | Contracts A | `pluto-contracts`: voice runtime, speech, proposal and tool types; `VoiceError`; traits `SpeechToText`, `TextToSpeech`, `ToolExecutor`, `FrameSink`; mocks | D1 | P0a | T-003 | M | Types compile and export TypeScript to `src/ipc/generated`; **D2 approves** | Export test; typecheck | To do |
| T-005 | Contracts B | `pluto-contracts`: meeting, LLM and decision types; traits `Llm`, `Decider`; `MockLlm` with fixtures | D2 | P0a | T-003 | M | Same; **D1 approves** | Export test; typecheck | To do |
| T-006 | UI plumbing | `src/ipc/voiceClient.ts`, `meetingAi.ts`, mock handlers composed into `createMock`, `voice://` event listeners, `voice` entry in `Page` / `App.tsx` / Sidebar, folders `src/features/voice`; shared fixtures in `test-data/fixtures` | D2 | P0a | T-004, T-005 | M | Every new command can be called from the UI against the mock; the Voice page opens | Vitest | To do |
| T-007 | Shell stubs | `src-tauri/src/voice.rs` and `meeting_ai.rs`: every new command as a stub returning fixture data; all added to `generate_handler!` in **one** edit; `voice://` event helper | D1 | P0a | T-004, T-005 | S | The UI can invoke every new command in the real shell | Smoke test | To do |
| T-008 | Test data | Scripts and ground truth for AS-1..AS-4 (synthetic); recorded audio in `test-data/audio` | D2 | P0a | — | M | Ground truth lists expected items, owners, dates and intents | Review by D1 | To do |
| T-009 | LLM check | T-LLM-01: standalone script; schema-valid JSON on 5 prompts each from Gemini and Groq; record free-tier limits; choose model IDs | D2 | P0a | — | S | 10 of 10 valid; limits and model IDs recorded | T-LLM-01 | To do |
| T-010 | Gnani check | T-GN-01: examples for a REST clip, a Realtime clip and TTS; record credits used | D1 | P0a | — | S | All three succeed with the team key | T-GN-01 | To do |
| T-011 | Mic check | T-IPC-01: 10 s capture → Rust → WAV plays back; tone playback; result recorded. If it fails: `cpal` capture in Rust | D1 | P0a | T-012, T-013 | S | Result recorded in `docs/results/` | T-IPC-01 | To do |
| T-012 | UI audio | `src/audio`: capture worklet (→ 16 kHz PCM16, 1,024-byte frames), frame sender, gapless player with `stop()` | D1 | P0a | — | L | Frames carry the right sample count; no gaps between chunks | Unit (downsampler) | To do |
| T-013 | Rust audio IPC | `audio_push` (raw body + `x-session-id`), `FrameSink` fan-out, WAV writer, playback Channel with a test tone | D1 | P0a | T-004 | M | Ordered 1,024-byte frames received; tone audible | Unit; T-IPC-01 | To do |

### M1 — Voice I/O and LLM live (Day 1–2)
| ID | Story / Feature | Task → deliverable | Owner | Priority | Dependencies | Est. | Acceptance criteria | Tests | Status |
|---|---|---|---|---|---|---|---|---|---|
| T-101 | Prisma Realtime | Realtime client: headers, frames, `commit`, reconnect, rotation at 14 min → segment stream | D1 | P0a | T-010, T-004 | L | A live clip gives segments with `detected_language_code`; a simulated close code 1001 reconnects | Contract (fixture WS); T-FH-04 | To do |
| T-102 | Timbre | Timbre WebSocket client → PCM 24 kHz → playback Channel; REST helper | D1 | P0a | T-010, T-013 | M | "Ho gaya" in hi-IN plays | Contract; audible check | To do |
| T-103 | Gnani base | `pluto-gnani` HTTP base: request queue (1 in flight), retry/backoff, error mapping to `VoiceError`, privacy-log write. **Needed by T-201, so early** | D1 | P0a | T-003 | M | A 429 → 200 sequence succeeds silently; 402 → `InsufficientCredit` | T-FH-02, T-FH-03 | To do |
| T-104 | LLM | Gemini and Groq clients, router per task, timeouts, one JSON-repair attempt, fallback with `fell_back` flag | D2 | P0a | T-009, T-005 | M | A mocked 429 on the primary is answered by the fallback; both failing → `LlmUnavailable` | T-FH-07, T-FH-08 | To do |
| T-105 | Secrets | `SecretStore` (OS keychain) for the Gnani, Gemini and Groq keys; key commands; `voice.*` / `llm.*` settings keys through Pluto's settings service | D2 | P0a | T-003 | S | Keys survive a restart; never in the database or logs | T-PR-01 | To do |
| T-106 | Migration 012 | `012_meeting_ai.sql` + `meeting_ai_store`: recordings, jobs, segments + FTS5, provenance, pipeline status (DATA_SCHEMA) | D2 | P0a | T-003 | M | Chain applies; FTS5 query returns inserted segments; delete cascades | T-DB-01..04 | To do |
| T-107 | Migration 013 | `013_voice.sql` + `voice_store`: interactions, proposals, privacy log | D1 | P0a | T-003 | S | Chain applies; constraints hold | T-DB-05 | To do |

### M2 — Meeting MVP end to end (Day 2–3)
| ID | Story / Feature | Task → deliverable | Owner | Priority | Dependencies | Est. | Acceptance criteria | Tests | Status |
|---|---|---|---|---|---|---|---|---|---|
| T-201 | Batch | `pluto-gnani/src/batch.rs`: create/start/poll/files/transcript; `meeting_import` (≤ 10 MB); segments with offsets into `meeting_ai_store`; job poller and `meeting://job` events; retry | D1 | P0a | T-103, T-106 | L | AS-1 gives speaker-labelled segments in the database; a `PARTIAL_FAILURE` fixture gives gap markers | Contract (wiremock); T-ML-01; T-FH-06 | To do |
| T-202 | Meetings UI | Extend the Meetings page: Import dialog, status chips, job progress, Transcript and Speakers tabs; `meeting_transcript_get`, `meeting_speakers_set` | D2 | P0a | T-006, T-106 | L | Mapping updates every segment with that label; works on mocks, then live | Vitest; manual | To do |
| T-203 | Extraction | Prompt + schema (relevance and extraction in one pass), Rust validator (segment ref, owner ∈ participants), date resolver, duplicate check against open tasks | D2 | P0a | T-104, T-005 | L | Items without a valid segment are dropped; "Friday tak" resolves against the meeting date | Unit (validator, dates); T-ML-02 | To do |
| T-204 | Review and apply | Review tab (edit, accept, quote jump, duplicate badge) + `meeting_extraction_apply`: write via core `meetings::update`, provenance, `convert_action_item`, then set the item's due date with `tasks::update` | D2 | P0a | T-203, T-202 | L | Applying AS-1 creates linked tasks with the right due dates; re-applying never duplicates | Unit; Vitest | To do |
| T-205 | Recap | `meeting_recap`: recap in the chosen language from applied items, spoken through `TextToSpeech` | D2 | P0a | T-102, T-104 | S | Hindi recap plays | Live smoke | To do |
| T-206 | Integration C2 | Run journey A1 on the import path with AS-1; record the result | D2 | P0a | T-201..T-205 | S | A1 passes through to tasks created | E2E steps 2–4 | To do |
| T-207 | Recorder | Mic recording: `FrameSink` → ≤ 4.5-min WAV chunks, consent enforcement, pause/stop, crash recovery, record dialog, `meeting_rec_*` | D2 | P0b | T-013, T-106 | L | A 6-min recording gives 2 chunks with correct offsets; no consent → `ConsentRequired` | T-PR-03; T-FH-09 | To do |

### M3 — Voice Assistant end to end (Day 3–4)
| ID | Story / Feature | Task → deliverable | Owner | Priority | Dependencies | Est. | Acceptance criteria | Tests | Status |
|---|---|---|---|---|---|---|---|---|---|
| T-301 | Voice runtime | State machine, push-to-talk on a global hotkey **including key release** (`shortcuts.rs`: new `VoicePtt` action), `voice://status`, barge-in, `Busy` while recording | D1 | P0a | T-101, T-102, T-011 | L | Every transition in ARCHITECTURE §5 covered; hotkey → Listening shown ≤ 100 ms | Transition tests; T-LAT-01 | To do |
| T-302 | Decisions | Rules table (hi/ta/kn/en yes/no/cancel/stop/focus) + intent LLM prompt and schema + date resolution → `Decision` with resolved args | D2 | P0a | T-104, T-005 | M | Rules cover the listed phrases; AS-4 routing ≥ 90% | Unit; T-ML-04 | To do |
| T-303 | Assistant | Tool registry over core functions, proposals (30 s), approval by voice or click, 6-turn context, interaction log. P0 tools: tasks list/create/complete/update due, reminders list/create/done, meeting search | D1 | P0a | T-301, T-302, T-107 | L | No write without an approved proposal; "usse Friday kar do" resolves the previous task | T-AS-05; B1 manual | To do |
| T-304 | Voice UI | Voice bar and overlay states, proposal card (approve/reject/edit, countdown), Voice history page | D2 | P0a | T-006 | M | All states render on mocks; voice and click approval both work live | Vitest; B1 manual | To do |
| T-305 | Recall | `meeting_recall`: FTS search + filters (top 8) + grounded answer with citations + follow-up proposal hook | D2 | P0a | T-203, T-106 | L | Answer cites a real segment; no match → "Not found in your meetings" | Unit (grounding); B2 manual | To do |
| T-306 | Integration C3 | Run journeys B1 and B2 live; record latency | D1 | P0a | T-303..T-305 | S | B1 and B2 pass; T-LAT-03/04 recorded | B1, B2, T-LAT | To do |

### M4 — P0 hardened and demo-ready (Day 4–5 noon)
| ID | Story / Feature | Task → deliverable | Owner | Priority | Dependencies | Est. | Acceptance criteria | Tests | Status |
|---|---|---|---|---|---|---|---|---|---|
| T-401 | Voice settings | "Voice & privacy" section in Settings: opt-in disclosure, keys, languages, caption style, voice preview, hotkey (shortcut row), model IDs | D2 | P0a | T-105 | M | Cloud voice off → assistant disabled; keys show "Saved" only | Vitest; T-PR-04 | To do |
| T-402 | Demo build | Hide non-product pages (time, study, diary, character, mcp, archive) behind one setting; app tracking stays off | D2 | P0a | T-006 | S | The demo build shows only Today, Tasks, Notes, Projects, Meetings, Voice, Settings | Vitest | To do |
| T-403 | Safety | Automate T-AS-05 (no write without approval), T-PR-01..04 (keys, privacy log, consent, cloud-off = zero outbound) | D1 | P0a | T-303 | S | All pass | Listed | To do |
| T-404 | Build | Windows installer (`pnpm tauri build`), demo-machine setup, rollback installer kept | D1 | P0a | T-306 | S | Fresh install on the demo machine passes the voice test | Manual | To do |
| T-405 | Measurement | Run T-ML-01..04, T-NZ, T-LAT; write `docs/results/` | D2 | P0b | T-306 | M | Results recorded with commit and machine | Listed | To do |
| T-406 | Integration C4 | E2E acceptance (TESTING_STRATEGY §7) on the demo machine | D2 | P0a | T-401..T-404 | S | All steps pass | E2E §7 | To do |

### M5 — P1 (only if M4 is early; otherwise after the post)
| ID | Story / Feature | Task → deliverable | Owner | Priority | Dependencies | Est. | Acceptance criteria | Tests | Status |
|---|---|---|---|---|---|---|---|---|---|
| T-501 | Live captions | Fan recorder frames to Realtime → `meeting://caption`; captions pane with native/romanised toggle | D2 | P1 | T-207, T-101 | M | Captions appear while recording; offline → recording continues | T-FH-01 | To do |
| T-502 | Warnings | Quality warnings (no decisions, no owner, unresolved date), banner + spoken | D2 | P1 | T-203 | S | AS-1 variant with no owner shows the warning | Unit | To do |
| T-503 | Commitments | `commitments_list` + Voice → Commitments tab | D2 | P1 | T-204 | M | Overdue / Done / NoTask computed correctly | Unit; Vitest | To do |
| T-504 | Comparison | `compare_transcribe` + diff view; choose and document a baseline | D2 | P1 | T-101 | M | Reproducible diff on a named clip | T-CMP-01 | To do |
| T-505 | Glossary | Team glossary → Prisma `keyterms`; UI | D2 | P1 | T-106 | S | Terms reach every Prisma request | Unit | To do |
| T-506 | Privacy log | Privacy-log and usage-counter commands (backend) | D1 | P1 | T-103 | S | Counters move after each call | Unit | To do |
| T-507 | Privacy UI | Privacy log and usage in Settings | D2 | P1 | T-506 | S | Renders live | Vitest | To do |
| T-508 | More voice tools | Notes (`notes::create/search`), focus (`focus::start/stop`), dictation in `codemix`/`transliterate` | D1 | P1 | T-303 | M | Dictated Hinglish note saved and found by search | Unit | To do |
| T-509 | mp3 export | Recap mp3 export per language | D1 | P1 | T-102 | S | mp3 plays externally | Manual | To do |
| T-510 | Audio tags | Allow-list of Timbre tags in the reply composer | D2 | P1 | T-102 | S | Only allow-listed tags are sent | Unit | To do |
| T-511 | Failure suite | Run and fix T-FH-01..09 | D1 | P1 | T-306 | M | All pass or have a documented fallback | T-FH-* | To do |
| T-512 | Integration C5 | Re-run E2E §7 with the P1 features that are in; record the cut list | D2 | P1 | T-501..T-511 or cuts | S | E2E still passes | E2E §7 | To do |
| T-513 | macOS mic | macOS microphone support: add `NSMicrophoneUsageDescription` to `src-tauri/Info.plist` and check WKWebView capture on a Mac. CI already compiles and smoke-tests macOS; capture cannot be tested there | D1 | P1 | T-012 | S | Microphone permission prompt appears and capture works on a Mac | Manual on a Mac | To do |

### M6 — Demo, post and submission (Day 5–7)
| ID | Story / Feature | Task → deliverable | Owner | Priority | Dependencies | Est. | Acceptance criteria | Tests | Status |
|---|---|---|---|---|---|---|---|---|---|
| T-601 | Freeze | Tag `demo-v1` on `main`; archive the installer | D1 | P0a | T-406 | S | Tag exists; installer archived | — | To do |
| T-602 | Rehearsal | Two clean run-throughs on the frozen build (quiet and noisy take) | D2 | P0a | T-601 | M | Two clean runs | E2E §7 | To do |
| T-603 | Video and post | Record the 60-second video; post on LinkedIn and X with the required hashtags and disclosures | D2 | P0a | T-602 | M | Posts live; links saved | — | To do |
| T-604 | Submission | Written explanation, models used, each member's contribution, nominated post links; submit | D1 | P0a | T-603 | S | Form submitted; confirmation saved | — | To do |

### M7 — P2 stretch (never in the demo build)
| ID | Task | Owner | Est. | Dependencies |
|---|---|---|---|---|
| T-701 | Voice tools for projects (derived from `tasks.project`) | D1 | M | T-303 |
| T-702 | Own-voice recap with voice cloning (consent required) | D1 | M | T-102 |
| T-703 | Folder watch for new recordings | D1 | M | T-201 |
| T-704 | WASAPI loopback capture (Windows) | D1 | L | T-207 |
| T-705 | Always-on mode with an on-device wake word | D1 | L | T-301 |
| T-706 | Pre-meeting briefing | D2 | M | T-503 |
| T-707 | Follow-up message drafts per owner | D2 | S | T-204 |
| T-708 | Meeting analytics (talk time) | D2 | S | T-202 |
| T-709 | Daily briefing | D2 | S | T-402 |


## 5. Effort summary (estimates)
| | D1 | D2 |
|---|---|---|
| P0a before the demo freeze (M0–M4) | 50 h | 49 h |
| P0a after the freeze (M6: freeze, rehearsal, post, submission) | 2 h | 5 h |
| P0b (cut first) | 0 h | 8 h |
| P1 | 8 h | 12 h |
| P2 | 18 h | 6 h |
| P0a task count (L / M / S) | 5 / 6 / 9 | 4 / 10 / 7 |

**Reading this:** the P0a work before the freeze is about 50 h for D1 and 49 h for D2. The time available from Day 1 to the freeze (Day 5 noon) is roughly 45 h per developer at long hackathon days **[Guessing]**. The plan is therefore at its limit, and P0b is expected to be used as the cut line. **At C1, compare actual time to these estimates and re-size the remaining P0a** (HACKATHON_PLAN §6).

## 6. Dependency graph (critical path in bold)
```
DAY 0   T-001 (D2) · T-002 (D1)
DAY 1   **T-003 (D1, first hour)** ──┬── T-004 (D1) ──┬── T-007 stubs (D1) ──┐
                                     ├── T-005 (D2) ──┤                      ├── C0
                                     │                └── T-006 UI plumbing (D2) ┘
                                     ├── T-106 migration 012 (D2) · T-107 migration 013 (D1) · T-105 secrets (D2)
        parallel-safe: T-008 · T-009 (D2) · T-010 · T-012 (D1)
        T-012 + T-013 ── **T-011 mic check**
DAY 2   T-010 ── **T-101 Realtime** · T-102 Timbre · **T-103 Gnani base** (D1)       T-009 ── T-104 LLM router (D2)
DAY 3   T-103 + T-106 ── **T-201 Batch + import** (D1) ── T-202 Meetings UI / speakers (D2)
        T-104 ── **T-203 extraction** ── **T-204 review + apply** ── T-206 (C2)        T-205 recap
DAY 4   T-101 + T-102 + T-011 ── **T-301 runtime** ── **T-303 assistant** (needs T-302, T-107) ── T-306 (C3)
        T-203 ── **T-305 recall** ── T-306 (C3) · T-304 voice UI
DAY 5   T-401..T-404 ── **T-406 (C4)** ── **T-601 freeze** ── T-602 ── **T-603 post** ── T-604 submit
```

## 7. Definition of Done (every task)
1. Merged to `main` through a reviewed PR from your branch, with CI green (`docs/common/WORKING_AGREEMENT.md` §4).
2. The task's tests are written and pass. UI tasks are checked on the mock in every state.
3. The acceptance criteria are verified by the owner. Live-service tasks are verified with one real call.
4. Contracts are unchanged, or a contract PR (`WORKING_AGREEMENT.md` §5) merged first with both developers' approval.
5. No secrets in code, logs or fixtures. Synthetic data only.
6. The board row is updated.

## 8. Official criteria → evidence
| Criterion (official) | Feature | Test / evidence | Point in the 60-second demo |
|---|---|---|---|
| Genuine use of Gnani technology (eligibility) | Batch transcript, Realtime voice input, Timbre replies | T-GN-01, T-ML-01 | Transcript on screen; every spoken reply |
| Mixed Vibes (primary) | Code-mixed meeting → items; spoken Hindi answer with quote | T-ML-01..04 | 10–25 s extraction from Hinglish; 30–45 s spoken answer with citation |
| Real Deal | Meeting → approved tasks; reminder by voice | T-ML-02, E2E §7 | 25–30 s tasks created; 45–55 s reminder |
| Noise Canceller (secondary) | Noisy take | T-NZ-01/02 | Only if the results support it |
| Jugaad Genius (secondary) | Expressive voice (audio tags, P1) | T-510 | Final confirmation line |
| Demo Day Drop | Whole P0 flow | E2E §7, T-602 | Entire video |
| Shortlist (verified engagement) | Public post | T-603 | Post live on Day 5 |
| Rules (synthetic data, no misleading demo) | Consent, opt-in, disclosures | T-PR-01..04 | Disclosure text in the post |

Presentation material is out of scope here. The only presentation file is the placeholder [`docs/common/PPT_FRAMEWORK.md`](docs/common/PPT_FRAMEWORK.md).
