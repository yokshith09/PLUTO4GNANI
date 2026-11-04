import { beforeEach, describe, expect, it, vi } from "vitest";
import { nth } from "./testing";

const core = vi.hoisted(() => ({
  isTauri: vi.fn(() => false),
  invoke: vi.fn(async () => undefined),
  channels: [] as Array<{ onmessage: ((m: unknown) => void) | null }>,
}));

vi.mock("@tauri-apps/api/core", () => ({
  isTauri: core.isTauri,
  invoke: core.invoke,
  Channel: class {
    onmessage: ((m: unknown) => void) | null = null;
    constructor() {
      core.channels.push(this);
    }
  },
}));

import { AUDIO_PUSH, SESSION_HEADER, playbackChannel, pushFrame, toBytes } from "./ipc";

beforeEach(() => {
  core.isTauri.mockReset().mockReturnValue(false);
  core.invoke.mockClear();
  core.channels.length = 0;
});

describe("pushFrame", () => {
  it("sends the frame as the raw body with the session header in the app", async () => {
    core.isTauri.mockReturnValue(true);
    const frame = Uint8Array.from([1, 2, 3]).buffer;
    await pushFrame("sess-1", frame);
    expect(core.invoke).toHaveBeenCalledWith(AUDIO_PUSH, new Uint8Array(frame), { headers: { [SESSION_HEADER]: "sess-1" } });
  });

  it("does nothing outside the app (no Rust side, no network)", async () => {
    await pushFrame("s", new ArrayBuffer(4));
    expect(core.invoke).not.toHaveBeenCalled();
  });
});

describe("playback channel", () => {
  it("accepts raw buffers and JSON number arrays alike", () => {
    expect(toBytes(Uint8Array.from([1, 2]))).toEqual(Uint8Array.from([1, 2]));
    expect(toBytes(Uint8Array.from([3]).buffer)).toEqual(Uint8Array.from([3]));
    expect(toBytes([4, 5])).toEqual(Uint8Array.from([4, 5]));
  });

  it("hands every message to the callback as bytes", () => {
    const got: Uint8Array[] = [];
    playbackChannel((pcm) => got.push(pcm));
    const ch = nth(core.channels, 0);
    ch.onmessage?.([1, 2]);
    ch.onmessage?.(Uint8Array.from([3, 4]).buffer);
    expect(got).toEqual([Uint8Array.from([1, 2]), Uint8Array.from([3, 4])]);
  });
});
