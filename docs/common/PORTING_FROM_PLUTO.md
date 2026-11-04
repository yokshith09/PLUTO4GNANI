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
| `pnpm test` (UI) | **486 passed** at the port; **468 passed** after the diary page was removed (§7); 7 skipped, 0 failed |
| `pnpm check:core-deps` | OK: no Tauri crates in the core's 148 dependencies |
| `cargo check` on the Tauri shell | Not possible in the Linux sandbox (missing GTK libraries). **Built and tested on GitHub's Windows and macOS runners (CI green, 4 Nov)** |
| End-to-end on a Windows runner | **Passed** (welcome, task, note, meeting with action item → task, restart) |
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

## 7. Cleanup (4 Nov 2026) — what was removed, what was kept, and why
**Removed (on `D2`, verified: typecheck, lint, 468 UI tests, coverage 94.95% lines, script tests, Tauri guard):**
- The **Diary page** (`src/features/diary`, about 450 lines plus 171 lines of tests) and its menu item. Its only references were one import and one sidebar entry.
- `scripts/svg-to-png.mjs` (pet-art tooling) and `scripts/tracking-report.py` (usage-tracking report). Nothing in the build, CI or `package.json` ran them.

**Kept on purpose:**
- `scripts/ram/`: its test is the only one the CI "script tests" step runs; removing it would fail that step.
- `packs/` (6.1 MB): `tauri.conf.json` bundles four of its pet characters, so deleting it breaks `tauri build`.
- The pet, characters and wardrobe: the pet bubble is the planned spoken-confirmation feedback.

**Why most of Pluto's unrelated features were not deleted.** They are woven through shared code, not separate files:

| Feature area | Approx. size (non-test lines) | Where it is wired in | Decision |
|---|---|---|---|
| Study pack (courses, exams) | UI 281, core 557 | Imported by 8 UI files (tasks, notes, sidebar, Today, guide) | **Hide** (T-402) |
| Time tracking and analytics | UI 1,360, core about 3,000, shell about 2,100 | Imported by Today, wellbeing and the pet card; off by default | **Hide**; tracking stays off |
| Wellbeing nudges, routine, guide, links, calendar | UI about 1,200 | Imported by App, Today, Settings, Welcome, Tasks, Notes | **Keep** (hiding is enough) |
| Legacy importers (`legacy_data.rs`, `earlier_app.rs`) | core 295, shell 143 | Registered in the shell and IPC (D1's files) | **Candidate for D1 after C4** |
| Diary backend (`diary.rs`, migrations 009/010) | core 745 | Registered in IPC; migrations cannot be removed (forward-only) | Dormant; UI removed |

**Rule for further pruning:** nothing else is deleted before checkpoint C4. After C4, D1 may remove backend modules one at a time in their own PR, with CI green and the coverage floors held. Until then the demo hides pages through T-402. Deleting woven-in features now risks regressions in a base whose tests and end-to-end run are currently green.
