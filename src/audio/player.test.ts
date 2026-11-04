import { describe, expect, it, vi } from "vitest";
import { must, nth } from "./testing";
import { PLAYBACK_SAMPLE_RATE } from "./pcm";
import { PcmPlayer, type PlayerContext } from "./player";

interface FakeSource {
  buffer: { length: number; data: Float32Array } | null;
  startedAt: number | null;
  stopped: boolean;
  onended: (() => void) | null;
  connect: () => void;
  start: (t: number) => void;
  stop: () => void;
}

function fakeContext() {
  const sources: FakeSource[] = [];
  const ctx = {
    currentTime: 0,
    state: "running" as AudioContextState,
    destination: {} as AudioDestinationNode,
    resume: vi.fn(async () => undefined),
    close: vi.fn(async () => undefined),
    createBuffer: (_channels: number, length: number, rate: number) => {
      expect(rate).toBe(PLAYBACK_SAMPLE_RATE);
      const data = new Float32Array(length);
      return { length, data, getChannelData: () => data };
    },
    createBufferSource: () => {
      const s: FakeSource = {
        buffer: null,
        startedAt: null,
        stopped: false,
        onended: null,
        connect: () => undefined,
        start(t) {
          this.startedAt = t;
        },
        stop() {
          this.stopped = true;
        },
      };
      sources.push(s);
      return s;
    },
  };
  return { ctx: ctx as unknown as PlayerContext & { currentTime: number; state: AudioContextState }, sources, raw: ctx };
}

/** `n` samples of PCM16 LE with value `v`. */
function pcm(n: number, v = 0x1000): Uint8Array {
  const b = new Uint8Array(n * 2);
  const view = new DataView(b.buffer);
  for (let i = 0; i < n; i++) view.setInt16(i * 2, v, true);
  return b;
}

describe("PcmPlayer", () => {
  it("schedules chunks back to back with no gap", () => {
    const { ctx, sources } = fakeContext();
    const player = new PcmPlayer({ createContext: () => ctx, leadSeconds: 0.05 });
    player.enqueue(pcm(2400)); // 100 ms
    player.enqueue(pcm(4800)); // 200 ms
    player.enqueue(pcm(240)); // 10 ms
    expect(sources.map((s) => s.startedAt)).toEqual([0.05, 0.15000000000000002, 0.35000000000000003]);
    expect(player.playing).toBe(true);
  });

  it("decodes the samples into the buffer", () => {
    const { ctx, sources } = fakeContext();
    new PcmPlayer({ createContext: () => ctx }).enqueue(pcm(3, 0x4000));
    expect(Array.from(must(nth(sources, 0).buffer).data)).toEqual([0.5, 0.5, 0.5]);
  });

  it("carries a sample split across two chunks", () => {
    const { ctx, sources } = fakeContext();
    const player = new PcmPlayer({ createContext: () => ctx });
    player.enqueue(Uint8Array.from([0x00, 0x40, 0x00])); // one sample + the low byte of the next
    player.enqueue(Uint8Array.from([0xc0])); // its high byte
    expect(Array.from(must(nth(sources, 0).buffer).data)).toEqual([0.5]);
    expect(Array.from(must(nth(sources, 1).buffer).data)).toEqual([-0.5]);
  });

  it("ignores an empty chunk and does not open the device for it", () => {
    const create = vi.fn(() => fakeContext().ctx);
    const player = new PcmPlayer({ createContext: create });
    player.enqueue(new Uint8Array(0));
    player.enqueue(Uint8Array.from([1])); // half a sample: carried, nothing to play yet
    expect(create).not.toHaveBeenCalled();
    expect(player.playing).toBe(false);
  });

  it("stop() cancels everything queued (barge-in) and the next chunk starts fresh", () => {
    const { ctx, sources } = fakeContext();
    const onIdle = vi.fn();
    const player = new PcmPlayer({ createContext: () => ctx, onIdle, leadSeconds: 0.05 });
    player.enqueue(pcm(24_000)); // 1 s
    player.enqueue(pcm(24_000));
    ctx.currentTime = 0.3;
    player.stop();
    expect(sources.every((s) => s.stopped)).toBe(true);
    expect(player.playing).toBe(false);
    expect(onIdle).not.toHaveBeenCalled();
    player.enqueue(pcm(2400));
    expect(nth(sources, 2).startedAt).toBeCloseTo(0.35);
  });

  it("calls onIdle once the last queued chunk has ended", () => {
    const { ctx, sources } = fakeContext();
    const onIdle = vi.fn();
    const player = new PcmPlayer({ createContext: () => ctx, onIdle });
    player.enqueue(pcm(10));
    player.enqueue(pcm(10));
    nth(sources, 0).onended?.();
    expect(onIdle).not.toHaveBeenCalled();
    nth(sources, 1).onended?.();
    expect(onIdle).toHaveBeenCalledTimes(1);
    expect(player.playing).toBe(false);
  });

  it("restarts with the lead after an underrun", () => {
    const { ctx, sources } = fakeContext();
    const player = new PcmPlayer({ createContext: () => ctx, leadSeconds: 0.05 });
    player.enqueue(pcm(2400));
    nth(sources, 0).onended?.();
    ctx.currentTime = 5;
    player.enqueue(pcm(2400));
    expect(nth(sources, 1).startedAt).toBeCloseTo(5.05);
  });

  it("resumes a suspended context and closes it on close()", async () => {
    const { ctx, raw } = fakeContext();
    raw.state = "suspended";
    const player = new PcmPlayer({ createContext: () => ctx });
    player.enqueue(pcm(10));
    expect(raw.resume).toHaveBeenCalled();
    await player.close();
    expect(raw.close).toHaveBeenCalled();
    await player.close(); // twice is fine
    expect(raw.close).toHaveBeenCalledTimes(1);
  });

  it("stop() tolerates a source that already stopped", () => {
    const { ctx, sources } = fakeContext();
    const player = new PcmPlayer({ createContext: () => ctx });
    player.enqueue(pcm(10));
    nth(sources, 0).stop = () => {
      throw new Error("InvalidStateError");
    };
    expect(() => player.stop()).not.toThrow();
  });
});
