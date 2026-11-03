# Working Agreement — Developer 1 and Developer 2

**This file is the single authority for:**
- who owns which path;
- how shared files are handled;
- branching, commits and pull requests;
- how contracts change.

Everything else (types, commands, tables) is in the other `docs/common/` files and is not repeated here.

**Goal:** two people write code at the same time for 8 days without waiting on each other, without merge conflicts in shared files, and without discovering at the end that the halves don't fit.

---

## 1. Repository layout and dependency direction
> **Path mapping after the port (PRD D-12):** `app` = `src-tauri/`, `ui` = `src/`, `crates/core` = `crates/pluto-core/`. The ownership map in §2 applies to the mapped paths until the re-plan finalises the layout.

```
/app                  Tauri shell: commands, events, hotkey, tray, keychain          D1
/crates/types         shared contract types + traits (no logic)                      D1 (files split, see §2)
/crates/core          database, migrations, personal services (tasks, reminders…)    D1
/crates/voice         Gnani clients, runtime, recorder, assistant, privacy           D1
/crates/intel         LLM layer, decisions, meeting intelligence                     D2
/ui                   React + Vite app                                               D2 (personal pages: D1)
/test-data            audio scripts, ground truth, shared JSON fixtures              split, see §2
/docs                 documentation                                                  see §2
```

**Crate dependencies (one direction only; a cycle is a bug):**
```
types  ←  core  ←  voice
types  ←  core  ←  intel
app depends on all of them and wires them together.
```
- `voice` and `intel` **never depend on each other.**
- They talk through **traits defined in `types`** (e.g. the assistant in `voice` receives an `Arc<dyn Decider>` and `Arc<dyn Llm>` implemented in `intel`; the recap in `intel` receives an `Arc<dyn TextToSpeech>` implemented in `voice`). `app` creates the implementations and passes them in.
- No crate depends on `tauri` except `app` (ADR-002 style; enforced by a CI check).

## 2. Ownership map
"Owner" writes and merges. The other developer reviews. Editing a file you don't own is allowed only through a PR the owner approves, and only when the change is needed to unblock you.

| Path | Owner | Notes |
|---|---|---|
| `/app/src/main.rs`, `/app/src/commands/registry.rs`, `/app/src/events.rs`, `/app/src/keychain.rs`, `tauri.conf.json`, capabilities | **D1** | The registry lists **every** command from day 1 (task T-006). Adding a command later is a contract change (§5) |
| `/app/src/commands/personal.rs`, `voice.rs`, `settings.rs` | **D1** | |
| `/app/src/commands/meetings.rs` | **D1 writes the stubs at T-006, then D2 owns it** | Thin wrappers calling `intel`; D2 replaces the stub bodies |
| `/crates/types/src/lib.rs` | **D1** | Lists every module once, at skeleton time. Nobody edits it afterwards |
| `/crates/types/src/{ids,time,error,personal,voice,speech}.rs` | **D1** | Contract files: changes need D2's approval |
| `/crates/types/src/{meeting,llm,decide}.rs` | **D2** | Contract files: changes need D1's approval |
| `/crates/core/**` (incl. `migrations/`) | **D1** | D2 reviews migrations. D2 may call core's public functions only |
| `/crates/voice/**` | **D1** | |
| `/crates/intel/**` | **D2** | |
| `/ui/package.json`, `vite.config.ts`, `tsconfig*.json`, router, layout, sidebar, `src/components/**` | **D2** | |
| `/ui/src/audio/**`, `features/meetings/**`, `features/voice/**`, `features/settings/**` | **D2** | |
| `/ui/src/features/personal/**` | **D1** | Files are created as placeholders at T-002: `TodayPage.tsx`, `TasksPage.tsx`, `RemindersPage.tsx`, `NotesPage.tsx`, `FocusPage.tsx`, `ProjectsPage.tsx`. D1 replaces their contents; routes already point at them |
| `/ui/src/lib/api/generated/**` | **generated** | Never hand-edited (§6) |
| `/ui/src/lib/api/client.ts` | **D2** | The only place that calls `invoke`/`listen` |
| `/ui/src/lib/mock/personal.ts` | **D1** | Mock for the personal commands |
| `/ui/src/lib/mock/{meetings,voice,settings}.ts`, `mock/index.ts` | **D2** | |
| `/test-data/audio-scripts/**`, `/test-data/ground-truth/**` | **D2** | |
| `/test-data/fixtures/{llm,meetings,voice}/**` | **D2** | |
| `/test-data/fixtures/{gnani,personal}/**` | **D1** | |
| `/.github/**`, root configs (`Cargo.toml`, `rustfmt.toml`, `.editorconfig`, `.gitattributes`, `.gitignore`) | **D1** | Dependency additions follow §3 |
| `/docs/common/**` | **Both** | Contract documents: both approve (§5) |
| `/docs/dev1/**` | D1 | |
| `/docs/dev2/**` | D2 | |
| `/PROJECT_ROADMAP.md` | **Both** | Each developer edits **only their own task rows** (one row per line, so Git merges them cleanly) |

## 3. Shared files that cause conflicts, and the rule for each
| File | Rule |
|---|---|
| Root `Cargo.toml` | Workspace members are a **glob** (`members = ["app", "crates/*"]`), so adding a crate never edits this file. Shared dependency versions live in `[workspace.dependencies]`; a dependency your crate needs that isn't there yet is added in a **dedicated `chore/deps-<name>` PR** that touches only manifests and the lockfile, and is merged within the hour |
| `Cargo.lock`, `pnpm-lock.yaml` | **Never hand-resolve a conflict.** Take the incoming version, then regenerate (`cargo update -w`, `pnpm install`) and commit. Lockfiles change only in the PR that changes a manifest |
| `ui/package.json` | D2 owns it. D1 adds UI dependencies through a `chore/deps-…` PR |
| `crates/*/Cargo.toml` | Owned by the crate's owner |
| Generated TypeScript types | Regenerate with `pnpm gen:types`. Never edit by hand. On conflict: delete, regenerate, commit (§6) |
| `ui/src/lib/mock/index.ts` | Composes the per-owner mock files. D2 owns it; D1 changes only `mock/personal.ts` |
| `app/src/commands/registry.rs` | Fully populated at T-006. After that it changes only in a contract PR |
| `PROJECT_ROADMAP.md` | Your own rows only |
| `docs/common/*.md` | Contract PR only (§5) |

## 4. Branches, commits and pull requests

### Branches
| Branch | Purpose | Who may push |
|---|---|---|
| `main` | Demo-ready only; updated at checkpoints C0, C2, C3, C4, C5 | Nobody pushes directly. PR from `develop` after the checkpoint passes |
| `develop` | Integration. **Must always build and pass CI** | Nobody pushes directly. PRs only |
| `feat/T-<id>-<short-name>` | One task | Its author. May be force-pushed with `--force-with-lease` by the author only |
| `fix/T-<id>-<short-name>` | Bug in a task | Its author |
| `contract/<id>-<short-name>` | Contract change (§5) | Its author. Never mixed with feature code |
| `chore/<short-name>` | Dependencies, tooling, docs | Its author |

Branch off the latest `develop`. Rebase on `develop` at least once a day (`git fetch && git rebase origin/develop`). Never force-push `main` or `develop`.

### Commits
- Format: `type(scope): subject (T-<id>)`. Types: `feat`, `fix`, `test`, `docs`, `chore`, `refactor`, `contract`. Example: `feat(intel): add Gemini and Groq router with fallback (T-104)`.
- Subject in the imperative, ≤ 72 characters.
- Each commit carries **only the developer's own name and email**. No tool or bot attribution lines.
- Each commit builds and passes formatting. Small commits are preferred.

### Pull requests
- One task ID per PR. Target ≤ 400 changed lines. Open as **draft** early; mark ready when the checks pass.
- The PR template (`.github/pull_request_template.md`) must be filled in: task ID, acceptance criteria ticked, tests run, contract impact, screenshots for UI.
- **The other developer reviews every PR within 2 working hours** and either approves or lists the exact changes needed. Review focus: does it match the contract, are errors handled, does it break the other side's mocks.
- **Merge:** squash merge, by the author, after approval and green CI. Delete the branch.
- **Merge checks (CI):** `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace`, `pnpm lint`, `pnpm typecheck`, `pnpm test`, no `tauri` dependency outside `app`, generated types up to date, command manifest matches registry and mock API (task T-014).
- **Stop the line:** if `develop` is red, the next commit from either developer is the fix.
- **Cadence:** each developer merges to `develop` at least once a day. At 6 PM each day both pull `develop` and run the full check locally. Whoever sees red fixes it, regardless of who broke it.

### GitHub settings (repository owner sets once)
- Add Developer 1 as a collaborator with write access.
- Protect `main` and `develop`: require a pull request, 1 approval, and the CI checks to pass; disallow force pushes.
- Optional `.github/CODEOWNERS` generated from §2, using the real GitHub handles.

## 5. Changing a contract
A contract is anything the other developer builds against: `docs/common/API_CONTRACTS.md`, `DATA_SCHEMA.md`, files under `crates/types`, the command registry, events, fixtures in `test-data/fixtures`, and `mock/*.ts`.

1. **Prefer additive changes** (a new optional field, a new command). Renaming or removing something after C0 needs both developers' agreement in the PR, because it breaks the other's mocks.
2. Open a `contract/<id>-<name>` PR containing, **together**:
   - the document change;
   - the Rust type or trait change;
   - the regenerated TypeScript types;
   - the updated fixture and mock;
   - the registry entry and the matching function in `ui/src/lib/api/client.ts`.
3. **Both developers must approve.** Merge within 4 hours; the other developer rebases immediately.
4. Bump `CONTRACT_VERSION` in `crates/types/src/lib.rs` and the version line in `API_CONTRACTS.md`. Fixtures carry the version they were written for; CI fails on a mismatch.
5. Until a contract PR is merged, **nobody builds against the proposed change.**

## 6. Rules that prevent integration errors
**Generated code**
- TypeScript types come from the Rust types (`ts-rs`) into `ui/src/lib/api/generated/`, one file per type. They are committed.
- CI regenerates them and fails if the output differs from what is committed.

**One source of truth for data shapes**
- JSON fixtures in `test-data/fixtures/` are the **only** sample data. The UI mocks import them. Rust tests deserialize every fixture into its Rust type (task T-015). A fixture that doesn't match the real type fails CI.
- Mocks have the same function signatures as the real client (`ui/src/lib/api/client.ts`). Screens never call `invoke` directly.

**Where things are decided (to avoid two implementations of the same rule)**
| Rule | Single home |
|---|---|
| Natural-language dates ("kal 5 baje", "Friday tak") → ISO | `intel` only. `Decision.args` reach `voice` and `core` already resolved |
| Approval of writes | `voice::assistant`, nowhere else |
| Retry, queues, rate limits for Gnani | `voice::gnani`, nowhere else |
| Fallback between Gemini and Groq | `intel::llm`, nowhere else |
| Enum conversion between wire and database | `core` only |
| ID generation | Rust only (`types::ids::new_id()`); the UI never creates IDs |
| Consent check before recording | `voice::recorder` and the DB constraint |

**Errors and logging**
- Inside crates use `thiserror`. Convert to `AppError` only at the boundary where a command returns.
- No `unwrap()` or `expect()` outside tests. No `println!`; use `tracing`.
- Never log API keys, and never log transcript text above `debug` level.

**Secrets**
- API keys live only in the OS keychain at runtime. For command-line examples and tests, use environment variables (`GNANI_API_KEY`, `GEMINI_API_KEY`, `GROQ_API_KEY`) from a local `.env`, which is git-ignored. `.env.example` lists the names with empty values.
- Never commit keys, recordings or real data.

**Windows-specific**
- Line endings are LF (`.gitattributes`). Editors must respect `.editorconfig`.
- Use `PathBuf`, never string paths or hard-coded separators.
- Recording files may be locked while open; close them before reading or deleting.

**Encoding and Indian-language text**
- Everything is UTF-8. Test fixtures include Devanagari, Tamil and Kannada text.
- Lengths in the database use characters, not bytes. In Rust use `chars().count()` for limits.
- HTTP headers must be ASCII: non-ASCII `keyterms` are JSON-escaped (API_CONTRACTS §2.1).

**Definition of Ready (before starting a task)**
- The contract types, command and mock it uses exist on `develop`.
- Its dependencies in `PROJECT_ROADMAP.md` are merged, or it is marked parallel-safe.
- If not, raise it in the daily sync immediately instead of working around it.

## 7. Communication
- 15-minute sync each morning: yesterday, today, blockers.
- Anything affecting the other developer (a contract, a hot file, a delay) is written in the PR or the board, not only spoken.
- A blocker lasting more than 1 hour is escalated in the sync or by message.
- If a developer is unavailable for a day, the other works only on the demo-critical path (HACKATHON_PLAN §7).
