import { describe, expect, it, vi } from "vitest";

vi.mock("./workletUrl", () => ({ default: "/assets/capture.worklet.js" }));

import {
  CAPTURE_PROCESSOR,
  CaptureError,
  MIC_CONSTRAINTS,
  MIC_DENIED_MESSAGE,
  startCapture,
  toCaptureError,
  type CaptureOptions,
} from "./capture";

class FakeTrack {
  stopped = false;
  private listeners: Array<() => void> = [];
  stop() {
    this.stopped = true;
  }
  addEventListener(_type: string, cb: () => void) {
    this.listeners.push(cb);
  }
  end() {
    this.listeners.forEach((cb) => cb());
  }
}

function setup(opts: { getUserMedia?: () => Promise<unknown>; addModule?: () => Promise<void>; state?: AudioContextState; workletReplies?: boolean } = {}) {
  const track = new FakeTrack();
  const stream = { getTracks: () => [track], getAudioTracks: () => [track] };
  const source = { connect: vi.fn(), disconnect: vi.fn() };
  const port = {
    onmessage: null as ((e: { data: unknown }) => void) | null,
    postMessage: vi.fn((msg: unknown) => {
      if (msg === "stop" && opts.workletReplies !== false) {
        port.onmessage?.({ data: { type: "frame", frame: new ArrayBuffer(1024) } });
        port.onmessage?.({ data: { type: "stopped" } });
      }
    }),
  };
  const node = { port, disconnect: vi.fn() };
  const ctx = {
    sampleRate: 48_000,
    state: opts.state ?? "running",
    audioWorklet: { addModule: vi.fn(opts.addModule ?? (async () => undefined)) },
    createMediaStreamSource: vi.fn(() => source),
    resume: vi.fn(async () => undefined),
    close: vi.fn(async () => undefined),
  };
  const getUserMedia = vi.fn(opts.getUserMedia ?? (async () => stream));
  const createWorkletNode = vi.fn(() => node);
  const onFrame = vi.fn();
  const onEnded = vi.fn();
  const options: CaptureOptions = {
    onFrame,
    onEnded,
    mediaDevices: { getUserMedia } as unknown as Pick<MediaDevices, "getUserMedia">,
    createContext: () => ctx as unknown as AudioContext,
    createWorkletNode: createWorkletNode as unknown as CaptureOptions["createWorkletNode"] & object,
    stopTimeoutMs: 20,
  };
  return { track, source, port, node, ctx, getUserMedia, createWorkletNode, onFrame, onEnded, options };
}

describe("startCapture", () => {
  it("asks for a mono, echo-cancelled, noise-suppressed mic and wires it to the worklet", async () => {
    const t = setup();
    const handle = await startCapture(t.options);
    expect(t.getUserMedia).toHaveBeenCalledWith(MIC_CONSTRAINTS);
    expect(MIC_CONSTRAINTS.audio).toMatchObject({ channelCount: 1, echoCancellation: true, noiseSuppression: true });
    expect(t.ctx.audioWorklet.addModule).toHaveBeenCalledWith("/assets/capture.worklet.js");
    expect(t.createWorkletNode).toHaveBeenCalledWith(t.ctx, CAPTURE_PROCESSOR);
    expect(t.source.connect).toHaveBeenCalledWith(t.node);
    expect(handle.deviceSampleRate).toBe(48_000);
  });

  it("forwards every frame from the worklet in order", async () => {
    const t = setup();
    await startCapture(t.options);
    const a = new ArrayBuffer(1024);
    const b = new ArrayBuffer(1024);
    t.port.onmessage?.({ data: { type: "frame", frame: a } });
    t.port.onmessage?.({ data: { type: "frame", frame: b } });
    expect(t.onFrame.mock.calls.map(([f]) => f)).toEqual([a, b]);
  });

  it("stop() flushes the last frame, then releases the mic and the device", async () => {
    const t = setup();
    const handle = await startCapture(t.options);
    await handle.stop();
    expect(t.port.postMessage).toHaveBeenCalledWith("stop");
    expect(t.onFrame).toHaveBeenCalledTimes(1); // the flushed partial frame
    expect(t.track.stopped).toBe(true);
    expect(t.source.disconnect).toHaveBeenCalled();
    expect(t.node.disconnect).toHaveBeenCalled();
    expect(t.ctx.close).toHaveBeenCalled();
    await handle.stop();
    expect(t.ctx.close).toHaveBeenCalledTimes(1);
  });

  it("stop() still releases the mic when the worklet never answers", async () => {
    const t = setup({ workletReplies: false });
    const handle = await startCapture(t.options);
    await handle.stop();
    expect(t.track.stopped).toBe(true);
    expect(t.ctx.close).toHaveBeenCalled();
  });

  it("resumes a suspended context", async () => {
    const t = setup({ state: "suspended" });
    await startCapture(t.options);
    expect(t.ctx.resume).toHaveBeenCalled();
  });

  it("reports a lost microphone once and stops", async () => {
    const t = setup();
    const handle = await startCapture(t.options);
    t.track.end();
    await vi.waitFor(() => expect(t.onEnded).toHaveBeenCalledTimes(1));
    expect(t.ctx.close).toHaveBeenCalled();
    await handle.stop(); // nothing left to do
    expect(t.port.postMessage).not.toHaveBeenCalled();
  });

  it("maps a denied permission to mic_permission_denied", async () => {
    const t = setup({ getUserMedia: async () => Promise.reject(new DOMException("denied", "NotAllowedError")) });
    await expect(startCapture(t.options)).rejects.toMatchObject({ code: "mic_permission_denied", message: MIC_DENIED_MESSAGE });
  });

  it("maps a missing device to mic_unavailable", async () => {
    const t = setup({ getUserMedia: async () => Promise.reject(new DOMException("none", "NotFoundError")) });
    await expect(startCapture(t.options)).rejects.toMatchObject({ code: "mic_unavailable" });
  });

  it("reports mic_unavailable when there is no getUserMedia at all", async () => {
    const t = setup();
    await expect(startCapture({ ...t.options, mediaDevices: {} as Pick<MediaDevices, "getUserMedia"> })).rejects.toBeInstanceOf(CaptureError);
  });

  it("releases the mic if the worklet cannot load", async () => {
    const t = setup({ addModule: async () => Promise.reject(new Error("blocked")) });
    await expect(startCapture(t.options)).rejects.toMatchObject({ code: "mic_unavailable" });
    expect(t.track.stopped).toBe(true);
    expect(t.ctx.close).toHaveBeenCalled();
  });
});

describe("toCaptureError", () => {
  it("treats SecurityError as a permission problem and anything else as unavailable", () => {
    expect(toCaptureError({ name: "SecurityError" }).code).toBe("mic_permission_denied");
    expect(toCaptureError({ name: "NotReadableError" }).code).toBe("mic_unavailable");
    expect(toCaptureError("weird").code).toBe("mic_unavailable");
  });
});
