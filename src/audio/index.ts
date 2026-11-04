// Hybrid audio path, UI side (ARCHITECTURE §4). Owner: D1 (T-012).
//
//   const sender = new FrameSender(sessionId, pushFrame, { onError });
//   const mic = await startCapture({ onFrame: (f) => sender.send(f), onEnded });
//   …  await mic.stop(); await sender.close();
//
//   const player = new PcmPlayer({ onIdle });
//   const playback = playbackChannel((pcm) => player.enqueue(pcm));   // pass to voice_speak
//   …  player.stop();                                                  // barge-in
export { CAPTURE_SAMPLE_RATE, FRAME_BYTES, PLAYBACK_SAMPLE_RATE } from "./pcm";
export { CaptureError, startCapture, type CaptureErrorCode, type CaptureHandle, type CaptureOptions } from "./capture";
export { BacklogError, FrameSender, type FrameSenderOptions, type PushFn } from "./sender";
export { PcmPlayer, type PlayerOptions } from "./player";
export { playbackChannel, pushFrame } from "./ipc";
