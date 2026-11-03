# Hackathon Plan — Pluto Voice

**Team:** exactly 2 developers.
**Task board:** [`PROJECT_ROADMAP.md`](PROJECT_ROADMAP.md) §4 (single source for tasks).
**Contracts:** [`docs/common/`](docs/common/).

This plan improves the odds of a working demo. It cannot guarantee one: external services (Gnani, Gemini, Groq) and unknown limits remain risks, and §5 lists the fallbacks.

## Current Focus
**M0 — Foundation and contracts.** First tasks:
- **D1:** T-001a (merge within the first hour) → T-003 → T-005 / T-009 / T-013 → T-006 → T-001b.
- **D2:** T-002, T-008 and T-012 from minute 0 (no dependency on D1) → T-003b / T-004 → T-007 / T-010.

## 1. Team allocation
Team skills are unknown, so ownership follows integration boundaries. Each side can be built and tested against mocks of the other.

| | **Developer 1 — Platform, Gnani I/O, runtime, personal module** | **Developer 2 — UI audio, LLM and decisions, meeting intelligence, voice UX** |
|---|---|---|
| Owns (Rust) | `crates/core` (DB, tasks, reminders, notes, focus, projects), `crates/voice` (gnani, runtime, assistant), `app` shell, `crates/types` (personal, voice, speech files) | `crates/intel` (llm, decide, meeting), `crates/types` (meeting, llm, decide files) |
| Owns (UI) | Today, Tasks, Reminders, Notes, Focus, Projects pages | `ui/src/audio`, Meetings pages, Voice page, overlay and proposal card, Settings |
| Owns (process) | Repo, CI and contract checks, schema, build and installer, submission form | Test scripts and ground truth, measurements, demo rehearsal, video and post |
| Spec | [`docs/dev1/`](docs/dev1/) | [`docs/dev2/`](docs/dev2/) |

**Rebalancing rule:** at C1, if one developer's critical-path tasks are behind by ≥ 1 L task, move the next unstarted UI page to the other developer, with no contract change.

## 2. Dependency order
1. **Contracts before code.** T-003 (types), T-004 (traits), T-005 (schema) and T-006/T-007 (stubs, mocks) must merge before any dependent task starts.
2. **Riskiest first.** T-011 (WebView2 mic + binary IPC), T-012 (LLM JSON) and T-013 (Gnani key) run in M0. A failure changes the plan on day one, not in week two.
3. **The meeting journey (M2) is integrated before the assistant journey (M3)**, and both before personal P1.

Full graph: PROJECT_ROADMAP §5.

## 3. Timeboxed milestones
Relative days from the start of build. Integration happens at the end of every milestone, never only at the end of the project.

| Day | Milestone | D1 | D2 | Checkpoint (owner) |
|---|---|---|---|---|
| 1 | M0 | T-001a, T-003, T-005, T-009, T-013, T-006, T-001b, T-014 | T-002, T-008, T-012, T-003b, T-004, T-015, T-007, T-010 | **C0** (D1): app builds; contracts frozen; Day-1 checks recorded (T-011 joint) |
| 2 | M1 | T-101, T-102, T-103, T-105, T-106, T-107 | T-104, T-204, T-206a, T-304 (mocks) | **C1** (D1): live caption smoke; TTS plays; LLM router falls back |
| 3 | M2 | T-201, T-202, T-203 | T-205, T-206b, T-207, T-208 | **C2** (D2): journey A1 live → tasks created |
| 4 | M3 | T-301, T-303 | T-302, T-305, T-304 live | **C3** (D1): journeys B1, B2 live |
| 5 | M4 | T-402, T-404, T-405, T-407 | T-403, T-406, then T-408 | **C4** (D2): E2E §7 passes on the demo machine |
| 6 | M5 + buffer | T-501..T-504 (in cut order) | T-505..T-510 (in cut order), T-511 | **C5** (D2): E2E still passes; cut list recorded |
| 7 | M6 | T-601, then fixes only | T-602, T-603 | Demo posted |
| 8 | M6 + buffer | T-604 | Support | Submitted. M7 only if everything above is done |

**Protected time:**
- Half of day 6 is for fixing, not features.
- Day 7 morning is rehearsal.
- Day 8 is a buffer.
- Installer build and demo-machine setup (T-407) are on day 5, not on demo day.

## 4. Engineering workflow
**SDLC per task:** Plan (read the spec and contracts) → Definition of Ready check → Implement on a branch → Test → PR → Review → Merge to `develop` → Integrate at the checkpoint → Verify against the acceptance criteria → Update the board.

All rules are in [`docs/common/WORKING_AGREEMENT.md`](docs/common/WORKING_AGREEMENT.md): ownership map (§2), shared-file rules (§3), branches, commits and PRs (§4), contract changes (§5), integration-error rules (§6), communication (§7). Summary:

| Item | Rule |
|---|---|
| Branches | `main` (demo-ready) ← `develop` (always green) ← `feat/T-xxx-name`, `fix/…`, `contract/…`, `chore/…` |
| PRs | One task ID, ≤ 400 lines, reviewed by the other developer within 2 working hours, squash-merged by the author |
| Cadence | Merge to `develop` at least daily; `develop` → `main` only at C0, C2, C3, C4, C5; 6 PM daily full check of `develop` |
| Contracts | Change only through a `contract/` PR approved by both; fixtures, mocks and generated types change in the same PR |
| Stop-the-line | If `develop` is red, the next commit from either developer is the fix |

## 5. Risks and fallbacks
| Risk | Trigger | Fallback | Owner |
|---|---|---|---|
| WebView2 mic or binary IPC fails | T-011 fails | T-011F: Rust capture with `cpal`; contracts unchanged | D1 |
| LLM free-tier limits too low | T-012 limits or 429s in tests | Router fallback; smaller batches; change model IDs in settings | D2 |
| Gnani 1-request limit (G-2) | 429 under load | Request queues; assistant shows "Busy" while recording | D1 |
| Gnani credits run out (G-1) | 402 | Usage counters; short clips; record the demo early | D1 |
| Large uploads (G-4) | Import over 10 MB | WAV chunks ≤ 4.5 min; import limit 10 MB | D1 |
| Extraction accuracy below target | T-ML-02 < 85% | Prompt and schema iteration; add glossary keyterms; report honestly | D2 |
| Code-mixed advantage not visible | T-CMP-01 shows no gap | Drop the comparison from the demo; lean on extraction and recall | D2 |
| A checkpoint slips by more than half a day | Missed exit criterion | Apply §6 cut rules immediately | Both |

## 6. Scope-cut rules
1. **C4 not passed by the end of day 5:** no P1 work starts. Both developers work only on the demo-critical path.
2. **P1 cut order (first cut first):** T-508 comparison → T-507 ticker → T-506 commitments → T-503 export → T-502 focus → T-501 notes.
3. **M7 (P2)** starts only after C5. Work goes on separate branches and is never merged into `demo-v1`.
4. **Never cut:** consent, the approval gate, cloud-voice opt-in, citations on extracted items, E2E §7.

## 7. Demo-critical path
```
T-001a → T-003/T-003b/T-005 → T-006/T-007 → T-011 (mic) → T-101 (Realtime) → T-201 (record) → T-203 (Batch)
→ T-205 (speakers) → T-206a/b (extract) → T-207 (apply → tasks) → C2
→ T-301 (runtime) → T-302 (decide) → T-303 (tools + approval) → T-305 (recall) → C3
→ T-402/T-403 → T-408 (E2E, C4) → T-601 (freeze) → T-602 (rehearse) → T-603 (post) → T-604 (submit)
```

## 8. Submission checklist
- [ ] Working app on Gnani APIs: Prisma (Batch, Realtime, REST), Timbre v3.0.
- [ ] Models named. Gemini and Groq listed as third-party reasoning models.
- [ ] Each developer's contribution listed (from §1).
- [ ] Demo posted on LinkedIn and X with **#GnaniAI** and **#GreatIndianAIInternshipChallenge**, tagging Gnani AI; live before submission; one post nominated per platform.
- [ ] Synthetic data only; any mocked or pre-recorded segment disclosed.
- [ ] Submitted before 11:59 PM IST, 10 Nov 2026.
