# Developer 1 — Start Here

You own: platform, audio, Gnani, voice runtime, assistant. Branch: `D1`.
Full task list: [`TASKS_DEV1.md`](TASKS_DEV1.md). Specs: [`FEATURE_SPECS_DEV1.md`](FEATURE_SPECS_DEV1.md). Rules: [`../common/WORKING_AGREEMENT.md`](../common/WORKING_AGREEMENT.md).

## 1. What exists today (all on `main`)
- Ported Pluto base: `pluto-core` (tasks, reminders, notes, focus, meetings, SQLite), Tauri shell, React UI, CI, e2e and release workflows.
- Your skeleton (T-003, merged): crates `pluto-contracts`, `pluto-gnani`, `pluto-voice`, `pluto-intel` with empty module placeholders; migrations 012 and 013 registered but empty.
- Nothing yet calls Gnani, Gemini or Groq. Every new type, trait and command is still to build.

## 2. Sync your branch
```bash
git fetch origin '+refs/heads/*:refs/remotes/origin/*'
git checkout D1
git merge origin/main
```

## 3. Environment
- Node 22 (`.nvmrc`), pnpm 10.28, Rust 1.97 (pinned), WebView2 on Windows.
- `cp .env.example .env` and fill `GNANI_API_KEY` (needed for T-010). The desktop app reads keys from the OS keychain, never `.env`.
- Verify: `pnpm install`, `cargo test -p pluto-core -p pluto-contracts -p pluto-gnani -p pluto-voice`, `pnpm test`.

## 4. First tasks, in order
1. **T-004** — voice, speech and error types in `crates/pluto-contracts/src/{voice,speech,error}.rs`; traits `SpeechToText`, `TextToSpeech`, `ToolExecutor`, `FrameSink`. Use `#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]`, then `pnpm gen:types`. New enums are snake_case; IDs via `pluto_core::ids::new_id()`.
2. **T-007** — register all new commands as stubs in one edit: `src-tauri/src/voice.rs`, `meeting_ai.rs`, the `generate_handler!` list in `src-tauri/src/lib.rs`. New commands return `VoiceError`.
3. **T-013, T-010, T-012** — as listed in `TASKS_DEV1.md`. T-010 checks your Gnani key.
4. **T-011** (mic in WebView2) — do it jointly with D2's check; failure changes the plan.

Waits on: T-007 needs D2's T-005 types for the meeting/llm commands; coordinate through a `contract` PR.

## 5. Files
- Yours: `pluto-gnani` (http, realtime, tts, batch), `pluto-voice`, `pluto-contracts` voice/speech/error, migration 013, `src/audio`, CI and release workflows.
- Do not edit: D2's files (`pluto-intel`, `meeting.rs`, `llm.rs`, `decide.rs`, migration 012, `src/features/*` UI pages). Hot files and owners: WORKING_AGREEMENT §2.

## 6. Daily rhythm
Morning: merge `origin/main`. Work on `D1`. PR into `main` at least daily (merge commit, not squash); D2 reviews within 2 working hours. If `main` is red, fix it first. Contract changes go only through a PR labelled `contract`.

## 7. Ready checklist
- [ ] Branch synced, `cargo test` and `pnpm test` pass
- [ ] `.env` has the Gnani key
- [ ] Read WORKING_AGREEMENT §2, §4, §5
