# Gnani live checks (T-GN-01 / T-010: T-101, T-102, T-201)

Run on 2026-11-05 by D1 against `api.vachana.ai` with the team key (read from the git-ignored `.env`).
Test audio: an 11.1 s Hinglish clip spoken by Timbre (`Shivani`, hi-IN):
"Chalo meeting shuru karte hain. Rahul, login fix Friday tak ho jayega na? Haan, main Friday tak kar
dunga. Aur Asha DBMS assignment Monday ko submit karegi." For Realtime it was resampled to 16 kHz mono.

| Check | Command | Result |
|---|---|---|
| Timbre stream (T-102) | `cargo run -p pluto-gnani --example tts_smoke -- "Theek hai, ho gaya." hi-IN` | First audio **0.53–0.63 s** warm (8.2 s on the very first, cold connection); 1.3–2.1 s of PCM 24 kHz in ≈ 1 s. Audible check: play `target/tts_smoke.wav` |
| Batch (T-201) | `cargo run -p pluto-gnani --example batch_smoke -- meeting.wav Priya,Rahul,Asha` | Job created → started → `COMPLETED` on the first poll (≈ 13 s end to end, 0.5 MB). One segment 00:00–00:11, `speaker_id` **1** (a number, even with `speaker_names` sent), `hi-IN`, codemix text with English terms kept in Latin script |
| Prisma REST (T-010) | `cargo run -p pluto-gnani --example rest_smoke -- meeting16k.wav` | `success: true` in **0.76 s** (`processing_time` 0.24 s), `hi-IN`, same codemix transcript |
| Realtime (T-101) | `cargo run -p pluto-gnani --example rt_smoke -- meeting16k.wav` | Connected in 0.45 s; 3 final transcripts, the last 0.6–0.7 s after the audio ended; `detected_language_code` hi-IN |

## Findings
- **Realtime `latency` is in milliseconds** (raw values 74, 97, 164). The client's "under 30 means seconds"
  guess was removed; the value is now taken as milliseconds.
- **Batch `speaker_id`** is numeric with diarization and `speaker_names`, so speaker mapping (T-202) works on
  labels as designed. Whether Gnani orders labels by `speaker_names` needs a multi-voice clip (AS-1).
- **Polling:** Gnani asks for status calls no more often than every 10 s; the client polls at 10 s backing off
  to 15 s (API_CONTRACTS said 5 s).
- **Cold start:** the first Timbre connection of a session was slow (8 s). Worth a warm-up connection when the
  app opens with cloud voice on (T-301).

## Credits used (T-010)
Gnani exposes no usage API, so this is what the checks sent (check the balance on the Gnani dashboard):

| Service | Sent |
|---|---|
| Timbre | 5 requests, ≈ 330 characters (≈ 18 s of speech) |
| Prisma Batch | 1 job, 11.1 s of audio |
| Prisma Realtime | 2 sessions, 22.2 s of audio |
| Prisma REST | 1 clip, 11.1 s of audio |

Total speech-to-text ≈ 44 s. All three T-010 acceptance checks (REST clip, Realtime clip, TTS) succeed with the team key.
