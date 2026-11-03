# PRD — Pluto Voice (working name)

**Event:** Gnani AI Great Indian AI Internship Challenge 2026. Deadline **10 Nov 2026, 11:59 PM IST**.
**Team:** exactly 2 developers (Developer 1, Developer 2).
**Build:** a desktop app built **on top of a base ported from the owner's earlier Pluto app** (PRD D-12). The voice, meeting and LLM layers are new. See `PORTING_FROM_PLUTO.md` for exactly what was copied. Contracts and tasks in this pack are being re-baselined against that base.

---

## 1. Problem
1. **Meeting outcomes are lost.** Indian teams, student groups and freelancers hold meetings in code-mixed speech (Hinglish, Tanglish, Kannada-English). Decisions, action items, owners and deadlines are written up by hand or not at all.
2. **Past meetings can't be questioned.** "Who promised what, by when?" can't be answered without replaying recordings.
3. **Productivity tools are English-first and keyboard-first.** Capturing tasks, notes and reminders, or retrieving them, means switching windows and typing.

**Competitive gap [Verified, vendor pages]:**
- Otter supports 6 languages and not Hindi.
- Fireflies supports Hindi, with multi-language mode in beta.
- Neither turns meetings into tasks in a local productivity app, or answers questions about meetings by voice in Indian languages.

## 2. Target users
| Persona | Primary need |
|---|---|
| Student project team member | Owners and deadlines from code-mixed meetings captured automatically; ask "what did we decide?" |
| Startup / SME team lead | Standup action items become tracked tasks with source quotes |
| Freelancer | Client commitments and deadlines are never missed |

## 3. Product
Two core capabilities share one voice runtime and one local database:
1. **Meeting Assistant (primary):**
   - Record or import meetings.
   - Speaker-labelled, code-mixed transcription.
   - Extract decisions, action items, owners, deadlines, questions and risks, each linked to its exact quote.
   - Approve the items into tasks.
   - Spoken recaps, voice recall and a commitment tracker.
2. **Voice Assistant (common interface):** manage tasks, reminders, notes, focus sessions and projects, and ask questions about meetings, by speaking in any supported language mix. Replies are spoken in the same language.

**Personal productivity module** (provided by the ported Pluto base; to be adapted): tasks, reminders, notes, focus sessions and projects. Meeting records and personal records stay separate. Meeting tasks come only from approved action items.

## 4. Goals and success metrics
Measured on synthetic, scripted audio.

| Goal | Metric | Target |
|---|---|---|
| Accurate meeting capture | Action items correct (text + owner + due date) | ≥ 85% |
| Correct speakers | Segment speaker correct after the user maps speakers to participants | ≥ 90% |
| Responsive assistant | End of speech → first audio of the reply (median) | ≤ 3 s |
| Correct routing | Voice command reaches the correct action | ≥ 90% |
| Safety | Writes without explicit approval | 0 |
| Clear Gnani advantage | Errors on names, numbers and dates: Prisma vs a baseline engine on the same clips | Reported, not assumed |

## 5. Requirements and priorities

**P0** = MVP and demo-critical. **P1** = planned, cut first if late. **P2** = stretch, only after P1 is complete.

### 5.1 Meeting Assistant
| ID | Requirement | P |
|---|---|---|
| MA-01 | Record from the microphone after a consent confirmation. Indicator, pause/stop | P0 |
| MA-02 | Import an audio file (wav, mp3, mp4, m4a, flac, ogg, opus, aac, webm, amr), up to 10 MB | P0 |
| MA-03 | Batch transcription (Prisma v3.0): speaker separation, `codemix`, participant names and glossary hints | P0 |
| MA-04 | Map speaker labels to participants | P0 |
| MA-05 | Live captions while recording (Prisma Realtime) | P0 |
| MA-06 | Extract decisions, action items (owner, due date, priority), questions and risks, each with a source segment and quote | P0 |
| MA-07 | Review and approve. Approved action items can become tasks | P0 |
| MA-08 | Voice recall with a spoken answer and citation. A follow-up change goes through approval | P0 |
| MA-09 | Spoken recap (Timbre v3.0) | P0 |
| MA-10 | Quality warnings (no decisions, item without owner, unresolved date) | P1 |
| MA-11 | Live action-item ticker while recording | P1 |
| MA-12 | Commitment tracker across meetings | P1 |
| MA-13 | Recap exported as mp3 per chosen language | P1 |
| MA-14 | Accuracy comparison view (Prisma vs a baseline on the same clip) | P1 |
| MA-15 | Pre-meeting briefing for a set of participants | P2 |
| MA-16 | Recap in the user's own cloned voice (own voice only, with consent) | P2 |
| MA-17 | Follow-up message drafts per owner | P2 |
| MA-18 | Meeting analytics (talk time, decisions per meeting) | P2 |
| MA-19 | Folder watch for new recordings | P2 |
| MA-20 | Windows system-audio capture (WASAPI loopback, needs native capture) | P2 |

### 5.2 Personal productivity
| ID | Requirement | P |
|---|---|---|
| PP-01 | Tasks: create, edit, complete, delete, due date, priority, list/filter (Today, Upcoming, All) | P0 |
| PP-02 | Reminders: create, list, complete, desktop notification when due | P0 |
| PP-03 | Notes: create, edit (Markdown), search | P1 |
| PP-04 | Focus sessions: start, pause, stop, link to a task, history | P1 |
| PP-05 | Projects: group tasks and notes, project view | P2 |

### 5.3 Voice Assistant
| ID | Requirement | P |
|---|---|---|
| VA-01 | Push-to-talk on a global hotkey. Listening / thinking / speaking states | P0 |
| VA-02 | Meeting queries by voice (MA-08) | P0 |
| VA-03 | Voice actions on tasks and reminders | P0 |
| VA-04 | Voice actions on notes (dictation in `codemix` or `transliterate`) and focus | P1 |
| VA-05 | Voice actions on projects | P2 |
| VA-06 | Approval gate for every write, by voice ("haan" / "yes") or click | P0 |
| VA-07 | Reply language follows the detected language | P0 |
| VA-08 | Expressive voice using Timbre audio tags | P1 |
| VA-09 | Daily briefing on request | P2 |
| VA-10 | Optional always-on mode with an on-device wake word | P2 |

### 5.4 Platform
| ID | Requirement | P |
|---|---|---|
| PL-01 | Opt-in "Cloud voice" with a disclosure of what is sent where | P0 |
| PL-02 | API keys (Gnani, Gemini, Groq) in the OS keychain only | P0 |
| PL-03 | Privacy log of outbound requests | P1 |
| PL-04 | Team glossary → Prisma `keyterms` | P1 |
| PL-05 | Usage counters (audio seconds, TTS characters, LLM calls) | P1 |

## 6. Non-functional requirements
| ID | Requirement |
|---|---|
| NF-01 | No network calls from the web UI. All external calls go from Rust. The CSP allows IPC only |
| NF-02 | No audio leaves the device without opt-in plus an explicit user action |
| NF-03 | Gnani retries follow Gnani's guidance: connection errors and 429/500/503/504 with backoff; no retry on other 4xx |
| NF-04 | Synthetic data only, in the app and the demo (challenge rule) |
| NF-05 | Windows is the demo and target platform. macOS builds are best effort |
| NF-06 | Every data change is undoable or confirmable. Writes from voice always go through a proposal |

## 7. Non-goals
- Real-time translation (Gnani has no translation API).
- Joining online calls as a bot.
- Warp speech-to-speech (no published API).
- Gnani Agent Builder (excluded by the challenge rules).
- Offline speech-to-text or text-to-speech. **[Verified]** Gnani's Prisma and Timbre are hosted only; no weights are published.
- Any third-party decision classifier. Decisions use rules, then the LLM.
- Cloning anyone else's voice.
- Recording without an explicit user action.

## 8. MVP
All P0 requirements, working end to end on the Windows demo machine with synthetic Hinglish and Tanglish meetings.

## 9. Acceptance criteria
1. A 3-person code-mixed meeting is recorded or imported. It becomes a transcript, an approved meeting record and created tasks, with no typing other than speaker mapping and approvals.
2. "Priya ne login fix kab tak bola tha?" (By when did Priya say she'd fix the login?) gets a spoken answer quoting the source. "Monday kar do" (make it Monday) updates the task after approval.
3. "Kal shaam 5 baje DBMS submit karne ka reminder lagao" (set a reminder for 5 PM tomorrow to submit DBMS) produces a proposal, then "haan" creates it, and the notification fires at the right time.
4. With Cloud voice off, there are zero outbound requests.
5. All P0 tests in `TESTING_STRATEGY.md` §7 pass on the demo machine.

## 10. Decision register (owner-approved)
| ID | Decision |
|---|---|
| D-01 | **Superseded by D-12.** The app is built on a ported Pluto base |
| D-02 | Meetings are the main feature. The Voice Assistant is the common interface |
| D-03 | Personal scope: tasks, reminders, notes, focus and projects, built in tiers (tasks + reminders P0 → notes + focus P1 → projects P2) |
| D-04 | Stack: **Tauri v2 hybrid.** React/TypeScript UI does audio capture and playback with browser APIs (WebView2). Rust does Gnani, LLM, storage and logic. Details in ARCHITECTURE §3 |
| D-05 | Speech: Gnani Prisma v3.0 (Batch, Realtime, REST) and Timbre v3.0 |
| D-06 | Reasoning: free-tier hosted LLMs, split by job. **Meetings:** Gemini first, Groq fallback. **Voice commands:** Groq first, Gemini fallback |
| D-07 | Decisions: rules first, then the LLM |
| D-08 | Every write needs explicit approval |
| D-09 | Primary award target: Mixed Vibes |
| D-10 | Gnani questions (credits, concurrency, streaming hints, presigned upload) are asked on Discord but don't block the build. Each has a fallback in ARCHITECTURE §13 |
| D-12 | **Port approved by the owner (Developer 1 agreed):** the working base of Pluto (Rust core, shell, UI, scripts, build configs, migrations) was copied into this repository as-is on branch `chore/port-pluto-base`. The owner holds the rights (QUORBIT TECHNOLOGIES is the owner's company). The submission does not mention the port (owner's decision). After the base is verified, the team re-plans what to keep, prune, adapt and build. Details: `PORTING_FROM_PLUTO.md` |
| D-13 | **Superseded by D-12** (the UI theme and base components are part of the port) |
| D-11 | Work is divided by component between Developer 1 and Developer 2 (`HACKATHON_PLAN.md` §2, `PROJECT_ROADMAP.md` board) |

## 11. Open items (non-blocking)
| ID | Item | Fallback while open |
|---|---|---|
| G-1 | Credit conversion (STT minutes, TTS characters) | Local usage counters; short test clips |
| G-2 | "1 active request at a time" scope and the concurrent Realtime session limit | Request queues; assistant shows "Busy" during recording |
| G-3 | Whether Realtime accepts `topic` / `speaker_names` | Use `keyterms` only on Realtime |
| G-4 | Presigned upload path | WAV chunks ≤ 4.5 min; imports ≤ 10 MB |
| L-1 | Exact free-tier limits for Gemini (shown in AI Studio) and Groq (shown in the Groq console) | Fallback provider + backoff; record limits on Day 1 |
| N-1 | Final app name and repository name | Working name "Pluto Voice" |
