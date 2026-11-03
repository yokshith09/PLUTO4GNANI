# Application Flows — Pluto Voice

Names match `API_CONTRACTS.md`.

## A. Meeting mode

### A1 Record → transcript → review → tasks (primary)
```
User : Meetings → Record → title, participants, consent ✓, captions ✓ → Start
UI   → meeting_rec_start → getUserMedia → frames → audio_push
Rust : WAV chunks (≤4.5 min) + Prisma Realtime → meeting://caption
User : Stop → meeting_rec_stop
Rust : Batch create (all chunks) → start → poll → meeting://job → segments → status NEEDS_SPEAKERS
User : map speakers → meeting_speakers_set
UI   → meeting_extract  (Gemini relevance + extraction → Rust validation)
User : edit / accept → Apply → meeting_extraction_apply → tasks created
UI   : toast "3 tasks created" → optional meeting_recap(speak=true)
```

### A2 Import
Import → file picker → `meeting_import` → continues as A1 from "Batch create".

### A3 Failure paths
| # | Situation | Flow |
|---|---|---|
| A3.1 | Consent not ticked | Start disabled; `ConsentRequired` if called directly |
| A3.2 | Network drops during recording | Captions "offline"; WAV continues; the job stays `QUEUED` until online |
| A3.3 | Realtime reaches 14 min | Silent reconnect |
| A3.4 | Mic lost | Pause, "Mic lost", Resume starts a new chunk |
| A3.5 | Import over 10 MB | `FileTooLarge` message |
| A3.6 | Batch `PARTIAL_FAILURE` | Gap markers + "Retry failed parts" |
| A3.7 | 402 credits | Voice disabled; banner |
| A3.8 | Gemini down or rate-limited | Groq fallback; if both fail, error + Retry |
| A3.9 | Item without a valid segment | Dropped and logged |
| A3.10 | Crash during recording | "Recover recording?" on the next launch |

## B. Voice Assistant

### B1 Write with approval
```
User : hold hotkey → "Kal shaam 5 baje DBMS submit karne ka reminder lagao" → release
Rust : Listening → commit → transcript (hi-IN) → rules: no match → Groq intent
       → {tool: reminder_create, args:{title:"DBMS submit", when_phrase:"kal shaam 5 baje"}}
       → Rust resolves the time → Proposal → TTS "Kal shaam 5 baje 'DBMS submit' reminder. Laga doon?"
           (Reminder 'DBMS submit' for 5 PM tomorrow. Shall I set it?)
User : hotkey → "haan" → rules: Approval → reminder_create → TTS "Ho gaya." (Done.)
```

### B2 Meeting recall + action
```
User : "Priya ne login fix kab tak bola tha?"   (By when did Priya say she'd fix the login?)
Rust : intent MeetingQuery → FTS top-8 → Gemini grounded answer + citation
       → TTS "Priya ne Friday tak bola tha — 'main Friday tak login fix kar dungi'."
           (Priya said by Friday: "I'll fix the login by Friday.")
User : "Monday kar do" → Proposal task_update(due_date = next Monday) → "haan" → done
```

### B3 Read-only
"Aaj ke tasks batao" (Tell me today's tasks) → `tasks_list(Today)` → spoken list (at most 5, then "and N more").

### B4 Personal examples
| Utterance | Tool |
|---|---|
| "Report wala task done kar do" (Mark the report task done) | `task_complete` (proposal) |
| "25 minute focus start karo DBMS pe" (Start a 25-minute focus on DBMS) | rules → `focus_start` (proposal) (P1) |
| "Note likho: client ko invoice bhejna hai" (Write a note: need to send the client an invoice) | `note_create` (proposal) (P1) |

### B5 Failure paths
| # | Situation | Flow |
|---|---|---|
| B5.1 | Empty or noise transcript | "Didn't catch that" |
| B5.2 | Several matching tasks | Proposal lists 3; "doosra" (second) selects |
| B5.3 | No answer within 30 s | Expired → "Cancelled" |
| B5.4 | "ruko" / "cancel" | Rules → Cancel |
| B5.5 | Hotkey while speaking | Barge-in |
| B5.6 | Hotkey while recording | "Busy: recording" |
| B5.7 | TTS fails | Reply shown as text with a note |
| B5.8 | Groq slow (> 4 s) or rate-limited | Gemini fallback |
| B5.9 | Cloud voice off | "Voice is off — enable in Settings"; nothing sent |

## C. Setup
1. First launch: voice `Disabled`; personal features work fully offline.
2. Settings → enable Cloud voice → disclosure → accept.
3. Enter Gnani, Gemini and Groq keys → keychain.
4. Mic permission prompt (WebView2) → Test: "Hello" → transcript + spoken reply.

## D. Offline behaviour
- Personal features: fully available.
- Meeting recording: continues locally; transcription runs when back online.
- Voice assistant: disabled with a clear message.
