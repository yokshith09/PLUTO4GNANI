# Developer 1 — Task Order

**Owns:** platform, Gnani I/O, voice runtime and tools, personal module, build and submission.
**Board (acceptance criteria, tests, status):** [`../../PROJECT_ROADMAP.md`](../../PROJECT_ROADMAP.md) §4.
**Specs:** [`FEATURE_SPECS_DEV1.md`](FEATURE_SPECS_DEV1.md). **Contracts:** [`../common/`](../common/).

Work top to bottom. Each item lists the tasks it waits on from Developer 2.

| # | Task | Waits on Developer 2 |
|---|---|---|
| 1 | T-001 Repo, workspace, CI | — |
| 2 | T-003 Shared types (Developer 2 reviews) | — |
| 3 | T-005 Schema + migration runner | — |
| 4 | T-009 `audio_push` + playback Channel | — |
| 5 | T-013 Gnani Day-1 check | — |
| 6 | T-006 Stub commands + events | — |
| 7 | T-011 **Joint mic/IPC check** (you own the result) | T-010 |
| 8 | T-101 Prisma Realtime client | — |
| 9 | T-102 Timbre client | — |
| 10 | T-103 Queues, retries, error mapping, privacy log | — |
| 11 | T-105 Keychain + settings commands | — |
| 12 | T-106 Tasks backend | — |
| 13 | T-107 Reminders backend + scheduler | — |
| 14 | T-201 Recorder | — |
| 15 | T-202 Live captions | — |
| 16 | T-203 Batch client + import | — |
| 17 | T-301 Voice runtime + hotkey | — |
| 18 | T-303 Tool registry, proposals, approval, execution | T-302 |
| 19 | T-306 **Checkpoint C3** (B1/B2) | T-304, T-305 |
| 20 | T-402 Tasks, Reminders and Today pages | — |
| 21 | T-404 Safety tests | — |
| 22 | T-405 Failure suite | — |
| 23 | T-407 Windows build + demo machine | — |
| 24 | P1, in cut order reversed: T-501 Notes → T-502 Focus → T-503 Export → T-504 Privacy and usage backend | — |
| 25 | T-601 Freeze `demo-v1` | — |
| 26 | T-604 Submission form | T-603 |
| — | P2 (only after C5): T-701, T-702, T-703, T-704, T-705 | — |

**You review:** every Developer 2 PR touching `voice::llm`, `voice::decide`, `voice::meeting`, `mockApi.ts` or any contract.
