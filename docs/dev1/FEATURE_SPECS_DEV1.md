# Feature Specs — Developer 1

Specs for features owned by **Developer 1**. Shared contracts: [`../common/API_CONTRACTS.md`](../common/API_CONTRACTS.md), [`../common/DATA_SCHEMA.md`](../common/DATA_SCHEMA.md). Task IDs and status: [`../../PROJECT_ROADMAP.md`](../../PROJECT_ROADMAP.md). Definition of Done: PROJECT_ROADMAP §6.

### P1 Tasks (PP-01) — P0 — D1
- **Story:** I manage my tasks with due dates and priorities and see what's due today.
- **Behaviour:**
  - Create, edit, complete, uncomplete and delete (with confirmation).
  - Views: Today (due ≤ today, not done), Upcoming, All, Done.
  - Search by title.
- **Errors:** `Validation` (empty title, invalid date).
- **Edge cases:** completing sets `completed_at`; reopening clears it. Deleting a task linked from a meeting item sets that item's `task_id` to NULL.

### P2 Reminders (PP-02) — P0 — D1
- **Behaviour:**
  - Create with a date and time, optionally linked to a task.
  - A Rust scheduler sleeps until the next pending reminder, fires an OS notification plus `reminder://due`, and sets `fired_at`.
  - Mark done; delete.
- **Edge cases:**
  - Reminders missed while the app was closed fire once on startup, marked "missed".
  - A time in the past is rejected at creation.

### P3 Notes (PP-03) — P1 — D1
- Markdown editor with preview. FTS search. Optional project.
- Voice dictation uses F7 with `codemix` or `transliterate`.

### P4 Focus (PP-04) — P1 — D1
- Start (minutes, optional task), pause, resume, stop. `focus://tick` every second.
- On completion: notification + spoken "Focus done".
- History list (last 7 days).
- **Edge case:** only one active session; starting a new one asks to stop the current one.

### P5 Projects (PP-05) — P2 — D1
- Create, rename, archive. Assign tasks and notes. A project page lists both.

---

## Voice platform

### F1 Push-to-talk and runtime (VA-01, VA-07) — P0 — D1 (overlay UI: D2)
- **Behaviour:**
  1. Hotkey down → `voice_ptt_start` → the UI starts capture (`getUserMedia`, worklet) and streams frames to `audio_push` → Prisma Realtime (`indic-auto`, `codemix`, `keyterms` = glossary + open task titles + participants).
  2. Hotkey up → `commit` → wait up to 3 s for the final transcript → `Thinking`.
- **Errors:** `MicPermissionDenied`, `MicUnavailable`, `MissingApiKey`, `CloudVoiceDisabled`, `NetworkOffline`, `RateLimited`, `InsufficientCredit`, `Busy`.
- **Edge cases:**
  - Held for under 300 ms → ignored.
  - Held for over 60 s → auto-commit.
  - Pressed while `Speaking` → barge-in.
  - Pressed while `Recording` → `Busy`.
- **DoD:** STT latency stored in `voice_interactions`.

### F2 Speech output (VA-07, VA-08) — P0 (tags P1) — D1 (player: D2)
- `voice_speak` → Timbre WS (`timbre-v3.0`, `raw`, 24 kHz) → playback `Channel` → web audio queue. Language = detected language.
- Text over 500 characters is split at sentence boundaries. Audio tags only from the allow-list.
- **Errors:** TTS failure → the reply is shown as text with a "voice unavailable" note.

### F4 Tools and approval gate (VA-03, VA-06; VA-04 P1; VA-05 P2) — P0 — D1
- **Behaviour:**
  - Reads execute and are spoken (lists: at most 5 items, then "and N more").
  - Writes → `Proposal` (stored, shown, spoken) → `AwaitingApproval`, 30 s → approve by voice (rules) or click → core service → spoken result.
- **Context:** the last 6 turns, so "usse Friday kar do" (make that one Friday) resolves "that one".
- **Edge cases:**
  - Several matching tasks → the proposal lists up to 3; "pehla / doosra" (first / second) selects.
  - Unresolvable date → choose on screen.
- **DoD:** test T-AS-05 proves no write runs without an approved proposal.

### F5 Settings and privacy (PL-01 to PL-05) — P0 / P1 — D1 (backend), D2 (UI)
- Opt-in disclosure. Keys → keychain (Gnani, Gemini, Groq). Languages, caption style, voice (with preview), hotkey.
- P1: glossary, privacy log, usage counters.

---

## Meeting Assistant

### M1 Recording (MA-01, MA-05) — P0 — D1 (UI: D2)
- **Behaviour:**
  1. `meeting_rec_start` requires `consent`.
  2. The UI streams frames.
  3. Rust writes 16 kHz mono WAV chunks of ≤ 4.5 min, flushed every 30 s.
  4. With `live_captions`, frames also go to Realtime and produce `meeting://caption`. The connection rotates at 14 min.
  5. Stop → batch job queued.
- **Errors:** `ConsentRequired`, `MicUnavailable`, `NetworkOffline` (captions only; recording continues).
- **Edge cases:** recording under 5 s → discard after confirmation; crash → recovery on next launch.

### M2 Import and batch transcription (MA-02, MA-03) — P0 — D1
- **Behaviour:**
  - Validate the extension and size (≤ 10 MB).
  - Create the job with: participant count as `num_speakers` (1–8), the first 4 participants as `speaker_names`, and glossary + participants as `keyterms`.
  - Start → poll → fetch the transcript files → insert segments with chunk offsets → meeting status `NEEDS_SPEAKERS`.
- **Errors:** `FileTooLarge`, `UnsupportedAudio`, `BatchJobFailed`, `RateLimited`, `InsufficientCredit`.
- **Edge cases:** `PARTIAL_FAILURE` → keep the completed chunks and show gap markers; no speech → "No speech found".

