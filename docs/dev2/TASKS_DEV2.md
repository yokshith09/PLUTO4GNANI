# Developer 2 — Task Order

**Owns:** UI audio, LLM and decisions, meeting intelligence, meeting/voice/settings UI, measurement, demo.
**Board (acceptance criteria, tests, status):** [`../../PROJECT_ROADMAP.md`](../../PROJECT_ROADMAP.md) §4.
**Specs:** [`FEATURE_SPECS_DEV2.md`](FEATURE_SPECS_DEV2.md). **Contracts:** [`../common/`](../common/).

Work top to bottom. Each item lists the tasks it waits on from Developer 1.

| # | Task | Waits on Developer 1 |
|---|---|---|
| 1 | T-002 UI shell: you create and own `ui/` (runs in the browser, no desktop shell needed) | — |
| 2 | T-003b Meeting, LLM and decision types + `Llm` and `Decider` traits in `crates/types` (Developer 1 approves); then T-004 `crates/intel` + MockLlm | T-001a |
| 3 | T-008 Test scripts + ground truth (fill gaps with this) | — |
| 4 | T-007 `client.ts` + `mock/{meetings,voice,settings,index}.ts`; leave `mock/personal.ts` empty for Developer 1 | T-003 |
| 4a | T-015 Fixture round-trip tests | T-003b |
| 5 | T-010 Capture worklet + player | — |
| 6 | T-011 Joint mic/IPC check (support Developer 1) | T-009 |
| 7 | T-012 Gemini/Groq Day-1 check, model choice | — |
| 8 | T-104 LLM router + fallback | — |
| 9 | T-204 Meetings list, record dialog, live view (on mocks) | — |
| 10 | T-206a Relevance | — |
| 11 | T-304 Voice overlay, proposal card, history (on mocks) | — |
| 12 | T-205 Transcript + Speakers tabs | T-203 for live data (use the fixture until then) |
| 13 | T-206b Extraction + validator | — |
| 14 | T-207 Review + apply → tasks | T-106 |
| 15 | T-208 Recap | T-102 |
| 16 | T-209 **Checkpoint C2** (A1) | T-201, T-203 |
| 17 | T-302 Rules + intent | — |
| 18 | T-305 Recall | — |
| 19 | T-403 Settings UI | T-105 |
| 20 | T-406 Measurements → `docs/results/` | C3 |
| 21 | T-408 **Checkpoint C4** (E2E §7) | T-402..T-407 |
| 22 | P1, in cut order reversed: T-505 Warnings → T-509 Glossary/privacy UI → T-510 Audio tags → T-506 Commitments → T-507 Ticker → T-508 Comparison; then T-511 C5 | T-504 for T-509 |
| 23 | T-602 Rehearsal ×2 | T-601 |
| 24 | T-603 Video + post | — |
| — | P2 (only after C5): T-706, T-707, T-708, T-709 | — |

**You review:** every Developer 1 PR touching contracts, `crates/types` (their files), migrations, `app/src/commands/**`, or `voice::runtime` events.
**Rules you follow:** [`../common/WORKING_AGREEMENT.md`](../common/WORKING_AGREEMENT.md). Do not edit `crates/{core,voice}/**`, `app/**`, root configs or `ui/src/features/personal/**` without a PR D1 approves.
**Before D1's skeleton lands:** work in `ui/`, `test-data/` and a scratch script for T-012. Do not create `Cargo.toml` files yet; wait for T-001a (about an hour).
