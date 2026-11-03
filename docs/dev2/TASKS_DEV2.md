# Developer 2 — Task Order

**Owns:** base verification, contracts B, UI plumbing, test data, the LLM layer and secrets, migration 012, meeting intelligence (extraction, review, recall, recap), the Meetings and Voice UI, settings, measurements, rehearsal and the post.

**Board (acceptance criteria, tests):** [`../../PROJECT_ROADMAP.md`](../../PROJECT_ROADMAP.md) §4. **Rules:** [`../common/WORKING_AGREEMENT.md`](../common/WORKING_AGREEMENT.md). **Specs:** [`FEATURE_SPECS_DEV2.md`](FEATURE_SPECS_DEV2.md).

**Your branch:** `D2`. Pull `main` into it at the start of every day; open a PR into `main` at least daily.

Work top to bottom. **Wait** = a task of the other developer that must be merged first.

| # | Task | Title | Size | Priority | Day | Wait |
|---|---|---|---|---|---|---|
| 1 | T-001 | Base | S | P0a | 0 | — |
| 2 | T-005 | Contracts B | M | P0a | 1 | T-003 |
| 3 | T-006 | UI plumbing | M | P0a | 1 | T-004 |
| 4 | T-008 | Test data | M | P0a | 1–2 | — |
| 5 | T-009 | LLM check | S | P0a | 1 | — |
| 6 | T-104 | LLM | M | P0a | 2 | — |
| 7 | T-105 | Secrets | S | P0a | 1–2 | T-003 |
| 8 | T-106 | Migration 012 | M | P0a | 1–2 | T-003 |
| 9 | T-202 | Meetings UI | L | P0a | 2–3 | — |
| 10 | T-203 | Extraction | L | P0a | 3 | — |
| 11 | T-204 | Review and apply | L | P0a | 3 | — |
| 12 | T-205 | Recap | S | P0a | 3 | T-102 |
| 13 | T-206 | Integration C2 | S | P0a | 3 | — |
| 14 | T-207 | Recorder | L | P0b | 3–4 | T-013 |
| 15 | T-302 | Decisions | M | P0a | 3 | — |
| 16 | T-304 | Voice UI | M | P0a | 3–4 | — |
| 17 | T-305 | Recall | L | P0a | 4 | — |
| 18 | T-401 | Voice settings | M | P0a | 4–5 | — |
| 19 | T-402 | Demo build | S | P0a | 4 | — |
| 20 | T-405 | Measurement | M | P0b | 5 | T-306 |
| 21 | T-406 | Integration C4 | S | P0a | 5 | — |
| 22 | T-602 | Rehearsal | M | P0a | 5 | T-601 |
| 23 | T-603 | Video and post | M | P0a | 5 | — |

**P1 (after C4, only if on schedule):** T-501 Live captions, T-502 Warnings, T-503 Commitments, T-504 Comparison, T-505 Glossary, T-507 Privacy UI, T-510 Audio tags, T-512 Integration C5

**P2 (own branch state, never in the demo build):** T-706, T-707, T-708, T-709

**Effort (estimates):** P0a before freeze 49 h · after freeze 5 h · P0b 8 h · P1 12 h.
