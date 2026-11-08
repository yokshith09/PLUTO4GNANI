# T-011 Mic check (T-IPC-01)

**Result: pass.** WebView2 capture works on D1's laptop, so the `cpal` fallback isn't needed.

| Check | How | Result |
|---|---|---|
| Capture → Rust → WAV | Push-to-talk in `pnpm tauri dev`. Development builds save each press to `%APPDATA%\app.plutovoice.desktop\voice-check\last-ptt.wav` | 16 kHz, 16-bit, mono, 5.38 s; peak 16100 and RMS 1402 (clear speech, no clipping) |
| Capture → Gnani | The same presses streamed to Prisma Realtime | Words transcribed; the assistant made proposals from them |
| Playback | The assistant's Timbre reply played through the UI audio path | Heard by D1 ("adding this task", then the approve/reject proposal) |

- Machine: D1's Windows 11 laptop, built-in microphone.
- Date: 6–7 Nov 2026, on the `D1` branch around `705b66c`.
- Not measured: the full 10 s capture. The longest saved press was 5.4 s, and 60 s presses auto-commit by design (T-301).
