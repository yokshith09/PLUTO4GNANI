# Feature Roadmap — Pluto Voice

**Team:** exactly 2 developers — **D1** (Developer 1, `Duggineniakhil`) and **D2** (Developer 2, `yokshith09`). **Deadline:** Tue 10 Nov 2026, 11:59 PM IST.
**Status date:** 4 Nov 2026 (Day 1).

This document is the **feature view**: phases → milestones → features, who builds each, and what already exists. The **task view** (acceptance criteria, tests, dependencies, sizes) is the board in [`PROJECT_ROADMAP.md`](PROJECT_ROADMAP.md) §4; day-by-day schedule, risks and cut rules are in [`HACKATHON_PLAN.md`](HACKATHON_PLAN.md). Requirement IDs (MA-, VA-, PP-, PL-) are from [`docs/common/PRD.md`](docs/common/PRD.md). Task IDs (T-xxx) point to the board and are not restated here.

**Status words**

| Word | Meaning |
|---|---|
| **Exists** | In the ported Pluto base, tested, usable as-is |
| **Done** | New work, merged to `main` |
| **In review** | New work, built and open as a pull request |
| **To build** | Nothing written yet |
| **Hidden** | Exists in the base but kept out of the demo build |

---

## 1. Where we are

### 1.1 What exists today
| Capability | Status | Evidence |
|---|---|---|
| Desktop app on Windows and macOS (Tauri shell, tray, hotkeys, notifications, autostart, settings, data export/import) | **Exists** | CI and end-to-end green on GitHub's Windows and macOS runners (4 Nov) |
| Tasks: create, edit, quick-add, status changes, defer, repeat, bin; projects as a task field (PP-01, PP-05) | **Exists** | 492 core tests; end-to-end covers a task |
| Reminders with scheduler and desktop notification (PP-02) | **Exists** | Core tests |
| Notes with labels, pin, archive, bin; `LIKE` search (PP-03) | **Exists** | Core tests; end-to-end covers a note |
| Focus sessions (PP-04) | **Exists** | Core tests |
| Meetings typed by hand: participants, decisions, action items, follow-up date; an action item becomes a task | **Exists** | End-to-end covers a meeting and its action item |
| Desktop pet with bubble | **Exists** | Used later as spoken-confirmation feedback |
| UI mock layer (screens run without the backend) | **Exists** | `src/ipc/mock.ts` |
| CI, end-to-end and release workflows | **Done** | Merged as PR #4; green on both systems |
| Skeleton: four new crates, ownership headers, dependencies, migration slots 012 and 013, `gen:types` and the no-Tauri guard extended (T-003, built by D1) | **Done** | Merged as PR #3; compiles with `--locked`; 43 database tests pass |
| Cleanup: diary page and two unused scripts removed | **Done** | Merged with PR #4; UI tests 468 pass |
| Pluto-only features: time tracking, study pack, wellbeing, character studio | **Hidden** (T-402) | Kept dormant (wired into shared code; see PORTING_FROM_PLUTO §7) |

### 1.2 What does not exist (all of it is new work)
Audio capture and playback · speech-to-text · text-to-speech · any LLM · keychain · network clients · recordings, transcripts, speaker labels and quotes · meeting extraction · voice commands and the approval step · hotkey key-release handling · transcript search · the new commands and screens.

### 1.3 Every feature we are building (summary)
| Pillar | Features | Builder |
|---|---|---|
| Foundation | Contracts, command stubs, UI plumbing, test data, Day-1 checks | D1 and D2 |
| Voice I/O | Mic capture and playback, Prisma Realtime, Timbre speech, Gnani retry layer | D1 |
| Meeting intelligence | Batch transcription and import, speaker mapping, extraction with quotes and dates, review and apply, recap, recall | D1 (Batch), D2 (rest) |
| Voice Assistant | Push-to-talk, rules and intent, tools over the existing task/reminder code, approval gate, spoken replies | D1 (runtime, tools), D2 (rules, UI) |
| Platform | API-key storage, opt-in and privacy, settings screen, safety tests | D1 and D2 |
| Demo | Windows build, freeze, rehearsal, video, post, submission | D1 and D2 |

---

## 2. Phases at a glance
| Phase | Milestones | Goal | When | Checkpoint | Done when |
|---|---|---|---|---|---|
| **A — Foundation** | M0 | Everything needed to build in parallel without collisions | Tue 3 – Wed 4 Nov | C0 | Contracts on `main`, stubs callable, Day-1 checks recorded |
| **B — Core capabilities** | M1, M2, M3 | Voice in and out; a meeting becomes approved tasks; the assistant acts on request | Wed 4 – Sat 7 Nov | C1, C2, C3 | Journeys A1, B1, B2 pass (APPLICATION_FLOWS) |
| **C — Ship** | M4, M6 | A reliable demo, posted and submitted | Sat 7 – Tue 10 Nov | C4 | E2E gate passes; post live; submitted by 6 PM on the 10th |
| **D — Polish** (only if early) | M5 | Live captions, warnings, commitments, comparison, more voice tools | After C4 | C5 | E2E still passes |
| **E — Stretch** (never in the demo build) | M7 | Voice cloning, always-on, loopback capture and similar | After C5 | — | — |

---

## 3. Milestones and their features

### Phase A — Foundation

#### M0 — Base verified, skeleton and contracts · Tue 3 – Wed 4 Nov · Checkpoint C0
**Goal:** both developers can build at the same time against agreed contracts, mocks and test data.

| # | Feature | Status | Builder | Tasks |
|---|---|---|---|---|
| 1 | Run the base on a real Windows machine | In progress (CI is green; local run pending) | D2 | T-001 |
| 2 | CI, end-to-end and release workflows | **Done** (PR #4); D1 still verifies on their machine | D2 imported; D1 verifies | T-002 |
| 3 | Skeleton: crates, placeholders, dependencies, migration slots | **Done** (PR #3) | D1 | T-003 |
| 4 | Contracts A: voice, speech, error types; speech, tool and frame traits | To build | D1 | T-004 |
| 5 | Contracts B: meeting, LLM and decision types; LLM and decider traits; mock LLM | To build | D2 | T-005 |
| 6 | UI plumbing: client wrappers, mock handlers, `voice` page entry, shared fixtures | To build | D2 | T-006 |
| 7 | Command stubs in the shell (one registration edit) | To build | D1 | T-007 |
| 8 | Test data: four synthetic audio sets with ground truth | To build | D2 | T-008 |
| 9 | Day-1 checks: LLM JSON (Gemini, Groq) · Gnani key · mic and binary IPC | To build | D2 · D1 · D1 | T-009 · T-010 · T-011 |
| 10 | Mic capture, frame sender and gapless player in the UI | To build | D1 | T-012 |
| 11 | Rust audio pipe: `audio_push`, frame fan-out, WAV writer, playback channel | To build | D1 | T-013 |

**Done when (C0):** `main` builds; the UI calls every new command against a stub; T-IPC-01, T-LLM-01 and T-GN-01 results are recorded.

### Phase B — Core capabilities

#### M1 — Voice I/O and LLM live · Wed 4 – Thu 5 Nov · Checkpoint C1
**Goal:** speech goes in and out through Gnani; the LLM router works with fallback. Includes the velocity check that re-sizes the plan.

| # | Feature | Status | Builder | Tasks | Requirement |
|---|---|---|---|---|---|
| 1 | Live speech-to-text (Prisma Realtime) with language detection and reconnect | To build | D1 | T-101 | VA-01 |
| 2 | Spoken replies (Timbre) in the detected language | To build | D1 | T-102 | VA-07 |
| 3 | Gnani request layer: one request at a time, retries, error mapping, privacy-log writes | To build | D1 | T-103 | NF |
| 4 | LLM clients (Gemini, Groq) with per-task routing and fallback | To build | D2 | T-104 | — |
| 5 | API-key storage in the OS keychain and key commands | To build | D2 | T-105 | PL-02 |
| 6 | Migration 012 and store: recordings, jobs, transcript segments with full-text search, provenance | To build | D2 | T-106 | — |
| 7 | Migration 013 and store: interactions, proposals, privacy log | To build | D1 | T-107 | PL-03 |

**Done when (C1):** a live clip returns segments; Timbre audio plays; a mocked rate limit on one LLM falls back to the other; velocity check recorded.

#### M2 — Meeting MVP end to end · Thu 5 – Fri 6 Nov · Checkpoint C2
**Goal:** an imported meeting recording becomes a speaker-labelled transcript, approved decisions and action items with quotes, and real tasks.

| # | Feature | Status | Builder | Tasks | Requirement |
|---|---|---|---|---|---|
| 1 | Hand-entered meetings, participants, decisions, action items; action item → task | **Exists** | — | — | base |
| 2 | Batch transcription with speaker separation and code-mixed output; file import (up to 10 MB) | To build | D1 | T-201 | MA-02, MA-03 |
| 3 | Meetings screen: import dialog, status chips, job progress, transcript and speaker tabs | To build | D2 | T-202 | MA-04 |
| 4 | Extraction: decisions, action items, owners, resolved due dates, questions and risks, each linked to its quote; validation and duplicate check | To build | D2 | T-203 | MA-06 |
| 5 | Review and apply: edit, accept, quote jump, creates tasks with the item's own due date; never duplicates | To build | D2 | T-204 | MA-07 |
| 6 | Spoken recap in the chosen language | To build | D2 | T-205 | MA-09 |
| 7 | Integration check C2 on journey A1 | To build | D2 | T-206 | — |
| 8 | Mic recording with consent, WAV chunks and crash recovery (**P0b: cut first, the demo can import**) | To build | D2 | T-207 | MA-01 |

**Done when (C2):** journey A1 passes up to "tasks created" using the staged audio set.

#### M3 — Voice Assistant end to end · Fri 6 – Sat 7 Nov · Checkpoint C3
**Goal:** hold a key, speak, see a proposal, approve it, and the existing task or reminder code does the work; ask about a past meeting and hear a sourced answer.

| # | Feature | Status | Builder | Tasks | Requirement |
|---|---|---|---|---|---|
| 1 | The code the assistant will call: create/update/complete tasks, create reminders (`tasks`, `reminders`) | **Exists** | — | — | PP-01, PP-02 |
| 2 | Pet bubble and desktop notifications for feedback | **Exists** | — | — | base |
| 3 | Push-to-talk on a global hotkey, including key release, with listening/thinking/speaking states, barge-in and "busy" while recording | To build | D1 | T-301 | VA-01 |
| 4 | Rules (yes/no/cancel in Hindi, Tamil, Kannada, English) and intent from the LLM, with dates resolved in Rust | To build | D2 | T-302 | VA-03, VA-07 |
| 5 | Tools over core functions, proposals with a 30-second window, approval by voice or click, six-turn context, interaction log | To build | D1 | T-303 | VA-03, VA-06 |
| 6 | Voice bar, overlay, proposal card, history page | To build | D2 | T-304 | VA-01 |
| 7 | Recall: transcript search, grounded answer with citations, spoken, with a follow-up change as a proposal | To build | D2 | T-305 | MA-08, VA-02 |
| 8 | Integration check C3 on journeys B1 and B2 with latency recorded | To build | D1 | T-306 | — |

**Done when (C3):** push-to-talk → proposal → approval → change works; recall returns a cited answer.

### Phase C — Ship

#### M4 — P0 hardened and demo-ready · Sat 7 – Sun 8 Nov noon · Checkpoint C4
**Goal:** the demo path is reliable, safe and measured on the demo machine.

| # | Feature | Status | Builder | Tasks | Requirement |
|---|---|---|---|---|---|
| 1 | Voice and privacy settings: cloud-voice opt-in with disclosure, keys, languages, voice preview, hotkey | To build | D2 | T-401 | PL-01, PL-02 |
| 2 | Demo build hides non-product pages (time, study, MCP, archive); tracking stays off | To build | D2 | T-402 | — |
| 3 | Safety tests: no write without approval, consent, keys never logged, cloud-off sends nothing | To build | D1 | T-403 | NF |
| 4 | Windows installer, demo machine set up, rollback installer | To build | D1 | T-404 | — |
| 5 | Measurements: accuracy, noise and latency results (**P0b**) | To build | D2 | T-405 | — |
| 6 | End-to-end gate C4 on the demo machine | To build | D2 | T-406 | — |

**Done when (C4, Sunday noon):** the 8-step E2E in TESTING_STRATEGY §7 passes.

#### M6 — Demo, post and submission · Sun 8 – Tue 10 Nov
| # | Feature | Status | Builder | Tasks |
|---|---|---|---|---|
| 1 | Freeze: tag `demo-v1`, archive the installer | To build | D1 | T-601 |
| 2 | Two rehearsals (quiet and noisy take) | To build | D2 | T-602 |
| 3 | 60-second video and the LinkedIn and X post, **live by 9 PM Sunday** | To build | D2 | T-603 |
| 4 | Written explanation, models used, each member's contribution, submission | To build | D1 | T-604 |

**Done when:** posts are live with the required hashtags, and the submission is in before 6 PM Tuesday.

### Phase D — Polish (only if C4 is early; cut in the order of HACKATHON_PLAN §6)

#### M5 — P1 features · After C4 (Mon 9 Nov at the latest)
| # | Feature | Status | Builder | Tasks | Requirement |
|---|---|---|---|---|---|
| 1 | Live captions while recording | To build | D2 | T-501 | MA-05 |
| 2 | Quality warnings (no decisions, no owner, unresolved date) | To build | D2 | T-502 | MA-10 |
| 3 | Commitment tracker across meetings | To build | D2 | T-503 | MA-12 |
| 4 | Transcription comparison against a named baseline | To build | D2 | T-504 | MA-14 |
| 5 | Team glossary as recognition hints | To build | D2 | T-505 | PL-04 |
| 6 | Privacy log and usage counters (backend, then screen) | To build | D1 · D2 | T-506 · T-507 | PL-03, PL-05 |
| 7 | Voice tools for notes and focus; dictation | To build | D1 | T-508 | VA-04 |
| 8 | Recap as an mp3 export | To build | D1 | T-509 | MA-13 |
| 9 | Expressive voice (allowed audio tags) | To build | D2 | T-510 | VA-08 |
| 10 | Failure-injection suite | To build | D1 | T-511 | — |
| 11 | macOS microphone permission and capture check | To build | D1 | T-513 | — |
| 12 | Integration check C5 | To build | D2 | T-512 | — |

### Phase E — Stretch (never merged into the demo build)

#### M7 — P2 features
Projects by voice (D1, T-701) · own-voice recap with consent (D1, T-702) · folder watch (D1, T-703) · Windows system-audio capture (D1, T-704) · always-on wake word (D1, T-705) · pre-meeting briefing (D2, T-706) · follow-up message drafts (D2, T-707) · meeting analytics (D2, T-708) · daily briefing (D2, T-709).

---

## 4. Who builds what

| Area | D1 | D2 |
|---|---|---|
| Foundation | Skeleton (done), contracts A, command stubs, audio pipe, Day-1 checks (Gnani, mic) | Contracts B, UI plumbing, test data, LLM check, workflows |
| Voice I/O | Mic capture and player, Prisma Realtime, Timbre, request layer | — |
| Meeting intelligence | Batch client and import | LLM router, secrets, extraction, review and apply, recap, recall, recorder, migration 012 |
| Voice Assistant | Runtime and hotkey, tools and approval, migration 013 | Rules and intent, voice bar and proposal card |
| Platform and demo | Safety tests, Windows build, freeze, submission | Settings screen, hiding pages, measurements, rehearsal, video and post |

Estimated work before the freeze is about 50 hours for D1 and 49 for D2 against roughly 45 available, so the plan is at its limit. The cut order (P0b, then P1) is in `HACKATHON_PLAN.md` §6; the velocity check at C1 re-sizes it.

## 5. Critical path
`T-003 (done) → contracts T-004/T-005 → stubs and plumbing T-007/T-006 → mic check T-011 → Realtime T-101 → request layer T-103 → Batch T-201 → extraction T-203 → apply T-204 (C2) → runtime T-301 → rules T-302 → assistant T-303 → recall T-305 (C3) → C4 → freeze → post → submit`.

## 6. Explicitly out of scope
Real-time translation (Gnani has none) · joining online calls as a bot · Warp speech-to-speech (no public API) · Gnani Agent Builder (excluded by the rules) · offline speech models (Gnani's are hosted only) · cloning anyone else's voice · recording without a user action · real personal data in the demo.
