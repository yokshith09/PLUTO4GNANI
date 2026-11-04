# Testing Strategy — Pluto Voice

**Rule:** nothing is reported unless it was run. Results go to `docs/results/` with date, commit, machine and audio set.

## 1. Levels
| Level | Scope | Tools | Owner |
|---|---|---|---|
| Base regression | The base's existing tests must keep passing (492 core tests; 468 UI tests after the diary page was removed) | `cargo test -p pluto-core`, `pnpm test` | Both |
| Unit (Rust) | WAV chunking, retries and request queues, Realtime rotation, Batch parsing, date resolution, rules, LLM router fallback, grounding validator, proposal gate | `cargo test` | Module owner |
| Unit (DB) | Migrations 012–013, checks, cascades, FTS5 | `cargo test -p pluto-core` | D1 (013), D2 (012) |
| Unit (UI) | Every screen in every state on mocks; worklet downsampling (pure function) | Vitest + Testing Library | Page owner |
| Contract | Gnani clients against fixtures from documented shapes; LLM adapters against recorded responses | `wiremock` + local WS server | D1 (Gnani), D2 (LLM) |
| Live | Real Gnani / Gemini / Groq with short clips | `cargo test --features live -- --ignored` | Both |
| E2E | Journeys A1, A2, B1, B2 on the Windows demo machine | Manual script (§7) | Both |

## 2. Day-1 platform checks (blocking)
| ID | Check | Pass criterion |
|---|---|---|
| T-IPC-01 | WebView2 mic in Tauri + raw binary `invoke` + binary `Channel` | 10 s captured → Rust → WAV plays back correctly; a TTS chunk plays in the UI |
| T-LLM-01 | Gemini and Groq JSON output with the chosen models | Schema-valid JSON on 5 prompts each. Record the free-tier limits from each dashboard |
| T-GN-01 | Gnani key: REST clip, Realtime clip, TTS | All return successfully. Record the credits used |

### Contract checks (CI, every PR)
| ID | Check | Pass |
|---|---|---|
| T-CT-01 | Every fixture in `test-data/fixtures/` deserializes into its Rust type and round-trips | Pass |
| T-CT-02 | Regenerating TypeScript types produces no diff | Pass |
| T-CT-03 | Command names match across API_CONTRACTS §4, `registry.rs`, `client.ts` and every mock | Pass |
| T-CT-04 | No crate except `app` depends on `tauri`; no dependency cycle between crates | Pass |
| T-CT-05 | Fixture `contract_version` equals `CONTRACT_VERSION` | Pass |

## 3. Audio test set (synthetic only)
| Set | Content | Use |
|---|---|---|
| AS-1 | Hinglish meeting, 3 speakers, ~3 min, 4 decisions, 5 action items with names and relative dates, crosstalk | Extraction, speakers, demo |
| AS-2 | Tanglish meeting, 3 speakers, ~3 min | Language handling |
| AS-3 | Kannada-English meeting, 2 speakers, ~2 min | Regional coverage |
| AS-4 | 50 commands (hi-en, ta-en, kn-en, en) covering every P0/P1 tool | Routing, latency |
| AS-5 | AS-1 and AS-4 with noise at SNR 10 dB and 5 dB, plus an 8 kHz resample | Robustness |

Each set has a ground-truth file (expected items and intents). Noise is mixed with `ffmpeg` or `sox`; commands recorded in `docs/results/README.md`.

## 4. Functional tests
| ID | Test | Pass |
|---|---|---|
| T-ML-01 | AS-1..3 through Batch (`codemix`, speaker separation) | Transcript produced; languages logged |
| T-ML-02 | Extraction vs ground truth | ≥ 85% of action items correct |
| T-ML-03 | Speaker accuracy after mapping | ≥ 90% |
| T-ML-04 | AS-4 routing | ≥ 90% correct tool |
| T-NZ-01/02 | AS-5 vs clean | Drop reported (no target asserted) |
| T-CMP-01 | Prisma vs a named baseline | Errors on names, numbers and dates reported |
| T-PP-01 | Personal features (tasks, reminders, notes, focus) | Covered by the base's own tests (`pluto-core`, UI); new work must not break them |

## 5. Latency (20 runs each, from `voice_interactions`)
| ID | Measure | Target |
|---|---|---|
| T-LAT-01 | Hotkey → `Listening` shown | ≤ 100 ms |
| T-LAT-02 | End of speech → final transcript | Reported p50 / p90 |
| T-LAT-03 | Transcript → proposal or answer (Groq intent) | p50 ≤ 1.5 s |
| T-LAT-04 | End of speech → first TTS audio | p50 ≤ 3 s |
| T-LAT-05 | Batch turnaround for a 3-min meeting | Reported |

## 6. Failure, safety and privacy
| ID | Test | Expected |
|---|---|---|
| T-FH-01 | Network cut during recording | Recording continues; the job runs later |
| T-FH-02 | Mock Gnani 429 → 200 | Retried; no user error |
| T-FH-03 | Mock 402 | Banner; voice disabled |
| T-FH-04 | Realtime close code 1001 | Reconnect |
| T-FH-05 | Mic unplugged | "Mic lost"; resume works |
| T-FH-06 | Batch `PARTIAL_FAILURE` fixture | Gap markers + retry |
| T-FH-07 | Groq 429 | Gemini answers; `fell_back = true` logged |
| T-FH-08 | Both LLMs fail | `LlmUnavailable`; nothing written |
| T-FH-09 | Crash while recording | Recovery prompt |
| T-AS-05 | Every write tool unreachable without an approved proposal | Pass |
| T-PR-01 | Keys absent from logs, SQLite and exports | Pass |
| T-PR-02 | Every outbound call logged | Pass |
| T-PR-03 | Mic recording impossible without consent (UI + command + DB) | Pass |
| T-PR-04 | Cloud voice off → zero outbound requests | Pass |

## 7. E2E demo acceptance (P0 gate, Windows demo machine)
1. Keys saved; voice test passes.
2. AS-1 played from a speaker and recorded with captions on → captions visible.
3. Stop → transcript → speakers mapped.
4. Extract → at least 4 of 5 ground-truth action items, each with a quote → Apply → tasks appear on the Tasks page.
5. "Priya ne login fix kab tak bola tha?" → spoken answer with citation.
6. "Monday kar do" → proposal → "haan" → due date changed.
7. Reminder created by voice fires a notification at the set time.
8. Recap spoken in Hindi.

## 8. Regression (every PR)
- `pnpm typecheck && pnpm lint && pnpm test`
- `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`

## 9. CI (imported from Pluto, `.github/workflows/`)
- **`ci.yml`** on Windows and macOS: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo check --workspace --locked`, `cargo test --workspace --locked`, generated types up to date, coverage (`pluto-core` ≥ 90% lines), `pnpm typecheck`, `pnpm lint`, `pnpm test:coverage` (frontend floors 80 / 80 / 70 / 55 in `vite.config.ts`), script tests, `pnpm check:core-deps`, and `pnpm tauri build --no-bundle`.
- **`e2e.yml`**: on Windows, the real app is driven by WebDriver (welcome, a task, a note, a meeting with an action item turned into a task, restart to prove persistence); on macOS, a start-up smoke test.
- **`release.yml`**: Windows and macOS installers into a private draft release (version change, `v*` tag or manual run).
- **New code must keep the floors green**, so new modules in `pluto-core` and new UI need tests. The new crates join the workspace jobs automatically.

## 10. Commands
```
cargo test --workspace                 # core + contracts + gnani + voice + intel
cargo test -p pluto-intel              # D2, with MockLlm and fixtures
cargo test -p pluto-gnani -p pluto-voice   # D1, with wiremock and mock STT/TTS
pnpm gen:types && git diff --exit-code src/ipc/generated   # generated types are current
pnpm typecheck && pnpm lint && pnpm test && pnpm check:core-deps
pnpm tauri dev                         # on Windows
```
Live-service tests are ignored by default: `cargo test --features live -- --ignored` (needs the keys in `.env`).
