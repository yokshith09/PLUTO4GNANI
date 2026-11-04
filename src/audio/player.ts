// Gapless PCM player (ARCHITECTURE §4, playback). Owner: D1 (T-012).
//
// Turns PCM16 24 kHz chunks from the `playback` Channel into AudioBuffers and schedules each to
// start exactly where the previous one ends, so streamed TTS plays without clicks or gaps.
// `stop()` cancels everything queued (barge-in). A chunk that splits a sample in half keeps its
// odd byte for the next chunk.

import { PLAYBACK_SAMPLE_RATE, pcm16ToFloat32 } from "./pcm";

/** The part of `AudioContext` the player uses (lets tests pass a fake). */
export type PlayerContext = Pick<
  AudioContext,
  "currentTime" | "state" | "destination" | "createBuffer" | "createBufferSource" | "resume" | "close"
>;

export interface PlayerOptions {
  createContext?: () => PlayerContext;
  /** Delay before the first chunk (and after an underrun), so scheduling never starts in the past. */
  leadSeconds?: number;
  /** Everything queued has finished playing (not called after `stop()`). */
  onIdle?: () => void;
}

export class PcmPlayer {
  private ctx: PlayerContext | null = null;
  private nextTime = 0;
  private readonly sources = new Set<AudioBufferSourceNode>();
  private carry: number | null = null;

  constructor(private readonly options: PlayerOptions = {}) {}

  /** Whether any chunk is queued or playing. */
  get playing(): boolean {
    return this.sources.size > 0;
  }

  /** Queues one chunk of PCM16 LE mono 24 kHz. */
  enqueue(chunk: Uint8Array): void {
    let bytes = chunk;
    if (this.carry !== null) {
      const joined = new Uint8Array(chunk.length + 1);
      joined[0] = this.carry;
      joined.set(chunk, 1);
      bytes = joined;
      this.carry = null;
    }
    if (bytes.length % 2 === 1) this.carry = bytes[bytes.length - 1] ?? null;
    const samples = pcm16ToFloat32(bytes);
    if (samples.length === 0) return;

    const ctx = this.context();
    if (ctx.state === "suspended") void ctx.resume();
    const buffer = ctx.createBuffer(1, samples.length, PLAYBACK_SAMPLE_RATE);
    buffer.getChannelData(0).set(samples);
    const source = ctx.createBufferSource();
    source.buffer = buffer;
    source.connect(ctx.destination);

    const lead = this.options.leadSeconds ?? 0.05;
    const start = Math.max(ctx.currentTime + (this.sources.size === 0 ? lead : 0), this.nextTime);
    source.onended = () => {
      this.sources.delete(source);
      if (this.sources.size === 0) this.options.onIdle?.();
    };
    this.sources.add(source);
    source.start(start);
    this.nextTime = start + samples.length / PLAYBACK_SAMPLE_RATE;
  }

  /** Barge-in: stops what is playing and drops everything queued. */
  stop(): void {
    for (const source of this.sources) {
      source.onended = null;
      try {
        source.stop();
      } catch {
        // Already stopped.
      }
    }
    this.sources.clear();
    this.nextTime = 0;
    this.carry = null;
  }

  /** Stops playback and releases the audio device. */
  async close(): Promise<void> {
    this.stop();
    const ctx = this.ctx;
    this.ctx = null;
    await ctx?.close().catch(() => undefined);
  }

  private context(): PlayerContext {
    this.ctx ??= this.options.createContext ? this.options.createContext() : new AudioContext();
    return this.ctx;
  }
}
