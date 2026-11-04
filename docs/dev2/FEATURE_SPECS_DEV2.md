# Feature Specs — Developer 2

Specs for the features **Developer 2** owns. Tasks and acceptance criteria: [`../../PROJECT_ROADMAP.md`](../../PROJECT_ROADMAP.md) §4 (IDs in brackets). Shared contracts: [`../common/API_CONTRACTS.md`](../common/API_CONTRACTS.md), [`../common/DATA_SCHEMA.md`](../common/DATA_SCHEMA.md). Rules: [`../common/WORKING_AGREEMENT.md`](../common/WORKING_AGREEMENT.md).

**Shared Definition of Done:** same as Developer 1's spec (`FEATURE_SPECS_DEV1.md`).

---

## Base verification and plumbing [T-001, T-005, T-006]
- Run the base on Windows and record what works and what fails (`docs/results/base-verification.md`).
- **Contracts B:** meeting, LLM and decision types; traits `Llm`, `Decider`; `MockLlm` with fixtures.
- **UI plumbing:** `src/ipc/voiceClient.ts` and `meetingAi.ts` (typed wrappers), mock handlers composed into `createMock` with one edit to `src/ipc/mock.ts`, `voice://` / `meeting://` event listeners, the `voice` page entry (`Page`, `App.tsx`, Sidebar) and shared fixtures.

## LLM layer [T-009, T-104, T-105]
- **Day-1 check:** 5 prompts each against Gemini and Groq with JSON schemas; record free-tier limits and choose model IDs.
- **Router:** Intent → Groq then Gemini; Relevance, Extract, RecallAnswer and Recap → Gemini then Groq. Timeouts: intent 4 s, others 30 s. Fallback on timeout, 429, 5xx, or invalid JSON after one repair attempt; the response carries `fell_back`.
- **Secrets:** a `SecretStore` over the OS keychain for the Gnani, Gemini and Groq keys; commands to set and clear; non-secret settings (`voice.*`, `llm.*`) go through Pluto's settings service.
- **Errors:** `LlmUnavailable`, `LlmInvalidOutput`, `MissingApiKey`.

## Decisions [T-302]
- **Rules** (zero cost): yes / no / cancel / stop in Hindi, Tamil, Kannada and English; fixed phrases such as "focus N minute".
- **Otherwise** one intent call returns `{intent, tool, args}` validated against a JSON schema.
- **Dates:** the LLM returns the phrase plus a hint; Rust resolves the date against now and the user's timezone, so `AssistantDecision.args` carry resolved values (`due_date`, `remind_at`) plus the original phrase.

## Meeting pipeline
**Migration 012 [T-106]:** tables in DATA_SCHEMA §2, with `meeting_ai_store` functions; data export/import extended.

**Meetings UI [T-202]:** Import dialog (≤ 10 MB), status chips (`recording`, `transcribing`, `needs_speakers`, `needs_review`, `done`, `failed`), job progress, a Transcript tab (time, speaker, text, language) and a Speakers tab (each label with two sample quotes → choose a participant). Mapping updates every segment with that label and moves the meeting to `needs_review`.

**Extraction [T-203]:**
1. Segments go to the LLM in batches of up to 40, labelled `decision / action_item / question / risk / chitchat` (relevance and extraction in one pass for P0).
2. Rust validates: the source segment exists (otherwise the item is dropped), the owner is a participant or null, the due phrase resolves to a date, possible duplicates of open tasks are flagged.
3. Output is an `ExtractionResult`.

**Review and apply [T-204]:** edit, accept or reject each item; every item shows its quote and jumps to the transcript. Apply writes decisions and action items through core's `meetings::update`, stores provenance (`action_item_meta`, `decision_meta`, `meeting_insights`), calls `convert_action_item` for items marked "create task", then sets the item's own due date with `tasks::update` (the base uses the meeting's follow-up date). Re-running never duplicates rows.

**Recap [T-205]:** a 60–90 word recap in the chosen language from the applied items, spoken through `TextToSpeech`.

**Recorder, P0b [T-207]:** mic frames from `FrameSink` → WAV chunks of at most 4.5 minutes, flushed every 30 s; the consent checkbox is required (UI, command and database constraint); pause / stop; recovery of unfinished recordings on the next launch; stop queues the batch job.

## Voice UI and recall [T-304, T-305]
- **Voice bar and overlay:** states Idle, Listening (blue), Thinking (amber), Speaking (green), Recording (red), Error; always shows the text of what was heard and what is said.
- **Proposal card:** summary, Approve / Reject / Edit, a 30 s countdown; spoken "haan / nahi" also works.
- **History:** each turn with transcript, language, decision source (rules / LLM), provider (Gemini / Groq), tool and outcome.
- **Recall:** extract entities → FTS5 search with filters (top 8 segments) → a grounded answer with citations → spoken. A follow-up such as "Monday kar do" becomes a `task_update` proposal for the linked task. No match → "Not found in your meetings."

## Settings, demo build, measurements [T-401, T-402, T-405, T-406]
- **Voice & privacy section:** cloud-voice opt-in with a disclosure (audio → Gnani; transcript text → Gemini/Groq), keys (masked, shown as "Saved"), languages, caption style, voice preview, the push-to-talk shortcut row.
- **Demo build:** one setting hides non-product pages; tracking stays off.
- **Measurements:** run T-ML-01..04, T-NZ and T-LAT; record in `docs/results/` with commit and machine.

## P1 and P2
P1: live captions [T-501], warnings [T-502], commitments [T-503], comparison [T-504], glossary [T-505], privacy UI [T-507], audio tags [T-510]. P2: briefing, follow-up drafts, analytics, daily briefing [T-706..T-709].
