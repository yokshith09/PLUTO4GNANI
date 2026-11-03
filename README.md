# Pluto Voice — Hackathon Workspace

A new standalone Windows desktop app with two parts:
- a **Meeting Assistant** (primary);
- a **Voice Assistant** that operates the whole app, including its own tasks, reminders, notes, focus sessions and projects.

It is built on Gnani **Prisma** (speech-to-text) and **Timbre** (text-to-speech), with Gemini and Groq for reasoning. The existing Pluto app is not modified.

**Status:** documentation complete; implementation not started. **Current focus:** M0 (see [`PROJECT_ROADMAP.md`](PROJECT_ROADMAP.md)).

## Start here
| File | Purpose |
|---|---|
| [`PROJECT_ROADMAP.md`](PROJECT_ROADMAP.md) | Audit, milestones, epics, **task board** (single source for tasks), Definition of Done, criteria → evidence |
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
| Desktop shell | Tauri v2 (Rust) |
| UI | React 18 + TypeScript + Vite + Tailwind + shadcn/ui |
| Audio | **Hybrid:** mic capture and playback in the UI (WebView2 `getUserMedia`, AudioWorklet, Web Audio); everything else in Rust |
| Rust | crates `types`, `core`, `voice` (D1) and `intel` (D2); tokio, reqwest, tokio-tungstenite, rusqlite (SQLite + FTS5), hound, keyring, ts-rs |
| Speech | Gnani Prisma v3.0 (Realtime, Batch, REST), Timbre v3.0 |
| Reasoning | Gemini (meetings) and Groq (voice commands), each the other's fallback |
| Decisions | Rules first, then the LLM |
| Tests | cargo test, wiremock, Vitest + Testing Library |
