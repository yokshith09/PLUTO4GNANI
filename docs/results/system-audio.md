# System audio (the computer's sound) in recordings

**Status: built and compiled for Windows; not yet run on a real PC.** Test it before relying on it.

## What it does
- Tick "Include the computer's sound" in Record a meeting, or let Google Meet auto mode do it.
- The default speakers' output is captured (Windows WASAPI loopback, `crates/pluto-loopback`),
  turned into 16 kHz mono, and added to the microphone in time (`pluto_voice::mix`, 32 ms frames)
  before it reaches the recorder. The saved audio is one track with both sides of the call.
- Manual recording refuses to start if the sound cannot be captured (untick to use the mic alone).
  Auto mode falls back to the microphone alone and the pet says so.
- Nothing is sent anywhere until you stop, as before.

## To check on a Windows PC
1. Play a YouTube video, record 20 s with the box ticked, stop; the transcript should contain the video's words.
2. Speak while the video plays; both should be transcribed.
3. Headphones: the video should still be captured (loopback reads what is sent to the device).
4. Change the default output device mid-recording: capture keeps the old device (known limit).
5. Pause/Resume: the computer's sound keeps being mixed while paused only if the recorder accepts it;
   check there is no long silent gap.

## Known limits
- Only the default output device, at the time recording starts.
- Not available on macOS or Linux (the box returns a clear message).
- Per-app capture is not done; everything playing is recorded, including notification sounds.
