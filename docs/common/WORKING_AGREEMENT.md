# Working Agreement — Developer 1 and Developer 2

**This file is the single authority for:**
- which path each developer owns;
- how shared files are handled;
- branches, commits and pull requests;
- how contracts change.

Types, commands and tables live in the other `docs/common/` files and are not repeated here.

**Goal:** two people write code at the same time for a week without waiting on each other, without merge conflicts in shared files, and without finding out at the end that the halves don't fit.

---

## 1. Repository layout
The app is the ported Pluto base plus four new crates (PRD D-12, D-14).

```
src-tauri/                  Tauri shell: IPC commands, tray, hotkeys, events                  D1
  src/voice.rs              new voice commands (stubs first, then real)                       D1
  src/meeting_ai.rs         new meeting-intelligence commands (thin wrappers)                 D2 (D1 writes the stubs)
src/                        React + TypeScript UI                                              D2 (exceptions in §2)
crates/pluto-core/          existing core: db, tasks, reminders, notes, focus, meetings        D1 (new store files: §2)
crates/pluto-contracts/     NEW  shared types and traits, no logic                             files split, §2
crates/pluto-gnani/         NEW  Gnani clients                                                 files split, §2
crates/pluto-voice/         NEW  voice runtime, assistant, privacy                             D1
crates/pluto-intel/         NEW  LLM layer, decisions, meeting intelligence, recorder          D2
test-data/                  NEW  audio scripts, ground truth, shared JSON fixtures             D2 (some fixtures: D1)
desktop-pet-docs/.../migrations/   SQL migrations (001–011 from the base)                      012: D2, 013: D1
docs/                       documentation                                                      §2
```

**Crate dependencies (one direction; a cycle is a bug):**
```
pluto-core  ←  pluto-contracts  ←  pluto-gnani  ←  pluto-voice
                                              └──←  pluto-intel
pluto-voice and pluto-intel never depend on each other. src-tauri depends on all of them.
```
- `pluto-voice` and `pluto-intel` talk through **traits defined in `pluto-contracts`**. For example, the assistant in `pluto-voice` receives an `Arc<dyn Decider>` and an `Arc<dyn Llm>` implemented in `pluto-intel`; the recap in `pluto-intel` receives an `Arc<dyn TextToSpeech>` implemented in `pluto-gnani`. `src-tauri` creates the implementations and passes them in.
- No crate except `src-tauri` depends on `tauri` (Pluto's existing rule, checked by `pnpm check:core-deps`).
- Both `pluto-voice` and `pluto-intel` may call `pluto-core` functions (for example `tasks::create`, `meetings::update`). They never edit `pluto-core`'s existing files.

## 2. Ownership map
"Owner" writes the file. The other developer reviews the PR that merges it into `main`. Editing a file you don't own is allowed only to unblock yourself, in a PR the owner approves.

| Path | Owner | Notes |
|---|---|---|
| `src-tauri/src/lib.rs` (plugins, `generate_handler!`), `src-tauri/src/shortcuts.rs`, `tauri.conf.json`, capabilities | **D1** | `generate_handler!` gets **all** new commands in one edit at T-007. Later changes are contract PRs |
| `src-tauri/src/voice.rs` | **D1** | |
| `src-tauri/src/meeting_ai.rs` | **D1 writes the stubs (T-007), then D2 owns it** | |
| `crates/pluto-core/**` existing files | **D1** | Existing behaviour is not changed without a PR and a test |
| `crates/pluto-core/src/meeting_ai_store.rs` + `migrations/012_meeting_ai.sql` | **D2** | The `mod` line in `lib.rs` and the `migrate.rs` entry are added by D1 at T-003 |
| `crates/pluto-core/src/voice_store.rs` + `migrations/013_voice.sql` | **D1** | Same |
| `crates/pluto-contracts/src/{lib,voice,speech,error}.rs` | **D1** | Contract files: D2 approves changes. `lib.rs` is written once at T-003 and not edited again |
| `crates/pluto-contracts/src/{meeting,llm,decide}.rs` | **D2** | Contract files: D1 approves changes |
| `crates/pluto-gnani/src/{lib,http,realtime,tts}.rs` | **D1** | |
| `crates/pluto-gnani/src/{batch,rest}.rs` | **D1 / D2** | `batch.rs`: D1 (T-201). `rest.rs`: D2 (comparison, P1) |
| `crates/pluto-voice/**` | **D1** | |
| `crates/pluto-intel/**` | **D2** | |
| `src/audio/**` | **D1** | Capture worklet, sender, player |
| `src/ipc/voiceClient.ts`, `src/ipc/meetingAi.ts`, mock handlers for the new commands | **D2** | The one edit to `src/ipc/mock.ts` that composes them is made by D2 at T-006 |
| `src/ipc/generated/**` | **generated** | Never hand-edited (§6) |
| `src/features/meetings/**`, `src/features/voice/**`, `src/features/settings/**` (new Voice section), `src/App.tsx`, `src/features/shell/Sidebar.tsx`, `src/components/**` | **D2** | |
| Other existing UI under `src/features/**` | **D2** | No planned changes; hiding pages is one setting (T-402) |
| `test-data/audio-scripts/**`, `test-data/ground-truth/**`, `test-data/audio/**`, `test-data/fixtures/{llm,meetings,voice}/**` | **D2** | |
| `test-data/fixtures/gnani/**` | **D1** | |
| `.github/**`, `Cargo.toml` (workspace), `package.json` scripts, `.cargo/`, `rust-toolchain.toml` | **D1** | Dependency additions follow §3 |
| `docs/common/**` | **Both** | Contract documents: both approve (§5) |
| `docs/dev1/**` / `docs/dev2/**` | D1 / D2 | |
| `PROJECT_ROADMAP.md` | **Both** | Edit **only your own task rows** (one row per line, so Git merges them cleanly) |

## 3. Shared files that cause conflicts, and the rule for each
| File | Rule |
|---|---|
| Root `Cargo.toml` | Workspace members use a glob (`crates/*`) plus `src-tauri`. Shared dependency versions live in `[workspace.dependencies]`. A dependency another crate needs is added in a **tiny PR that touches only manifests and the lockfile**, merged within the hour |
| `Cargo.lock`, `pnpm-lock.yaml` | **Never hand-resolve a conflict.** Take the incoming version, regenerate (`cargo update -w`, `pnpm install`), commit. Lockfiles change only in a PR that changes a manifest |
| `package.json` | D1 owns scripts; D2 owns dependencies. Either may add a dependency through the tiny-PR rule |
| `src-tauri/src/lib.rs` | `generate_handler!` is edited once (T-007) |
| `crates/pluto-core/src/lib.rs`, `db/migrate.rs` | Edited once, by D1, at T-003 (module lines and both migration entries pre-registered) |
| `src/ipc/generated/**` | Regenerate with `pnpm gen:types` (§6). On a conflict: delete, regenerate, commit |
| `src/ipc/mock.ts`, `src/ipc/client.ts`, `src/App.tsx`, `src/features/shell/Sidebar.tsx` | Edited once by D2 at T-006 to wire in the new files; afterwards new work goes in new files |
| `PROJECT_ROADMAP.md` | Your own rows only |
| `docs/common/*.md` | Contract PR only (§5) |

## 4. Branches, commits and pull requests

### The only branches
| Branch | Purpose | Who pushes |
|---|---|---|
| `main` | Integration and the demo. Always builds and passes CI | Nobody pushes directly. Merge by pull request from `D1` or `D2` |
| `D1` | Developer 1's working branch | Developer 1 only |
| `D2` | Developer 2's working branch | Developer 2 only |

**No other branch is pushed to the remote.** Short-lived local branches are fine; do not push them. Tags (for example `demo-v1`) are allowed.

### Daily rhythm
1. **Start of day:** merge `main` into your branch (`git fetch && git merge origin/main`) and resolve conflicts in files you own; if a conflict is in a file you don't own, ask its owner.
2. Work and commit on your branch. Push often.
3. **At least once a day, and before every checkpoint:** open a pull request from your branch into `main`. The other developer reviews it within 2 working hours.
4. After approval and a green CI, the **author merges with "Create a merge commit"** (not squash: squash makes a long-lived branch conflict with `main` afterwards).
5. Immediately after any merge into `main`, the other developer merges `main` into their branch.
6. **6 PM each day:** both pull `main` and run the full check. Whoever sees red fixes it, regardless of who broke it. If `main` is red, the next commit from either developer is the fix.

### Commits
- Format: `type(scope): subject (T-<id>)`. Types: `feat`, `fix`, `test`, `docs`, `chore`, `refactor`, `contract`.
  Example: `feat(intel): add Gemini and Groq router with fallback (T-104)`.
- Imperative subject, 72 characters or fewer.
- Each commit carries **only the developer's own name and email**. No tool or bot attribution lines.
- Each commit builds and is formatted.

### Pull requests
- Title starts with the task IDs it completes. Target 400 changed lines or fewer; split larger work into several PRs from the same branch.
- Fill in the PR template (`.github/pull_request_template.md`): tasks, acceptance criteria ticked, tests run, contract impact, screenshots for UI.
- Review focus: does it match the contract, are errors handled, does it break the other side's mock or fixtures.
- **CI checks** (`.github/workflows/ci.yml` and `e2e.yml`, run on every PR and on pushes to `main`; not on `D1`/`D2` pushes, not for docs-only changes; macOS, the release build and the full coverage report run on `main` only — TESTING_STRATEGY §9): `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace`, `pnpm typecheck`, `pnpm lint`, `pnpm test`, `pnpm check:core-deps` (no `tauri` outside `src-tauri`), generated types up to date.

### GitHub settings (repository owner sets once)
- Add Developer 1 as a collaborator with write access.
- Protect `main`: require a pull request, 1 approval and the CI checks; disallow force pushes.
- Optionally generate `.github/CODEOWNERS` from §2 once both GitHub handles are known.

## 5. Changing a contract
A contract is anything the other developer builds against: `docs/common/API_CONTRACTS.md`, `DATA_SCHEMA.md`, files in `crates/pluto-contracts`, the command list in `generate_handler!`, events, fixtures in `test-data/fixtures`, and the mock handlers.

1. **Prefer additive changes** (a new optional field, a new command). Renaming or removing something after C0 needs both developers' agreement in the PR.
2. Open the PR from your branch with the label `contract`. It contains, **together**: the document change; the Rust type or trait change; regenerated TypeScript types; updated fixtures and mocks; the command registration; the client function.
3. **Both developers approve.** Merge within 4 hours; the other developer merges `main` into their branch immediately.
4. Bump `CONTRACT_VERSION` in `crates/pluto-contracts/src/lib.rs` and the version line in `API_CONTRACTS.md`.
5. Until the contract PR is merged, **nobody builds against the proposed change.**

## 6. Rules that prevent integration errors
**Generated code**
- TypeScript types come from the Rust types (`ts-rs`) into `src/ipc/generated/` (set by `.cargo/config.toml`), committed, regenerated with `pnpm gen:types`. The script is extended to the new crates at T-003.

**One source of truth for data shapes**
- JSON fixtures in `test-data/fixtures/` are the only sample data. The UI mocks import them and Rust tests deserialize every fixture into its Rust type. A fixture that doesn't match the real type fails CI.
- Screens call only the typed client in `src/ipc/`, never `invoke` directly (Pluto's existing rule).

**Where each rule lives (so it exists once)**
| Rule | Single home |
|---|---|
| Natural-language dates ("kal 5 baje", "Friday tak") → ISO | `pluto-intel` only. `AssistantDecision.args` reach `pluto-voice` and `pluto-core` already resolved |
| Approval of writes | `pluto-voice::assistant`, nowhere else |
| Retry, request queues, rate limits for Gnani | `pluto-gnani::http`, nowhere else |
| Fallback between Gemini and Groq | `pluto-intel::llm`, nowhere else |
| ID generation | Rust only (`pluto_core::ids::new_id()`, UUID v7); the UI never creates IDs |
| Consent check before recording | `pluto-intel::meeting::recorder` and the database constraint |
| Meeting provenance (quotes, source segment) | `meeting_ai_store`, written only by `meeting_extraction_apply` |

**Errors and logging**
- Inside crates use `thiserror`. Existing commands keep returning core's `AppError`. **New voice and meeting commands return `VoiceError`** (API_CONTRACTS §6), because core's `ErrorCode` has only 7 fixed values.
- No `unwrap()`/`expect()` outside tests; no `println!` (use `tracing`). Never log API keys; never log transcript text above `debug`.

**Secrets**
- API keys live only in the OS keychain at runtime. Command-line examples and tests read `GNANI_API_KEY`, `GEMINI_API_KEY`, `GROQ_API_KEY` from a git-ignored `.env`; `.env.example` lists the names. Never commit keys, recordings or real data.

**Windows and text**
- Line endings are LF (`.gitattributes`). Use `PathBuf`, never hard-coded separators.
- Everything is UTF-8. Fixtures include Devanagari, Tamil and Kannada text. Limits count characters, not bytes. HTTP headers are ASCII, so non-ASCII `keyterms` are JSON-escaped.

**Definition of Ready (before starting a task)**
- The contract types, command and mock it uses are on `main`, or the task is marked parallel-safe.
- If not, raise it in the daily sync at once instead of working around it.

## 7. Communication
- 15-minute sync each morning: yesterday, today, blockers.
- Anything affecting the other developer (a contract, a shared file, a delay) goes in the PR or the board, not only in conversation.
- A blocker longer than 1 hour is escalated immediately.
- If a developer is unavailable for a day, the other works only on the demo-critical path (HACKATHON_PLAN §7).
