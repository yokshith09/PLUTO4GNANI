# Hackathon Plan — Pluto Voice

**Team:** exactly 2 developers. **Deadline:** Tue 10 Nov 2026, 11:59 PM IST (fixed).
**Task board:** [`PROJECT_ROADMAP.md`](PROJECT_ROADMAP.md) §4 (single source). **Rules:** [`docs/common/WORKING_AGREEMENT.md`](docs/common/WORKING_AGREEMENT.md).

This plan improves the odds of a working demo. It cannot guarantee one: Gnani, Gemini and Groq limits and the Windows shell are still unverified, and §5 lists the fallbacks.

## Current Focus
**M0 — Base verified, skeleton and contracts.**
- **D2:** T-001 tonight → T-009, T-008, T-005, T-006, T-105, T-106.
- **D1:** T-002 → **T-003 (merge within the first hour of Day 1)** → T-004 → T-007, T-013, T-010, T-012.

## 1. Team allocation
Skills are unknown, so ownership follows integration boundaries; each side can be built and tested against mocks of the other.

| | **Developer 1 — platform, audio, Gnani, voice runtime, assistant** | **Developer 2 — LLM, meeting intelligence, UI, demo** |
|---|---|---|
| Branch | `D1` | `D2` |
| Rust | `src-tauri` shell, `pluto-core` (existing), `pluto-contracts` (voice, speech, error), `pluto-gnani` (http, realtime, tts, batch), `pluto-voice`, migration 013 | `pluto-contracts` (meeting, llm, decide), `pluto-intel` (llm, decide, meeting, recorder), migration 012, secrets |
| UI | `src/audio` (capture worklet, player) | Meetings, Voice, Settings, plumbing, hiding pages |
| Process | CI, build and installer, freeze, submission form | Test data, measurements, rehearsal, video and post |
| Lists | [`docs/dev1/`](docs/dev1/) | [`docs/dev2/`](docs/dev2/) |

**Rebalancing rule:** at C1, if one developer's P0a work is behind by one L task or more, move the next unstarted task that has no contract impact to the other developer.

## 2. Dependency order
1. **Contracts before code.** T-003 (skeleton), T-004/T-005 (types), T-006/T-007 (UI plumbing and stubs) merge before anything that depends on them.
2. **Riskiest first.** T-011 (mic in WebView2), T-009 (LLM JSON) and T-010 (Gnani key) run in M0. A failure changes the plan on Day 1.
3. **The meeting journey (M2) is integrated before the assistant journey (M3)**, and both before any P1.

Full graph: PROJECT_ROADMAP §6.

## 3. Schedule
Day 0 is tonight. **Integration happens at the end of every milestone**, never only at the end.

| Day | Date | D1 | D2 | Checkpoint |
|---|---|---|---|---|
| 0 | Tue 3 Nov | T-002 (verify, read the first CI runs) | T-001 (verify on Windows) | — |
| 1 | Wed 4 Nov | **T-003 first**, T-004, T-007, T-013, T-010, T-012 | T-005, T-006, T-009, T-105, T-106, T-008 | **C0** — contracts on `main`, stubs callable, Day-1 checks recorded |
| 2 | Thu 5 Nov | T-011, T-101, T-102, T-103, T-107 | T-104, T-008 (finish), T-202 on mocks | **C1** — live clip → segments; TTS plays; router falls back. **Velocity check** |
| 3 | Fri 6 Nov | T-201, start T-301 | T-203, T-204, T-205, T-202 live, start T-207 | **C2** — import → transcript → approved tasks |
| 4 | Sat 7 Nov | T-301, T-303, T-306 | T-302, T-304, T-305 | **C3** — push-to-talk → proposal → approval; recall answered |
| 5 | Sun 8 Nov | T-403, T-404, T-601 | T-401, T-402, T-406 (C4 by noon), T-602, T-603 | **C4** noon → freeze → rehearse → **post by 9 PM** |
| 6 | Mon 9 Nov | Fixes, P1 on `D1`, T-604 draft | Fixes, P1 on `D2`, written explanation | Demo post collects engagement |
| 7 | Tue 10 Nov | Final checks; **submit by 6 PM** | Contribution statements | **Submitted** |

**Protected time:** Day 4 evening and Day 5 morning are for fixes, not features. Nothing new merges into `main` after the freeze except fixes.

## 4. Engineering workflow
Full rules: WORKING_AGREEMENT §4. Summary:

| Item | Rule |
|---|---|
| Branches | Only `main`, `D1`, `D2` on the remote |
| Daily | Merge `main` into your branch in the morning; PR into `main` at least daily; 6 PM full check of `main` |
| PRs | Reviewed by the other developer within 2 working hours; merged by the author with a merge commit |
| Contracts | Changed only by a `contract` PR approved by both; docs, types, fixtures, mocks and registration move together |
| Stop the line | If `main` is red, the next commit from either developer is the fix |

## 5. Risks and fallbacks
| Risk | Trigger | Fallback | Owner |
|---|---|---|---|
| Windows shell does not build or run | T-001 or T-002 fails | Fix first; everything else waits. The Linux-verified core and UI tests stay valid | D2 / D1 |
| WebView2 mic or binary IPC fails | T-011 fails | `cpal` capture in Rust; contracts unchanged | D1 |
| LLM free-tier limits too low | T-009 limits or 429s | Router fallback; smaller batches; change model IDs | D2 |
| Gnani one-request limit (G-2) | 429 under load | Request queues; assistant shows "Busy" while recording | D1 |
| Gnani credits run out (G-1) | 402 | Usage counters; short clips; record the demo early | D1 |
| Large uploads (G-4) | Import over 10 MB | Imports limited to 10 MB; recorder chunks ≤ 4.5 min | D1 / D2 |
| Extraction accuracy below target | T-ML-02 < 85% | Prompt and schema iteration; glossary keyterms; report honestly | D2 |
| Code-mixed advantage not visible | T-CMP-01 shows no gap | Drop the comparison from the demo; lean on extraction and recall | D2 |
| Hot-file conflict on a merge | Conflict in a shared file | The file's owner resolves it (WORKING_AGREEMENT §2) | Owner |

## 6. Scope-cut rules
1. **Velocity check at C1 (Day 2 evening):** compare the actual time of finished tasks to the estimates. If the P0a work still to do exceeds the time left before Day 5 noon, move tasks from P0a to P0b in this order: T-405 measurements → T-207 recorder (demo uses import) → T-402 hide pages.
2. **C3 not passed by the end of Day 4:** no P1 starts. Both developers work only on the demo-critical path.
3. **C4 not passed by Day 5 noon:** post on Day 6 at noon instead, with P0a only.
4. **P1 cut order (first cut first):** T-504 comparison → T-501 captions → T-503 commitments → T-509 export → T-508 more voice tools.
5. **P2** starts only after C5; it never enters `demo-v1`.
6. **Never cut:** consent, the approval gate, cloud-voice opt-in, citations on extracted items, E2E §7.

## 7. Demo-critical path
```
T-003 → T-004/T-005 → T-006/T-007 → T-011 (mic) → T-101 (Realtime) → T-103 → T-201 (Batch) → T-202 (speakers)
→ T-203 (extract) → T-204 (apply → tasks) → C2
→ T-301 (runtime) → T-302 (decide) → T-303 (assistant + approval) → T-305 (recall) → C3
→ T-401..T-404 → T-406 (E2E, C4) → T-601 (freeze) → T-602 (rehearse) → T-603 (post) → T-604 (submit)
```

## 8. Submission checklist
- [ ] Working app on Gnani APIs: Prisma (Batch, Realtime), Timbre v3.0.
- [ ] Models named. Gemini and Groq listed as third-party reasoning models.
- [ ] Each developer's contribution listed (from §1).
- [ ] Demo posted on LinkedIn and X with **#GnaniAI** and **#GreatIndianAIInternshipChallenge**, tagging Gnani AI; live before submission; one post nominated per platform.
- [ ] Synthetic data only; any mocked or pre-recorded segment disclosed.
- [ ] The Pluto port is recorded in `docs/common/PORTING_FROM_PLUTO.md`. Submission wording about it is the owner's decision (PRD D-12).
- [ ] Submitted before 11:59 PM IST on 10 Nov 2026 (target 6 PM).
