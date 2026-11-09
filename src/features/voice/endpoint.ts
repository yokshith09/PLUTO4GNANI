// Ends a push-to-talk turn by itself after the person stops talking, so nobody has to press Send.
// Frames are 32 ms of 16 kHz PCM16 (1,024 bytes). Speech is anything clearly above the room's
// background level, which is followed slowly (the same idea as the always-on listener in Rust).

const FRAME_MS = 32;
const frames = (ms: number) => Math.ceil(ms / FRAME_MS);

export interface EndpointPolicy {
  /** A pause this long after speech ends the command. */
  endSilenceMs: number;
  /** No speech at all for this long: give up and send what there is. */
  noSpeechMs: number;
  /** The longest command. */
  maxMs: number;
}

export const DEFAULT_ENDPOINT: EndpointPolicy = { endSilenceMs: 1200, noSpeechMs: 6000, maxMs: 30_000 };

function rms(buf: ArrayBuffer): number {
  const s = new Int16Array(buf.slice(0, buf.byteLength - (buf.byteLength % 2)));
  if (s.length === 0) return 0;
  let sum = 0;
  for (const v of s) sum += v * v;
  return Math.sqrt(sum / s.length);
}

export interface Endpointer {
  /** Feed one captured frame; "end" once the command is over. */
  feed: (frame: ArrayBuffer) => "continue" | "end";
}

export function createEndpointer(policy: EndpointPolicy = DEFAULT_ENDPOINT): Endpointer {
  let floor = 150;
  let heardSpeech = false;
  let quiet = 0;
  let count = 0;
  return {
    feed(frame) {
      count += 1;
      const level = rms(frame);
      const speech = level >= Math.max(floor * 2.5, 350);
      if (speech) {
        heardSpeech = true;
        quiet = 0;
      } else {
        floor = floor * 0.95 + level * 0.05;
        if (heardSpeech) quiet += 1;
      }
      if (heardSpeech && quiet >= frames(policy.endSilenceMs)) return "end";
      if (!heardSpeech && count >= frames(policy.noSpeechMs)) return "end";
      if (count >= frames(policy.maxMs)) return "end";
      return "continue";
    },
  };
}
