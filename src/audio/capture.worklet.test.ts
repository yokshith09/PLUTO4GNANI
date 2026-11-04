import { describe, expect, it, vi } from "vitest";
import { nth } from "./testing";
import { FRAME_BYTES } from "./pcm";

type Msg = { type: "frame"; frame: ArrayBuffer } | { type: "stopped" };
interface Processor {
  port: { onmessage: ((e: { data: unknown }) => void) | null; postMessage: (m: Msg, t?: unknown[]) => void };
  process(inputs: Float32Array[][]): boolean;
}

/** Loads the worklet with the AudioWorkletGlobalScope globals faked, and returns one processor. */
async function loadProcessor(rate: number): Promise<{ proc: Processor; sent: Msg[]; name: string }> {
  const sent: Msg[] = [];
  let registered: { name: string; ctor: new () => Processor } | null = null;
  vi.stubGlobal("sampleRate", rate);
  vi.stubGlobal(
    "AudioWorkletProcessor",
    class {
      port = { onmessage: null, postMessage: (m: Msg) => sent.push(m) };
    },
  );
  vi.stubGlobal("registerProcessor", (name: string, ctor: new () => Processor) => {
    registered = { name, ctor };
  });
  vi.resetModules();
  await import("./capture.worklet");
  const r = registered as { name: string; ctor: new () => Processor } | null;
  if (!r) throw new Error("the worklet did not register a processor");
  // `sampleRate` is read when the processor is constructed, as in a real worklet scope.
  const proc = new r.ctor();
  vi.unstubAllGlobals();
  return { proc, sent, name: r.name };
}

describe("capture worklet", () => {
  it("registers under the name capture.ts expects", async () => {
    const { name } = await loadProcessor(48_000);
    expect(name).toBe("pluto-capture");
  });

  it("turns 48 kHz render quanta into 1,024-byte frames", async () => {
    const { proc, sent } = await loadProcessor(48_000);
    const quantum = new Float32Array(128).fill(0.25);
    for (let i = 0; i < 12; i++) expect(proc.process([[quantum]])).toBe(true); // 1,536 in → 512 out
    expect(sent).toHaveLength(1);
    const first = nth(sent, 0);
    expect(first.type).toBe("frame");
    if (first.type === "frame") expect(first.frame.byteLength).toBe(FRAME_BYTES);
  });

  it("ignores a missing or empty input", async () => {
    const { proc, sent } = await loadProcessor(48_000);
    expect(proc.process([])).toBe(true);
    expect(proc.process([[new Float32Array(0)]])).toBe(true);
    expect(sent).toHaveLength(0);
  });

  it("on stop flushes the partial frame, says stopped, and ends processing", async () => {
    const { proc, sent } = await loadProcessor(16_000);
    proc.process([[new Float32Array(100).fill(0.5)]]);
    proc.port.onmessage?.({ data: "other" });
    expect(sent).toHaveLength(0);
    proc.port.onmessage?.({ data: "stop" });
    expect(sent.map((m) => m.type)).toEqual(["frame", "stopped"]);
    expect(proc.process([[new Float32Array(128)]])).toBe(false);
    proc.port.onmessage?.({ data: "stop" });
    expect(sent).toHaveLength(2);
  });

  it("on stop with nothing buffered only says stopped", async () => {
    const { proc, sent } = await loadProcessor(16_000);
    proc.port.onmessage?.({ data: "stop" });
    expect(sent.map((m) => m.type)).toEqual(["stopped"]);
  });
});
