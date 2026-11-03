# Porting from Pluto — record

**Status:** the base is ported (branch `chore/port-pluto-base`). Verification on Windows and the re-plan are the next steps.
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
Pluto's other documentation (`desktop-pet-docs/` apart from migrations), `skills/`, its instructions file for AI sessions, `README.md`, `LICENSE`, `.github/workflows/` (CI, audit, e2e, release, v2-ram). CI will be added back deliberately at the re-plan; the release workflow is intentionally not brought over.

## 3. Verification so far (Linux sandbox, 3 Nov 2026)
| Check | Result |
|---|---|
| `cargo test -p pluto-core` | **492 passed, 0 failed** |
| `pnpm install --frozen-lockfile`, `pnpm typecheck` | Clean |
| `pnpm test` (UI) | **486 passed**, 7 skipped, 0 failed |
| `pnpm check:core-deps` | OK: no Tauri crates in the core's 148 dependencies |
| `cargo check` on the Tauri shell | **Not possible in this sandbox** (missing GTK system libraries). **Unverified.** It must be built on the Windows demo machine |
| Running the app, tray, shortcuts, notifications, tracking | **Unverified.** Pluto's own status file marks 42 such items as unverified on a real machine |

## 4. Mapping to the architecture in this pack (until the re-plan finalises it)
| This pack says | In the ported code |
|---|---|
| `app` (Tauri shell) | `src-tauri/` |
| `ui` | `src/` |
| `crates/core` | `crates/pluto-core/` |
| `crates/types`, `crates/voice`, `crates/intel` | Not yet created; they are the new layers |

**Naming finding:** a crate named `core` conflicts with Rust's built-in `core` library. Pluto's name `pluto-core` avoids this, so the docs' `crates/core` becomes `crates/pluto-core`.

## 5. Known differences from the contracts in this pack (to resolve at the re-plan)
- Task statuses: Pluto `PLANNED / IN_PROGRESS / PENDING / COMPLETED / CANCELLED`; this pack `TODO / IN_PROGRESS / DONE / CANCELLED`.
- Pluto has no `meeting_items` provenance (quotes, source segments), no transcript segments or recordings, and no voice, LLM or Gnani code.
- Pluto includes features outside this product's scope (desktop pet, characters, diary, courses and exams, app and website usage tracking). They compile and are tested, but are not part of the plan.
- Two incidental mentions of "claude" exist in the copied code: a URL in a unit test (`sites.rs`) and a comment (`tracking.rs`).

## 6. Next steps
1. Build and run on Windows; record results next to the checks above.
2. Decide what to keep, prune, adapt or replace (adopt Pluto's schema and types, or adapt them to the contracts in this pack).
3. Re-baseline `PROJECT_ROADMAP.md`, `API_CONTRACTS.md`, `DATA_SCHEMA.md` and the ownership map.
4. Then build the voice, audio, LLM and meeting layers on top.
