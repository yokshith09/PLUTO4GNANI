# Pluto Voice — Hackathon Workspace

A Windows desktop app with two parts:
- a **Meeting Assistant** (primary);
- a **Voice Assistant** that operates the whole app, including its own tasks, reminders, notes, focus sessions and projects.

It is built on Gnani **Prisma** (speech-to-text) and **Timbre** (text-to-speech), with Gemini and Groq for reasoning. It is built on a base ported from the owner's earlier Pluto app (see [`docs/common/PORTING_FROM_PLUTO.md`](docs/common/PORTING_FROM_PLUTO.md)); the voice, meeting and LLM layers are new.

**Status (4 Nov):** the Pluto base is ported; CI and end-to-end are green on GitHub's Windows and macOS runners; Developer 1's skeleton (PR #3) and the imported workflows (PR #4) are merged. The plan is re-baselined on that base. **Branches:** `main`, `D1`, `D2` only. **Current focus:** M0 (see [`PROJECT_ROADMAP.md`](PROJECT_ROADMAP.md)).

## Start here
| File | Purpose |
|---|---|
| [`PROJECT_ROADMAP.md`](PROJECT_ROADMAP.md) | **What exists now**, milestones, epics, **task board** (single source for tasks), effort summary, Definition of Done, criteria → evidence |
| [`FEATURE_ROADMAP.md`](FEATURE_ROADMAP.md) | **Feature view:** phases → milestones → features, who builds each, what exists and what is to build |
| [`docs/dev1/START_HERE.md`](docs/dev1/START_HERE.md) · [`docs/dev2/START_HERE.md`](docs/dev2/START_HERE.md) | **Start point per developer:** sync, setup, first tasks, owned files |
| [`HACKATHON_PLAN.md`](HACKATHON_PLAN.md) | Team allocation, dependency order, timeboxed milestones, PR/integration cadence, risks, cut rules, critical path |

## Documentation layout
```
docs/
├── common/   shared by both developers (authoritative contracts live here)
│   ├── PRD.md                  problem, requirements, decisions, open items
│   ├── ARCHITECTURE.md         components, hybrid audio, Gnani, LLM router, privacy, risks
│   ├── API_CONTRACTS.md        ★ single source: external APIs, types, commands, events, errors
│   ├── DATA_SCHEMA.md          ★ single source: tables and migrations
│   ├── UI_UX_SPEC.md           all screens and states
│   ├── APPLICATION_FLOWS.md    journeys and failure paths
│   ├── TESTING_STRATEGY.md     Day-1 checks, audio sets, tests, E2E gate
│   ├── WORKING_AGREEMENT.md    ★ ownership map, shared files, branches/PRs, contract changes
│   ├── PORTING_FROM_PLUTO.md   what could be ported from Pluto, why it is deferred, and the rules
│   ├── INTEGRATION_CONTRACT.md components, frozen-before-parallel order, mocks, checkpoints
│   └── PPT_FRAMEWORK.md        placeholder (supplied separately)
├── dev1/     Developer 1: platform, Gnani I/O, runtime, personal module
│   ├── FEATURE_SPECS_DEV1.md
│   └── TASKS_DEV1.md
└── dev2/     Developer 2: UI audio, LLM, meeting intelligence, voice UX
    ├── FEATURE_SPECS_DEV2.md
    └── TASKS_DEV2.md
```

## Tech stack
| Layer | Choice |
|---|---|
| Desktop shell | Tauri v2 (Rust 1.97.0), from the Pluto base |
| UI | React 18 + TypeScript + Vite + Tailwind 4 + Radix UI, from the Pluto base |
| Audio | **Hybrid:** mic capture and playback in the UI (WebView2 `getUserMedia`, AudioWorklet, Web Audio); everything else in Rust |
| Rust | Existing `pluto-core`; new `pluto-contracts`, `pluto-gnani`, `pluto-voice` (D1) and `pluto-intel` (D2); tokio, rusqlite (SQLite; FTS5 is new), ts-rs; new dependencies: reqwest, tokio-tungstenite, hound, keyring |
| Speech | Gnani Prisma v3.0 (Realtime, Batch, REST), Timbre v3.0 |
| Reasoning | Gemini (meetings) and Groq (voice commands), each the other's fallback |
| Decisions | Rules first, then the LLM |
| Tests | cargo test, Vitest + Testing Library (base); wiremock for Gnani contract tests (new) |
