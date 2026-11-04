# T-002 — Base verification on Developer 1's machine

**Date:** 4 Nov 2026 · **Commit:** `bd90e85` (`main`, after PRs #3–#5) · **Tester:** D1 (`Duggineniakhil`)

## Machine
| Item | Value |
|---|---|
| Model | Acer Aspire A515-58M |
| OS | Windows 11 Pro, build 26300 |
| RAM | 8 GB |
| Toolchain | Rust 1.97.0 (rustup, pinned by `rust-toolchain.toml`), MSVC build tools (Visual Studio), Node 22.16.0, pnpm 10.28.0 |

## CI (GitHub Actions, read on 4 Nov)
| Workflow | Branch | Commit | Result |
|---|---|---|---|
| CI (Windows + macOS) | `main`, `D1`, `D2` | `bd90e85` | success |
| End-to-end (Windows WebDriver run; macOS smoke) | `main`, `D1`, `D2` | `bd90e85` | success |
| Release build | `main` (PR #4 push) | — | success; `plan` skipped the build because the version did not change. No release exists yet |

No CI failure needed fixing.

## Local checks
| Command | Result |
|---|---|
| `pnpm install --frozen-lockfile` | OK |
| `pnpm typecheck` | Clean |
| `pnpm lint` | Clean |
| `pnpm test` | 73 files passed, 1 skipped; **468 tests passed**, 7 skipped |
| `pnpm test:scripts` | 21 passed, 0 failed |
| `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -D warnings`, `cargo test` (core + new crates: 492 + 1), `pnpm check:core-deps`, `pnpm gen:types` | All pass (run for T-003 on the same machine, 3–4 Nov) |

## `pnpm tauri dev`
| Step | Result |
|---|---|
| First debug build of the shell | `Finished` in 7 min 58 s (547 crates) |
| App start | `target\debug\pluto.exe` opened its window |
| Pages | Today, Tasks, Notes and Meetings loaded and were clickable (checked by D1) |
| Close | Exited with code 0. Log shows `Failed to unregister class Chrome_WidgetWin_0. Error = 1412` at shutdown — a known, harmless WebView2 message on window close |

**Not checked here** (covered by T-001 on D2's machine): a reminder notification firing, tray, autostart, global shortcuts.

## Decision recorded
Owner rule (D1, 4 Nov): **from now on, app builds for manual testing come from GitHub Actions** (`release.yml`, "Run workflow" → draft release with the Windows and macOS installers), not from compiling on a developer laptop. Local `pnpm tauri dev` is no longer part of the routine; CI already builds the shell on Windows and macOS on every push.

## Result
**T-002 passes:** CI and Windows e2e are green on `main`; D1's machine passes `pnpm tauri dev`.
