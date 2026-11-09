# Submission draft — Pluto Voice (T-604)

_Draft for the owner to edit. Fill the bracketed parts after rehearsal. Do not paste keys._

## What it is
Pluto Voice is a Windows desktop assistant for meetings and for hands-free work. It records or imports
a meeting, turns it into a speaker-labelled transcript that handles Hindi–English mixing, writes the
decisions, action items and a summary into the Meetings page, and lets you ask later: "what did Ravi
promise last week?" You can also talk to it: push-to-talk, or say "Pluto" for always-on listening.
Anything that changes your data (a task, a reminder) is read back and needs your yes first.

## Why Gnani
- **Prisma v3.0** does the transcription: batch for meetings (speaker separation, code-mixed text,
  participant names as hints), realtime for live captions.
- **Timbre v3.0** speaks the replies and the spoken recap.
- Names, numbers and dates are where it matters. Results: see `docs/results/` [T-405 numbers go here].

## Models used
| Job | Model |
|---|---|
| Speech to text | Gnani Prisma v3.0 |
| Text to speech | Gnani Timbre v3.0 |
| Meeting extraction and recall | Gemini (Groq as fallback) |
| Voice commands | Groq (Gemini as fallback) |
No model runs on the device. Nothing is sent while Cloud voice is off; the privacy log shows every call.

## What a user sees
1. Open Google Meet: the pet asks "Record this meeting?" (optional auto mode). It records the
   microphone and, on Windows, the computer's sound, and stops when the call ends.
2. Minutes later the meeting has decisions, action items and a summary, no review step.
3. "Pluto, add a task to send the budget report to Ravi" → read back → "haan kar do" → saved.

## Safety and privacy
- Recording needs a consent tick. Keys live in the OS keychain only.
- Cloud voice is off until the person turns it on; delete-my-data wipes saved replies and the log.

## Contributions
- Developer 2 (Yokshith): meeting intelligence (extraction, recap, recall, Google Meet auto mode),
  recording and system-audio capture, the Meetings UI, the pet, settings, and the app's polish.
- Developer 1 (Akhil): Gnani clients (Prisma, Timbre), voice runtime, wake word, assistant tools and
  approval, safety checks. [Confirm wording with Akhil.]

## Links
- Demo video: [T-603]  · Posts: [T-603]  · Repo: [add if public]
