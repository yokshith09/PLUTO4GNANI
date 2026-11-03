# Feature Specs — Developer 1

Specs for the features **Developer 1** owns. Tasks and acceptance criteria: [`../../PROJECT_ROADMAP.md`](../../PROJECT_ROADMAP.md) §4 (IDs in brackets). Shared contracts: [`../common/API_CONTRACTS.md`](../common/API_CONTRACTS.md), [`../common/DATA_SCHEMA.md`](../common/DATA_SCHEMA.md). Rules: [`../common/WORKING_AGREEMENT.md`](../common/WORKING_AGREEMENT.md).

**Shared Definition of Done:** tests added and passing; `cargo clippy -D warnings`, `pnpm lint`, `pnpm typecheck` clean; types regenerated with `pnpm gen:types` if changed; works on mocks and with the real service; all listed error states implemented; its acceptance test passes on the Windows demo machine.

---

## Skeleton and contracts [T-003, T-004, T-007]
- **Skeleton (merge within the first hour of Day 1):** add `pluto-contracts`, `pluto-gnani`, `pluto-voice`, `pluto-intel` to the workspace; one placeholder file per owner so nobody edits `lib.rs` later; extend `gen:types` to the new crates; register migrations 012 and 013 in `migrate.rs` as empty files; add the `mod` lines for `meeting_ai_store` and `voice_store` to `pluto-core`.
- **Contracts A:** voice, speech, proposal and tool types, `VoiceError`, traits `SpeechToText`, `TextToSpeech`, `ToolExecutor`, `FrameSink`, each with a mock.
- **Shell stubs:** every new command returns fixture data; all registered in `generate_handler!` in one edit.

## Audio I/O [T-012, T-013, T-011]
- **UI side (`src/audio`):** `getUserMedia` with echo cancellation and noise suppression, mono. An `AudioWorklet` converts the device rate to 16 kHz Int16 and packs **1,024-byte frames**. A sender calls `audio_push` (raw body, header `x-session-id`) while a session is active. A player turns PCM16 24 kHz chunks from the `playback` Channel into a gapless scheduled queue; `stop()` clears it (barge-in).
- **Rust side:** `audio_push` hands frames to every registered `FrameSink` (the WAV writer, the Realtime sender, the recorder). The playback Channel carries Timbre audio back.
- **Errors:** `MicPermissionDenied`, `MicUnavailable`.
- **Edge cases:** device rates of 44.1 and 48 kHz; frame boundary carry-over; window hidden (capture continues).
- **Fallback:** if WebView2 capture or binary IPC fails the Day-1 check, capture moves to Rust with `cpal`; contracts do not change.

## Gnani clients [T-101, T-102, T-103, T-201]
- **`http` (T-103):** one request in flight per REST family; retry with 0.5 / 1 / 2 s backoff on connection errors and 429/500/503/504, honouring `Retry-After`; no retry on other 4xx; 402 → `InsufficientCredit` and voice is disabled; every call writes a privacy-log row.
- **`realtime` (T-101):** headers per API_CONTRACTS §2.1; `commit` on hotkey release; rotate the connection at 14 minutes; reconnect with backoff, buffering up to 10 s of audio; one `TranscriptSegment` per final transcript (there are no partials).
- **`tts` (T-102):** WebSocket synthesis, base64 chunks decoded to PCM16 24 kHz and pushed to the playback Channel; REST for files; language follows the detected language; text over 500 characters is split at sentence boundaries; audio tags only from the allow-list.
- **`batch` (T-201):** create / start / poll (5 s backing off to 15 s) / files / transcript; segments stored with chunk offsets; `meeting_import` accepts Gnani's formats up to 10 MB; `PARTIAL_FAILURE` keeps finished chunks and shows gap markers; emits `meeting://job`.

## Voice runtime [T-301]
- **Push-to-talk:** a new `VoicePtt` action in `shortcuts.rs`. The base handles only key press, so key **release** is added. Hotkey down → Listening (shown within 100 ms) → frames to Realtime; release → `commit` → wait up to 3 s for the final transcript → Thinking.
- **States:** Disabled, Idle, Listening, Thinking, AwaitingApproval, Executing, Speaking, Recording, Error (ARCHITECTURE §5). Pressed while Speaking stops playback (barge-in). Pressed while Recording → `Busy`.
- **Edge cases:** a press under 300 ms is ignored; a press over 60 s auto-commits.

## Assistant: tools and approval [T-303]
- **Reads** run directly and are spoken (lists: at most 5 items, then "and N more"). **Writes** create a `Proposal` (stored, shown, spoken) and wait up to 30 s; approval by rules ("haan", "yes") or a click; then the core function runs and the result is spoken.
- **Context:** the last 6 turns, so "usse Friday kar do" refers to the previous task.
- **Ambiguity:** several matching tasks → the proposal lists up to 3; "pehla / doosra" selects.
- **Safety:** no write path bypasses `voice_proposal_respond` (T-AS-05).

## Safety, build and submission [T-403, T-404, T-601, T-604]
- Automate T-AS-05 and T-PR-01..04.
- `pnpm tauri build` for Windows, demo-machine setup, a rollback installer.
- Tag `demo-v1`; fill in the submission form with the models used and each developer's contribution.

## P1 and P2 (after C4; never in `demo-v1` unless done and tested)
Privacy log backend [T-506], notes and focus voice tools [T-508], mp3 export [T-509], failure suite [T-511]. P2: projects voice tools, voice cloning, folder watch, WASAPI loopback, always-on wake word [T-701..T-705].
