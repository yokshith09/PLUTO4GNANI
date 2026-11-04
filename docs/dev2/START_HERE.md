# Developer 2 — Start Here

You own: LLM, meeting intelligence, UI, demo. Branch: `D2`.
Full task list: [`TASKS_DEV2.md`](TASKS_DEV2.md). Specs: [`FEATURE_SPECS_DEV2.md`](FEATURE_SPECS_DEV2.md). Rules: [`../common/WORKING_AGREEMENT.md`](../common/WORKING_AGREEMENT.md).

## 1. What exists today (all on `main`)
- Ported Pluto base: `pluto-core` (tasks, reminders, notes, focus, meetings, SQLite), Tauri shell, React UI (`src/ipc/client.ts`, `src/ipc/mock.ts`), CI, e2e and release workflows.
- D1's skeleton (T-003, merged): crates `pluto-contracts`, `pluto-gnani`, `pluto-voice`, `pluto-intel` with empty module placeholders; migrations 012 and 013 registered but empty.
- Nothing yet calls Gemini, Groq or Gnani. All meeting, LLM and decision types are still to build.

## 2. Sync your branch
```bash
git fetch origin '+refs/heads/*:refs/remotes/origin/*'
git checkout D2
git merge origin/main
```

## 3. Environment
- Node 22 (`.nvmrc`), pnpm 10.28, Rust 1.97 (pinned), WebView2 on Windows.
- `cp .env.example .env` and fill `GEMINI_API_KEY` and `GROQ_API_KEY` (needed for T-009). Keys in the app go to the OS keychain.
- Verify: `pnpm install`, `pnpm test`, `cargo test -p pluto-core -p pluto-intel`, then `pnpm tauri dev` (this is T-001 on your machine).

## 4. First tasks, in order
1. **T-001** — build and run the shell locally on Windows; record the result.
2. **T-009** — prove Gemini and Groq return valid JSON for the extraction schema; record limits and 429 behaviour.
3. **T-008** — secrets storage (keychain) in the shell.
4. **T-005** — meeting, LLM and decision types in `crates/pluto-contracts/src/{meeting,llm,decide}.rs`; traits `Llm`, `Decider`; `MockLlm` in `pluto-intel`. Use `#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]`, then `pnpm gen:types`.
5. **T-006** — `src/ipc/voiceClient.ts`, `meetingAi.ts`, mock handlers in `src/ipc/mock.ts` (keyed by command name), the `voice` page entry, fixtures in `test-data/fixtures/`.
6. **T-105, T-106** — as listed in `TASKS_DEV2.md`.

Waits on: T-006 needs D1's T-004 and T-007 (command names/stubs). Until then build against mocks; `pnpm dev` runs the UI without the shell.

## 5. Files
- Yours: `pluto-intel`, `pluto-contracts` meeting/llm/decide, migration 012, `src-tauri/src/meeting_ai.rs`, secrets, UI pages (Meetings, Voice, Settings), test data.
- Do not edit: D1's files (`pluto-gnani`, `pluto-voice`, voice/speech/error contracts, migration 013, `src/audio`, workflows). Hot files and owners: WORKING_AGREEMENT §2.

## 6. Daily rhythm
Morning: merge `origin/main`. Work on `D2`. PR into `main` at least daily (merge commit, not squash); D1 reviews within 2 working hours. If `main` is red, fix it first. Contract changes go only through a PR labelled `contract`.

## 7. Ready checklist
- [ ] Branch synced, `pnpm test` passes
- [ ] `.env` has Gemini and Groq keys
- [ ] Read WORKING_AGREEMENT §2, §4, §5
