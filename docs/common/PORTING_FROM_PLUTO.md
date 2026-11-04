# Porting from Pluto — record

**Status:** the base is ported and on `main`. The plan is re-baselined on it (PROJECT_ROADMAP). Windows verification is task T-001/T-002.
**Source:** the owner's Pluto repository, commit `eb4d520`. The owner holds the rights (QUORBIT TECHNOLOGIES is the owner's company). Decisions: PRD D-12.

## 1. What was copied (as-is, Pluto's layout kept)
| Pluto path | Now in this repo | Role |
|---|---|---|
| `crates/pluto-core/` | `crates/pluto-core/` | Rust business logic: db, tasks, reminders, scheduler, notes, labels, focus, meetings, projects, settings, usage tracking, diary, pets… |
| `src-tauri/` | `src-tauri/` | Tauri v2 shell: IPC (99 commands), tray, global shortcuts, notifications, autostart, windows |
| `src/` | `src/` | React + TypeScript UI |
| `scripts/`, `e2e/` | same | Type generation, core-deps check, end-to-end smoke tests |
| `packs/` | `packs/` | Desktop-pet art |
| Root configs: `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml` (1.97.0), `package.json`, `pnpm-lock.yaml`, `tsconfig*.json`, `vite.config.ts`, `eslint.config.js`, `components.json`, `main.html`, `pet.html`, `card.html` | same | Build setup |
| `desktop-pet-docs/docs/05-backend/migrations/` | same path | The only docs the code needs (included with `include_str!`) |

## 2. What was not copied
Pluto's other documentation (`desktop-pet-docs/` apart from migrations), `skills/`, its instructions file for AI sessions, `README.md`, `LICENSE`, `audit.yml` and `v2-ram.yml` (not needed). **Imported later (3–4 Nov):** `ci.yml`, `e2e.yml` and `release.yml` into `.github/workflows/`, unchanged except that CI and e2e also run on pushes to `D1` and `D2`. `release.yml` builds Windows and macOS installers into a private draft release, only when the app version changes, on a `v*` tag, or on a manual run.

## 3. Verification so far (Linux sandbox, 3 Nov 2026)
| Check | Result |
|---|---|
| `cargo test -p pluto-core` | **492 passed, 0 failed** |
| `pnpm install --frozen-lockfile`, `pnpm typecheck` | Clean |
| `pnpm test` (UI) | **486 passed**, 7 skipped, 0 failed |
| `pnpm check:core-deps` | OK: no Tauri crates in the core's 148 dependencies |
| `cargo check` on the Tauri shell | **Not possible in this sandbox** (missing GTK system libraries). **Unverified.** It must be built on the Windows demo machine |
| Running the app, tray, shortcuts, notifications, tracking | **Unverified.** Pluto's own status file marks 42 such items as unverified on a real machine |

## 4. Mapping to this pack's architecture (final, from the re-plan)
| This pack says | In the ported code |
|---|---|
| Tauri shell | `src-tauri/` (unchanged name) |
| UI | `src/` (unchanged name) |
| Core | `crates/pluto-core/` (unchanged) |
| New layers | `crates/pluto-contracts`, `pluto-gnani`, `pluto-voice`, `pluto-intel` — created at T-003 |

**Naming:** the base's `pluto-core` is kept; a crate named plain `core` would conflict with Rust's built-in `core` library, so all docs use `pluto-core`.

## 5. Known differences from the contracts in this pack (to resolve at the re-plan)
- Task statuses: Pluto `PLANNED / IN_PROGRESS / PENDING / COMPLETED / CANCELLED`; this pack `TODO / IN_PROGRESS / DONE / CANCELLED`.
- Pluto has no `meeting_items` provenance (quotes, source segments), no transcript segments or recordings, and no voice, LLM or Gnani code.
- Pluto includes features outside this product's scope (desktop pet, characters, diary, courses and exams, app and website usage tracking). They compile and are tested, but are not part of the plan.
- Two incidental mentions of "claude" exist in the copied code: a URL in a unit test (`sites.rs`) and a comment (`tracking.rs`).

## 6. Re-plan result (3 Nov 2026)
- **Kept as-is:** everything functional in the base, including the pet, diary and tracking code (dormant). Nothing is deleted before the demo.
- **Adopted from the base:** its types, statuses, ids, errors and events (PRD D-14). The earlier duplicate contracts were removed.
- **Added:** four crates, migrations 012–013, the voice and meeting UI, CI.
- **Fixed after the port:** `.cargo/config.toml` and `.nvmrc` were missing from the first copy and were added.
- **Next:** T-001 and T-002 verify the base on both Windows machines.
