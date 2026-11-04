// The two binary IPC paths of the hybrid audio design (API_CONTRACTS §4.2, §5). Owner: D1 (T-012).
//
// They use Tauri's raw binary forms, which the JSON client in `src/ipc` doesn't cover:
// - `audio_push`: the frame is the raw request body; the session id travels in the
//   `x-session-id` header.
// - the `playback` Channel: Rust sends PCM16 24 kHz chunks; they arrive as an ArrayBuffer (raw
//   body) or as a number array (JSON-serialised `Vec<u8>`), so both are accepted.
// Outside the desktop app (browser preview, tests) pushes are dropped: there is no Rust side and
// no network call is ever made from the UI (NF-01).

import { Channel, invoke, isTauri } from "@tauri-apps/api/core";
import type { PushFn } from "./sender";

export const AUDIO_PUSH = "audio_push";
export const SESSION_HEADER = "x-session-id";

export const pushFrame: PushFn = async (sessionId, frame) => {
  if (!isTauri()) return;
  await invoke(AUDIO_PUSH, new Uint8Array(frame), { headers: { [SESSION_HEADER]: sessionId } });
};

export type PlaybackMessage = ArrayBuffer | Uint8Array | number[];

export function toBytes(message: PlaybackMessage): Uint8Array {
  if (message instanceof Uint8Array) return message;
  if (message instanceof ArrayBuffer) return new Uint8Array(message);
  return Uint8Array.from(message);
}

/** A Channel to pass as `playback` to `voice_speak` / `meeting_recap`; each chunk goes to `onChunk`. */
export function playbackChannel(onChunk: (pcm: Uint8Array) => void): Channel<PlaybackMessage> {
  const channel = new Channel<PlaybackMessage>();
  channel.onmessage = (message) => onChunk(toBytes(message));
  return channel;
}
