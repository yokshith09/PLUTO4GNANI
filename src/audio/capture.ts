// Microphone capture (ARCHITECTURE §4, steps 1–3). Owner: D1 (T-012).
//
// getUserMedia (mono, echo cancellation, noise suppression) → AudioContext at the device rate →
// the capture worklet → 1,024-byte PCM16 16 kHz frames handed to `onFrame` (usually a
// `FrameSender`). Capture keeps running while the window is hidden: an AudioContext is not
// throttled like timers are.

import workletUrl from "./workletUrl";

export const CAPTURE_PROCESSOR = "pluto-capture";

/** The `VoiceErrorCode`s capture can produce (API_CONTRACTS §6). */
export type CaptureErrorCode = "mic_permission_denied" | "mic_unavailable";

export class CaptureError extends Error {
  constructor(readonly code: CaptureErrorCode, message: string) {
    super(message);
    this.name = "CaptureError";
  }
}

export const MIC_DENIED_MESSAGE =
  "Pluto isn't allowed to use the microphone. Allow it in Windows Settings → Privacy & security → Microphone, then try again.";
export const MIC_UNAVAILABLE_MESSAGE = "No working microphone was found. Plug one in or check it isn't in use, then try again.";

/** Maps a getUserMedia failure to the contract's error codes. */
export function toCaptureError(e: unknown): CaptureError {
  const name = e && typeof e === "object" && "name" in e ? String((e as { name: unknown }).name) : "";
  if (name === "NotAllowedError" || name === "SecurityError") return new CaptureError("mic_permission_denied", MIC_DENIED_MESSAGE);
  return new CaptureError("mic_unavailable", MIC_UNAVAILABLE_MESSAGE);
}

export interface CaptureOptions {
  /** One 1,024-byte frame, in order. Called on the main thread about 31 times a second. */
  onFrame: (frame: ArrayBuffer) => void;
  /** The microphone went away mid-capture (unplugged, permission revoked). Capture has stopped. */
  onEnded?: () => void;
  /** Injection points for tests. */
  mediaDevices?: Pick<MediaDevices, "getUserMedia"> | undefined;
  createContext?: () => AudioContext;
  createWorkletNode?: (ctx: AudioContext, name: string) => AudioWorkletNode;
  moduleUrl?: string;
  /** How long `stop()` waits for the worklet's last frame. */
  stopTimeoutMs?: number;
}

export interface CaptureHandle {
  /** The device rate the worklet downsamples from (usually 48000 or 44100). */
  readonly deviceSampleRate: number;
  /** Flushes the last partial frame, then releases the microphone. Safe to call twice. */
  stop(): Promise<void>;
}

export const MIC_CONSTRAINTS: MediaStreamConstraints = {
  audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true, autoGainControl: true },
  video: false,
};

type WorkletMessage = { type: "frame"; frame: ArrayBuffer } | { type: "stopped" };

export async function startCapture(options: CaptureOptions): Promise<CaptureHandle> {
  const devices = options.mediaDevices ?? (typeof navigator !== "undefined" ? navigator.mediaDevices : undefined);
  if (!devices?.getUserMedia) throw new CaptureError("mic_unavailable", MIC_UNAVAILABLE_MESSAGE);

  let stream: MediaStream;
  try {
    stream = await devices.getUserMedia(MIC_CONSTRAINTS);
  } catch (e) {
    throw toCaptureError(e);
  }

  const ctx = options.createContext ? options.createContext() : new AudioContext();
  const releaseStream = () => stream.getTracks().forEach((t) => t.stop());
  let node: AudioWorkletNode;
  let source: MediaStreamAudioSourceNode;
  try {
    await ctx.audioWorklet.addModule(options.moduleUrl ?? workletUrl);
    source = ctx.createMediaStreamSource(stream);
    node = options.createWorkletNode
      ? options.createWorkletNode(ctx, CAPTURE_PROCESSOR)
      : new AudioWorkletNode(ctx, CAPTURE_PROCESSOR, { numberOfInputs: 1, numberOfOutputs: 0, channelCount: 1 });
    source.connect(node);
    if (ctx.state === "suspended") await ctx.resume();
  } catch {
    releaseStream();
    await ctx.close().catch(() => undefined);
    throw new CaptureError("mic_unavailable", MIC_UNAVAILABLE_MESSAGE);
  }

  let stopped = false;
  let onStopped: (() => void) | null = null;
  node.port.onmessage = (e: MessageEvent<WorkletMessage>) => {
    if (e.data.type === "frame") options.onFrame(e.data.frame);
    else onStopped?.();
  };

  const teardown = async () => {
    node.port.onmessage = null;
    source.disconnect();
    node.disconnect();
    releaseStream();
    await ctx.close().catch(() => undefined);
  };

  const stop = async () => {
    if (stopped) return;
    stopped = true;
    await new Promise<void>((resolve) => {
      const timer = setTimeout(resolve, options.stopTimeoutMs ?? 250);
      onStopped = () => {
        clearTimeout(timer);
        resolve();
      };
      node.port.postMessage("stop");
    });
    await teardown();
  };

  for (const track of stream.getAudioTracks()) {
    track.addEventListener("ended", () => {
      if (stopped) return;
      stopped = true;
      void teardown().then(() => options.onEnded?.());
    });
  }

  return { deviceSampleRate: ctx.sampleRate, stop };
}
