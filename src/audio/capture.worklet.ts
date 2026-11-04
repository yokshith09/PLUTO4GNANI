// Capture AudioWorklet (ARCHITECTURE §4, step 2–3). Owner: D1 (T-012).
//
// Runs on the audio rendering thread. It only glues the pure, tested code in `pcm.ts` to the
// worklet API: device-rate float samples in, 1,024-byte PCM16 16 kHz frames out over `port`.
// Loaded by `capture.ts` through Vite's `?worker&url`, which bundles `pcm.ts` into one
// same-origin file (the CSP allows no blob: URLs).
//
// Messages: in `"stop"` → flushes the last partial frame (padded with silence), then out
// `{ type: "stopped" }`. Out `{ type: "frame", frame: ArrayBuffer }` (transferred).

import { Downsampler, Framer } from "./pcm";

// AudioWorkletGlobalScope is not in TypeScript's DOM lib.
declare const sampleRate: number;
declare class AudioWorkletProcessor {
  readonly port: MessagePort;
}
declare function registerProcessor(name: string, processor: new () => AudioWorkletProcessor): void;

/** Must match `CAPTURE_PROCESSOR` in `capture.ts`. */
const CAPTURE_PROCESSOR = "pluto-capture";

class CaptureProcessor extends AudioWorkletProcessor {
  private readonly downsampler = new Downsampler(sampleRate);
  private readonly framer = new Framer();
  private running = true;

  constructor() {
    super();
    this.port.onmessage = (e: MessageEvent) => {
      if (e.data !== "stop" || !this.running) return;
      this.running = false;
      const last = this.framer.flush();
      if (last) this.port.postMessage({ type: "frame", frame: last }, [last]);
      this.port.postMessage({ type: "stopped" });
    };
  }

  process(inputs: Float32Array[][]): boolean {
    const channel = inputs[0]?.[0];
    if (this.running && channel && channel.length > 0) {
      for (const frame of this.framer.push(this.downsampler.process(channel))) {
        this.port.postMessage({ type: "frame", frame }, [frame]);
      }
    }
    return this.running;
  }
}

registerProcessor(CAPTURE_PROCESSOR, CaptureProcessor);
