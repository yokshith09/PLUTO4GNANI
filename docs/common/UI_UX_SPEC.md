# UI / UX Specification — Pluto Voice

**Base:** the ported Pluto UI (React + TypeScript + Tailwind + Radix, `src/`). Pages switch by state, not by a router (`Page` in `src/features/shell/Sidebar.tsx`). **This file specifies only what is new or changed.**

**Owners:** Developer 2 for all Meetings, Voice, Settings and shell changes; Developer 1 for `src/audio`.
All new screens are built against the in-memory mock (`src/ipc/mock.ts` plus the new mock handlers) first.

## 1. Navigation
**Existing sidebar pages:** Today, Tasks, Time, Notes, Projects, Study, Diary, Meetings, Archive, Bin, Character, MCP, Settings.

| Change | Detail |
|---|---|
| New page **Voice** | History of assistant turns, (P1) commitments and comparison. Added to `Page`, `PAGES` in `App.tsx` and the Sidebar once, at T-006 |
| **Demo build hides non-product pages** | Time, Study, Diary, Character, MCP, Archive are hidden by one setting (T-402). Their code and tests stay; app tracking stays off |
| Meetings page extended | Import (P0), Record (P0b), status chips, Transcript / Speakers / Review / Recap tabs |
| Settings gets a **Voice & privacy** section | Between "Keyboard shortcuts" and "Advanced"; adds a `voice_ptt` shortcut row |
| **Voice bar** | A slim bar at the bottom of the main window showing voice state, the last transcript line and a mic button |
| Push-to-talk hotkey | New `VoicePtt` global shortcut (default proposal `Ctrl+Shift+Space`, configurable; must not clash with the existing `Ctrl+Shift+L` / `Ctrl+Shift+N`) |

## 2. Screens that already exist (no UI work planned)
Today, Tasks, Notes, Projects, Focus (on Today), Reminders (in Tasks and Today), Bin. Voice-created tasks and reminders appear in them through the base's normal events, without a reload.

## 3. New and changed screens

### S1 Meetings (extends the existing page)
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

### S2 Voice page (new)
- **History:** each turn with transcript, language, decision source (rules / LLM), provider (Gemini / Groq), tool and outcome.
- **Commitments (P1):** table filtered by owner and status.
- **Comparison (P1):** clip → two columns with a diff.

### S3 Voice bar and overlay (new)
- **States:** Idle · Listening (blue pulse) · Thinking (amber) · Speaking (green) · Recording (red dot) · Error (red, with an action).
- **Proposal card:** summary, Approve / Reject / Edit, 30 s countdown. Voice "haan / nahi" also works.
- Text is always shown together with the speech.

### S4 Settings → Voice & privacy (new section)
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

## 4. Feedback rules
1. Show the voice state within 100 ms of the hotkey.
2. Every spoken reply is also shown as text.
3. Every write shows a proposal first.
4. Every extracted item shows its source quote.
5. Every error message says what happened and the one next step.

## 5. Accessibility
- Full keyboard navigation and visible focus.
- Adjustable caption size.
- A "text only" option that mutes speech.
