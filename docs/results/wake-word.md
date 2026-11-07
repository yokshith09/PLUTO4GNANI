# T-705 Always-on listening: wake-word measurements

**Result:** Rustpotter, taught with the user's own voice, caught every "Pluto" in D1's test session with no false triggers, at about 2% of one CPU core. It doesn't recognise other people's voices well, so always-on is set up per user: Settings → Voice & privacy → **Teach Pluto your voice**.

- Machine: D1's Windows 11 laptop (12 logical cores).
- Date: 7 Nov 2026.
- Engine: `rustpotter` 3.0.2 in reference mode, threshold 0.5. `half` is pinned to 2.4.1, because rustpotter's `candle` 0.2 doesn't build with newer releases.
- The test programs are not committed. Test audio:
  - D1's own recordings: 8 × "Pluto", plus a 40 s session with 4 "Pluto, …" commands and normal talk in between;
  - Gnani Timbre voices: 13 voices saying "Pluto", and 72 sound-alike words;
  - real speech from a push-to-talk recording.

## Why not the alternatives
| Option | Finding |
|---|---|
| Porcupine (Picovoice) | Picovoice ended its free tier on 30 Jun 2026; only a one-time enterprise trial is left. Its access key is checked online, which would break "Cloud voice off → zero outbound requests" (T-PR-04) |
| Rustpotter, reference built from Timbre voices | Detected only a voice that was in the reference, in quiet. New voices: 0 detections at any threshold. Thresholds of 0.4 and lower fired on ordinary speech |
| Rustpotter trained model, 13 Timbre voices (3 held out), 564 augmented clips | Learned the training voices perfectly but nothing transferable. On the held-out voices: 61–64% test accuracy, below the 73% from always saying "not Pluto"; 0/5 "Pluto" caught |
| openWakeWord | Works for any speaker by design (a pre-trained speech embedding). It adds an ONNX runtime (~15 MB) and offline Python training: a candidate after submission |

## The user's own voice (what ships)
The reference was built from 6 of D1's 8 recordings; 2 had no clear word, and teaching in the app now asks again for those.

| Threshold | Commands caught (4) | False triggers in D1's normal talk | Sound-alike words (72) | Other people saying "Pluto" (39) |
|---|---|---|---|---|
| 0.45 | 4 | 0 | 3 | 28 |
| **0.5** | **4** | **0** | **0** | **8** |
| 0.55 | 3 | 0 | 0 | 2 |
| 0.6 | 0 | 0 | 0 | 0 |

- **Leave one out:** a reference built from 5 of the clips found the sixth every time (6/6, scores 0.58–0.63).
- **Delay:** detection lands about 1 s after "Pluto" starts. The turn takes the last 0.5 s before the detection, so the first words of the command are kept.
- **CPU:**
  - quiet room: 0.1–0.35% of one core;
  - continuous talk: 2–8% of one core;
  - with the 6-clip reference: about 1.8% of one core, under 0.2% of the whole CPU.
- **Memory:** a few MB.

## How it behaves in the app
- The detector works wherever push-to-talk does: while idle, while a proposal waits for its answer ("Pluto, haan"), after an error, and over a reply (which it stops). It rests while Pluto is thinking, running a change or recording a meeting, and while Cloud voice is off.
- **Before "Pluto", nothing is sent.** After it, the command goes to Gnani exactly like a push-to-talk turn.
- The command ends at a 0.8 s pause (was 1.2 s; shortened for speed), or at 4 s if nothing follows "Pluto", or at 15 s at most.
- While always-on is on, its microphone also serves push-to-talk, so only one capture is open.
- The wake word ("Pluto, …", "Hey Pluto", "प्लूटो") is removed before the command is decided.
- Tests:
  - `pluto-voice` `wake::tests`: clip checks, store, the real detector on synthetic fixtures, and the listening loop with a fake runtime;
  - `assistant::tests::a_wake_word_command_is_handled_without_the_wake_word`;
  - UI: `src/features/voice/WakeSetup.test.tsx`.
