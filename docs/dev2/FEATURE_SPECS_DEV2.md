# Feature Specs — Developer 2

Specs for features owned by **Developer 2**. Shared contracts: [`../common/API_CONTRACTS.md`](../common/API_CONTRACTS.md), [`../common/DATA_SCHEMA.md`](../common/DATA_SCHEMA.md). Task IDs and status: [`../../PROJECT_ROADMAP.md`](../../PROJECT_ROADMAP.md). Definition of Done: PROJECT_ROADMAP §6.

### A1 Audio capture and playback in the UI (VA-01, MA-01) — P0 — Developer 2
- **Capture:**
  1. `getUserMedia` with echo cancellation, noise suppression, mono.
  2. An `AudioWorklet` downsamples from the device rate to 16 kHz Int16.
  3. Audio is packed into 1,024-byte frames.
  4. Frames go to `audio_push` (raw body, header `x-session-id`) while a session is active.
- **Playback:** receive PCM16 24 kHz bytes from the `playback` Channel → `AudioBuffer`s → a gapless scheduled queue. `stop()` clears the queue (barge-in).
- **Errors:**
  - permission denied → `MicPermissionDenied` banner;
  - no device → `MicUnavailable`;
  - device change → restart capture.
- **Tests:**
  - Unit: the downsampling function (sine-wave input gives the expected length and amplitude).
  - Joint: T-IPC-01 with Developer 1.
- **Edge cases:** device rates of 44.1 or 48 kHz; tab or window hidden (capture keeps running); frame boundary carry-over.

### F3 Decisions and LLM router — P0 — D2
- **Rules table:** haan / ha / yes / ok / theek hai → `Approval`; nahi / no / mat karo → `Reject`; ruko / cancel / stop → `Cancel`; "focus N minute" → `focus_start`.
- **Otherwise:** an `Intent` LLM task (Groq → Gemini) returns `{intent, tool, args}` validated against a JSON schema.
- **Fallback:** a timeout of 4 s, a 429/5xx, or invalid JSON after one repair attempt sends the request to the other provider.
- **Errors:** `LlmUnavailable`, `LlmInvalidOutput` → spoken "Sorry, I couldn't process that."
- **DoD:** rule tests in hi/ta/kn/en; router fallback tested with a mocked 429.

### M3 Speaker mapping (MA-04) — P0 — D2
- Each speaker label is shown with 2 sample quotes; pick a participant or "Unknown". Saved with `meeting_speakers_set`. Status → `NEEDS_REVIEW`.

### M4 Extraction and review (MA-06, MA-07, MA-10) — P0 (warnings P1) — D2
- **Behaviour:**
  1. **Relevance:** Gemini, batches of ≤ 40 segments.
  2. **Extraction:** Gemini; Groq as fallback.
  3. **Validation in Rust:**
     - the segment exists (otherwise the item is dropped);
     - the owner is a participant or null;
     - the due date is resolved from the phrase;
     - duplicates of open tasks are detected via FTS.
  4. Review UI.
  5. `meeting_extraction_apply` writes `meeting_items` and creates tasks for items with `create_task`. Status → `DONE`.
- **Errors:** `LlmUnavailable` (retry); `LlmInvalidOutput`.
- **Edge case:** re-running on an applied meeting shows a diff and never duplicates items.

### M5 Voice recall (MA-08, VA-02) — P0 — D2
- `MeetingQuery` → extract entities → FTS top-8 → grounded answer with citations (Gemini → Groq) → spoken.
- A follow-up change → proposal for `task_update` on the linked task.
- No results → "Not found in your meetings."

### M6 Recap (MA-09; MA-13 P1) — P0 — D2
- 60–90 word recap in the chosen language, generated from the applied items, then spoken. P1: export to mp3 via Timbre REST.

### M7 P1 meeting features — D2
| ID | Feature | Behaviour |
|---|---|---|
| MA-10 | Quality warnings | Banner + spoken warning |
| MA-11 | Live ticker | Caption windows of 5 segments → relevance → `meeting://ticker`, labelled "draft" |
| MA-12 | Commitments | Status computed from items + tasks (Done / Overdue / NoTask / Open); filter by owner and status |
| MA-14 | Comparison | Clip ≤ 30 s, Prisma REST vs a baseline engine, word diff. **The baseline must be named and reproducible** |

