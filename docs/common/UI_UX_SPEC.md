# UI / UX Specification — Pluto Voice

**Stack:** React + TypeScript + Tailwind + shadcn/ui.

**Owners:**
- Personal pages: Developer 1.
- Meetings, Voice and Settings pages, the overlay, and the audio modules: Developer 2.

All pages are built against `ui/src/lib/mockApi.ts` first (INTEGRATION_CONTRACT §3).

## 1. Navigation
**Left sidebar:**
- **Today** (default)
- **Tasks**
- **Reminders**
- **Notes** (P1)
- **Focus** (P1)
- **Projects** (P2)
- **Meetings**
- **Voice**
- **Settings**

**Always visible:**
- A voice bar at the bottom of the window, showing state, the last transcript line and a mic button.
- A tray icon with "Start recording" and "Quit".

**Global hotkey:** push-to-talk (default `Ctrl+Shift+Space`, configurable).

## 2. Screens

### S1 Today (D1)
- Tasks due today and overdue, the next 3 reminders, the active focus session, and open meeting commitments (P1).
- **Empty state:** "Nothing due today. Press Ctrl+Shift+Space and say what you need."

### S2 Tasks (D1)
- **Tabs:** Today / Upcoming / All / Done.
- **Row:** checkbox, title, due chip, priority dot, "from meeting" link if it came from a meeting.
- **Detail drawer:** edit all fields.
- **States:**

| State | Shown |
|---|---|
| Loading | Skeleton rows |
| Empty | Per-tab message |
| Error | Toast + Retry |
| Success | Inline update |

### S3 Reminders (D1)
- List sorted by time; overdue items highlighted.
- Create dialog with date and time pickers. A due reminder shows an OS notification and an in-app toast.

### S4 Notes (D1, P1)
List + editor (Markdown with preview), search, and a "Dictate" button (uses F7 dictation).

### S5 Focus (D1, P1)
Timer ring, task picker, start / pause / stop, and 7-day history.

### S6 Projects (D1, P2)
Project list → project page with its tasks and notes.

### S7 Meetings (D2)
**List:** title, date, status chip (`Recording`, `Transcribing`, `Needs speakers`, `Needs review`, `Done`, `Failed`). Buttons: **Record** and **Import audio**.

**Record dialog:** title, participants (chips), live-captions toggle, **consent checkbox (required)**.

**Live view:**
- Timer, level meter, pause, stop.
- Captions with a "Native / Romanised" toggle.
- P1: ticker column labelled "Live (draft)".
- **States:** connecting · captions offline (amber; "recording continues") · mic lost (red + Resume).

**Meeting detail tabs:**
- **Transcript:** time, speaker, text, language badge, search.
- **Speakers:** each label with 2 quotes → participant dropdown.
- **Review:**
  - Decisions / Action items (owner, due date, priority, "create task") / Questions / Risks.
  - Every item shows its quote; clicking jumps to the transcript.
  - Duplicate badge. Warnings banner (P1).
  - Apply with a count confirmation.
- **Recap:** language picker, Play, Export mp3 (P1).

**Job states:** progress · failed + Retry · "No speech found".

### S8 Voice (D2)
- **History:** each turn with transcript, language, decision source (rules / LLM), provider (Gemini / Groq), tool and outcome.
- **Commitments (P1):** table filtered by owner and status.
- **Comparison (P1):** clip → two columns with a diff.

### S9 Voice overlay / bar (D2)
- **States:** Idle · Listening (blue pulse) · Thinking (amber) · Speaking (green) · Recording (red dot) · Error (red, with an action).
- **Proposal card:** summary, Approve / Reject / Edit, 30 s countdown. Voice "haan / nahi" also works.
- Text is always shown together with the speech.

### S10 Settings → Voice & Privacy (D2)
| Control | Behaviour |
|---|---|
| Cloud voice | Off by default; disclosure modal |
| Keys | Gnani, Gemini, Groq: masked input → keychain; shows "Saved" afterwards |
| Languages | `Auto` or up to 4 |
| Caption style | Native (codemix) / Romanised (transliterate) |
| Voice | Timbre v3.0 voice list + preview |
| Hotkey | Recorder control |
| LLM models | Gemini and Groq model IDs |
| Glossary (P1) | Add / remove terms |
| Privacy log (P1) | Table + clear |
| Usage (P1) | Today's counters |
| Always-on (P2) | Toggle with a warning |

## 3. Feedback rules
1. Show the voice state within 100 ms of the hotkey.
2. Every spoken reply is also shown as text.
3. Every write shows a proposal first.
4. Every extracted item shows its source quote.
5. Every error message says what happened and the one next step.

## 4. Accessibility
- Full keyboard navigation and visible focus.
- Adjustable caption size.
- A "text only" option that mutes speech.
