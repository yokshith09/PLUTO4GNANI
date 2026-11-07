# T-403 Safety checks (T-AS-05, T-PR-01..04)

**Result: all five pass, as automated tests that run in CI on every pull request.**

| ID | Check | Test | Result |
|---|---|---|---|
| T-AS-05 | Every write tool is unreachable without an approved proposal | `pluto-voice` `assistant::tests::t_as_05_no_write_tool_changes_anything_without_an_approved_proposal` | Pass |
| T-PR-01 | Keys absent from logs, SQLite and exports | `pluto-voice/tests/safety.rs` `t_pr_01_the_key_never_reaches_logs_errors_sqlite_or_the_export` | Pass |
| T-PR-02 | Every outbound call logged | `t_pr_02_every_request_that_leaves_is_logged`; Realtime and Timbre in `pluto-gnani` (`realtime/tests.rs`, `tts/tests.rs`); LLMs: `pluto-intel/tests/llm_router.rs` `keys_travel_only_in_headers_and_every_call_is_logged_without_content` | Pass |
| T-PR-03 | Recording impossible without consent (UI + command + DB) | UI: `RecordDialog.test.tsx` "cannot start without consent, here or in the shell". Service and DB: `pluto-intel/tests/recorder.rs` `no_consent_means_no_meeting_and_no_recording`. DB check: `pluto-core` `meeting_ai_store` | Pass |
| T-PR-04 | Cloud voice off → zero outbound requests | `t_pr_04_cloud_voice_off_sends_nothing_on_any_path`; `t_pr_04_turning_cloud_voice_off_stops_calls_already_waiting_to_retry`; `pluto-gnani` `turning_cloud_voice_off_mid_session_stops_the_audio_and_ends_the_session` | Pass |

## What each test proves
- **T-AS-05:**
  - The test fails if a write tool is added to `tools::WRITE` without being covered here.
  - Each write tool is spoken in turn, then rejected. The whole user database (the data export, minus the assistant's own log) is unchanged after every step.
  - A write tool sent through `run_read` is refused. An answer to a proposal that isn't pending does nothing.
  - Control: one approved proposal does change the data.
- **T-PR-01:**
  - The key does reach Gnani, in the `x-api-key-id` header.
  - It never appears in any of these:
    - tracing output at every level;
    - the messages or `Debug` of the errors from failed REST, Batch, Realtime and Timbre calls;
    - privacy-log lines;
    - the data export;
    - the bytes of the SQLite files.
- **T-PR-02:** the mock server's request count equals the number of privacy-log lines. That includes retried attempts, error responses and the Batch transcript download. Every line has a purpose.
- **T-PR-04:**
  - With the switch off, all nine sending paths return `CloudVoiceDisabled`: REST, Batch create/status/files/download, Realtime, Timbre stream and file, and the LLM.
  - The REST mock, the WebSocket host and the LLM receive nothing, and nothing is logged.
  - Turning it off also stops a call waiting to retry, and Realtime audio mid-session, with no reconnect.
  - Control: the same clients do send once the switch is on.
  - A mutation check removed the Realtime gate, and the suite failed.

## Fixed while writing T-PR-04
Before this, Cloud voice off stopped only push-to-talk and spoken replies. **Meeting transcription (Gnani Batch) and the meeting LLM calls (extraction, recap, recall) still went out.**

Now one `CloudSwitch` (`pluto-gnani::http`) follows Settings → Cloud voice. Every client checks it immediately before sending:
- `GnaniHttp`, which covers REST and Batch, and the Batch download;
- `RealtimeClient`, on connect, each reconnect and each audio frame;
- `TimbreClient`;
- the LLMs, through `pluto_voice::privacy::CloudGated`, around the router for both the assistant and meetings.

These six are the only places in the app that open a network connection.

- Date: 7 Nov 2026, D1's Windows 11 laptop. CI runs the same tests on the Windows runner.
