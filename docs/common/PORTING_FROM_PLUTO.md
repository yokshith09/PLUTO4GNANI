# Porting from Pluto — what is possible, and the rules

**Status:** nothing is ported. Decision D-12 (PRD): everything is built from scratch; a port is considered only **after checkpoint C4**.
**Source:** the owner's Pluto repository, commit `eb4d520`. MIT licence, copyright notice "QUORBIT TECHNOLOGIES".

## 1. What Pluto has (measured in the repo)
| Module | Lines | Tests | Depends on |
|---|---|---|---|
| `scheduler.rs` | 439 | 12 | infrastructure only |
| `focus.rs` | 606 | 7 | infrastructure + `tasks` |
| `notes.rs` + `labels.rs` | 1,941 + 308 | 37 | infrastructure + `labels` |
| `reminders.rs` | 1,117 | 18 | infrastructure + `notes` + `tasks` |
| `tasks.rs` + `task_plans.rs` | 2,381 + | 41 | infrastructure + `task_plans` |
| UI: tasks / notes pages | about 1,365 / 1,367 | — | shadcn components, Pluto's hooks |
| `db/migrate.rs` (forward-only runner) | 224 | — | infrastructure |
| **Infrastructure** | `bus`, `clock`, `db`, `error`, `events`, `ids` | — | needed by every module above |

**Not in Pluto at all:** audio, speech-to-text, text-to-speech, LLM calls, transcripts, meeting extraction, voice commands. None of the judged features can be ported.

## 2. Why it can't be done a module at a time
Every module uses Pluto's shared event bus, clock and database wrapper. Porting one module means porting that infrastructure as well, and then adapting it to **our** frozen contracts:
- Pluto's task statuses are `PLANNED / IN_PROGRESS / PENDING / COMPLETED / CANCELLED`; ours are `TODO / IN_PROGRESS / DONE / CANCELLED` (`DATA_SCHEMA.md`).
- Pluto's IDs, event names and error types differ from `API_CONTRACTS.md` §0 and §6.
- Pluto's own status file lists 42 items as "Unverified" on a real machine.

**[Guessing]** Porting the infrastructure plus one module takes at least a day of Developer 1's time. Do not take on that cost before the demo path works.

## 3. Candidates, best first (all only after C4)
| Candidate | Tasks it would replace | Value | Cost | Verdict |
|---|---|---|---|---|
| `scheduler.rs` + infrastructure | Reminder scheduler inside T-107 | Medium: proven timing logic and 12 tests | The infrastructure comes with it | Only if our own scheduler has problems |
| `notes.rs` + `labels.rs` (UI included) | T-501 Notes (P1) | Medium: a full notes feature | Schema differs: labels, colours and pinning do not exist in our schema | Reasonable for P1 |
| `focus.rs` (UI included) | T-502 Focus (P1) | Low to medium | Needs a task adapter | Reasonable for P1 |
| `tasks.rs`, `reminders.rs` | T-106, T-107 (P0) | Low: the P0 versions are small | Largest adaptation (status mismatch) | **Do not port. Write fresh** |
| UI pages | T-402 and others | Low | Different hooks and components | **Do not port** |

## 4. Rules for any port
1. **Both developers agree**, and the PRD decision register is updated before any code is copied.
2. **Confirm who holds the rights.** The MIT notice names QUORBIT TECHNOLOGIES; the repository shows one contributor. Confirm before copying, and tell Developer 1.
3. **Keep the MIT notice** with the ported files.
4. **Port in its own PR** (`chore/port-<module>`), with the source commit and files listed in the description.
5. **Adapt to our contracts**, not the other way round. Ported tests must pass against our schema.
6. **Disclose it** in the submission (HACKATHON_PLAN checklist) and be ready to explain every ported file in the interview.
7. **Never port anything for voice, audio, LLM or meetings.**
