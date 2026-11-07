# Test data

Synthetic only. No real people, meetings or recordings.

| Path | Content |
|---|---|
| `audio-scripts/` | What to say for AS-1 (Hinglish launch sync), AS-2 (Tanglish sprint review), AS-3 (Kannada-English office move) and AS-4 (50 commands) |
| `ground-truth/` | Expected decisions, action items (owner, due date, quote), questions, risks; AS-4 expected tools and arguments |
| `audio/` | Recorded WAVs (to be added; record per script, 16 kHz mono or higher) |
| `wake/` | Two “Pluto” clips in the synthetic Timbre voice Shivani (16 kHz, trimmed to the word) for the wake-word tests (T-705) |
| `fixtures/` | Shared JSON fixtures used by the UI mock and the Rust contract tests |

`crates/pluto-intel/tests/ground_truth.rs` checks the scripts and ground truth agree (every quote is in its script). Noise variants (AS-5) are made from the clean recordings with `ffmpeg` or `sox`; commands go in `docs/results/README.md`.
