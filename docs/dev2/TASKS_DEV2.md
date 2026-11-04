# Developer 2 — Task Order

**Owns:** base verification, contracts B, UI plumbing, test data, the LLM layer and secrets, migration 012, meeting intelligence (extraction, review, recall, recap), the Meetings and Voice UI, settings, measurements, rehearsal and the post.

**Board (acceptance criteria, tests):** [`../../PROJECT_ROADMAP.md`](../../PROJECT_ROADMAP.md) §4. **Rules:** [`../common/WORKING_AGREEMENT.md`](../common/WORKING_AGREEMENT.md). **Specs:** [`FEATURE_SPECS_DEV2.md`](FEATURE_SPECS_DEV2.md).

**Your branch:** `D2`. Pull `main` into it at the start of every day; open a PR into `main` at least daily.

Work top to bottom. **Wait** = a task of the other developer that must be merged first.

| # | Task | Title | Size | Priority | Day | Wait | Status |
|---|---|---|---|---|---|---|---|
| 1 | T-001 | Base | S | P0a | 0 | — | In progress (CI green on Windows and macOS; local run pending) |
| 2 | T-005 | Contracts B | M | P0a | 1 | T-003 | Done (merged, PR #8) |
| 3 | T-006 | UI plumbing | M | P0a | 1 | T-004 | Done (merged, PR #13) |
| 4 | T-008 | Test data | M | P0a | 1–2 | — | Done (scripts and ground truth merged, PR #13; audio recordings pending) |
| 5 | T-009 | LLM check | S | P0a | 1 | — | To do |
| 6 | T-104 | LLM | M | P0a | 2 | — | Done (merged, PR #14; model IDs final after T-009) |
| 7 | T-105 | Secrets | S | P0a | 1–2 | T-003 | Done (keychain store merged, PR #13; `voice.*` and `llm.*` settings keys in this PR) |
| 8 | T-106 | Migration 012 | M | P0a | 1–2 | T-003 | Done (merged, PR #8) |
| 9 | T-202 | Meetings UI | L | P0a | 2–3 | — | Done (merged, PR #14; live data after T-201) |
| 10 | T-203 | Extraction | L | P0a | 3 | — | Done (merged, PRs #14 and #18; live accuracy T-ML-02 needs keys and T-201) |
| 11 | T-204 | Review and apply | L | P0a | 3 | — | Done (merged, PRs #14 and #18; needs a transcript from T-201) |
| 12 | T-205 | Recap | S | P0a | 3 | T-102 | Done (merged, PR #19; reading it aloud waits for T-102) |
| 13 | T-206 | Integration C2 | S | P0a | 3 | — | To do |
| 14 | T-207 | Recorder | L | P0b | 3–4 | T-013 | To do |
| 15 | T-302 | Decisions | M | P0a | 3 | — | Done (merged, PR #14; routing accuracy on the real model (T-ML-04) needs keys) |
| 16 | T-304 | Voice UI | M | P0a | 3–4 | — | To do |
| 17 | T-305 | Recall | L | P0a | 4 | — | In progress (recall merged, PR #19; spoken answer and the follow-up proposal wait for T-303) |
| 18 | T-401 | Voice settings | M | P0a | 4–5 | — | In progress (Voice and privacy section merged, PR #21; hotkey row and voice preview wait for D1's runtime) |
| 19 | T-402 | Demo build | S | P0a | 4 | — | In review (PR; one setting hides Time, Study, Character, MCP and Archive; Bin stays so a deleted note can be restored) |
| 20 | T-405 | Measurement | M | P0b | 5 | T-306 | To do |
| 21 | T-406 | Integration C4 | S | P0a | 5 | — | To do |
| 22 | T-602 | Rehearsal | M | P0a | 5 | T-601 | To do |
| 23 | T-603 | Video and post | M | P0a | 5 | — | To do |

**P1 (after C4, only if on schedule):** T-501 Live captions, T-502 Warnings, T-503 Commitments, T-504 Comparison, T-505 Glossary, T-507 Privacy UI, T-510 Audio tags, T-512 Integration C5

**P2 (own branch state, never in the demo build):** T-706, T-707, T-708, T-709

**Effort (estimates):** P0a before freeze 49 h · after freeze 5 h · P0b 8 h · P1 12 h.
