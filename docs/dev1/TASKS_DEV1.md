# Developer 1 — Task Order

**Owns:** the skeleton and contracts A, audio I/O (UI and Rust), Prisma Realtime, Timbre, the Gnani base and Batch client, the voice runtime and hotkey, the assistant (tools and approval), safety checks, the Windows build, freeze and submission.

**Board (acceptance criteria, tests):** [`../../PROJECT_ROADMAP.md`](../../PROJECT_ROADMAP.md) §4. **Rules:** [`../common/WORKING_AGREEMENT.md`](../common/WORKING_AGREEMENT.md). **Specs:** [`FEATURE_SPECS_DEV1.md`](FEATURE_SPECS_DEV1.md).

**Your branch:** `D1`. Pull `main` into it at the start of every day; open a PR into `main` at least daily.

Work top to bottom. **Wait** = a task of the other developer that must be merged first.

| # | Task | Title | Size | Priority | Day | Wait | Status |
|---|---|---|---|---|---|---|---|
| 1 | T-002 | Base | M | P0a | 0–1 | — | Done (`docs/results/d1-base-verification.md`) |
| 2 | T-003 | Skeleton | M | P0a | 1 | — | Done (merged, PR #3) |
| 3 | T-004 | Contracts A | M | P0a | 1 | — | In review |
| 4 | T-007 | Shell stubs | S | P0a | 1 | T-005 | To do |
| 5 | T-010 | Gnani check | S | P0a | 1 | — | To do |
| 6 | T-011 | Mic check | S | P0a | 2 | — | To do |
| 7 | T-012 | UI audio | L | P0a | 1–2 | — | To do |
| 8 | T-013 | Rust audio IPC | M | P0a | 1 | — | To do |
| 9 | T-101 | Prisma Realtime | L | P0a | 2 | — | To do |
| 10 | T-102 | Timbre | M | P0a | 2 | — | To do |
| 11 | T-103 | Gnani base | M | P0a | 2 | — | To do |
| 12 | T-107 | Migration 013 | S | P0a | 2 | — | To do |
| 13 | T-201 | Batch | L | P0a | 3 | T-106 | To do |
| 14 | T-301 | Voice runtime | L | P0a | 3–4 | — | To do |
| 15 | T-303 | Assistant | L | P0a | 4 | T-302 | To do |
| 16 | T-306 | Integration C3 | S | P0a | 4 | — | To do |
| 17 | T-403 | Safety | S | P0a | 4–5 | — | To do |
| 18 | T-404 | Build | S | P0a | 5 | — | To do |
| 19 | T-601 | Freeze | S | P0a | 5 | T-406 | To do |
| 20 | T-604 | Submission | S | P0a | 6–7 | T-603 | To do |

**P1 (after C4, only if on schedule):** T-506 Privacy log, T-508 More voice tools, T-509 mp3 export, T-511 Failure suite, T-513 macOS mic

**P2 (own branch state, never in the demo build):** T-701, T-702, T-703, T-704, T-705

**Effort (estimates):** P0a before freeze 50 h · after freeze 2 h · P0b 0 h · P1 8 h.
