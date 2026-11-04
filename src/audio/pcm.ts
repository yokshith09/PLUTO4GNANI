// Pure audio maths for the hybrid audio path (ARCHITECTURE §4). Owner: D1 (T-012).
//
// Runs inside the capture AudioWorklet (no DOM, no imports beyond this file) and in tests, so it
// must stay dependency-free. The formats are contracts (API_CONTRACTS §0 "Audio formats"); they
// mirror `pluto_contracts::speech` and never change without a contract PR.

/** Mic → Rust: PCM16 little-endian, mono, 16 kHz. */
export const CAPTURE_SAMPLE_RATE = 16_000;
/** Every frame sent to `audio_push` is exactly this many bytes (512 samples, 32 ms). */
export const FRAME_BYTES = 1_024;
/** Rust → speaker: PCM16 little-endian, mono, 24 kHz (Timbre `linear_pcm`). */
export const PLAYBACK_SAMPLE_RATE = 24_000;

/** Float sample in [-1, 1] → Int16, clamped and rounded. */
export function floatToInt16(s: number): number {
  const c = s > 1 ? 1 : s < -1 ? -1 : Number.isNaN(s) ? 0 : s;
  // `| 0` turns -0 (a tiny negative value rounded) into 0, as an Int16Array would store it.
  return Math.round(c < 0 ? c * 0x8000 : c * 0x7fff) | 0;
}

/**
 * Streaming downsampler from the device rate (usually 48 or 44.1 kHz) to 16 kHz Int16.
 *
 * Each output sample is the average of the input samples in its window (a box filter), which
 * removes most energy above the new Nyquist frequency before decimating. The fractional window
 * position and any unused input carry over between calls, so the output does not depend on how
 * the input is split into blocks (the worklet gets 128-sample render quanta).
 */
export class Downsampler {
  /** Unused input, starting at absolute input index `base`. */
  private rest: Float32Array = new Float32Array(0);
  private base = 0;
  /** Output samples produced so far. */
  private produced = 0;

  constructor(readonly inputRate: number, readonly outputRate: number = CAPTURE_SAMPLE_RATE) {
    if (!(inputRate >= outputRate) || outputRate <= 0) {
      throw new RangeError(`cannot downsample ${inputRate} Hz to ${outputRate} Hz`);
    }
  }

  /** First input index of output sample `n`. Computed from integers, so block splits never drift. */
  private windowStart(n: number): number {
    return Math.floor((n * this.inputRate) / this.outputRate);
  }

  process(input: Float32Array): Int16Array {
    let data: Float32Array;
    if (this.rest.length === 0) {
      data = input;
    } else {
      data = new Float32Array(this.rest.length + input.length);
      data.set(this.rest, 0);
      data.set(input, this.rest.length);
    }
    const available = this.base + data.length;
    const out = new Int16Array(Math.ceil((data.length * this.outputRate) / this.inputRate) + 1);
    let n = 0;
    for (;;) {
      const start = this.windowStart(this.produced);
      const end = this.windowStart(this.produced + 1);
      if (end > available) break;
      let sum = 0;
      for (let i = start; i < end; i++) sum += data[i - this.base] ?? 0;
      out[n++] = floatToInt16(sum / (end - start));
      this.produced++;
    }
    const next = this.windowStart(this.produced);
    this.rest = data.slice(next - this.base);
    this.base = next;
    return out.subarray(0, n);
  }
}

/**
 * Packs Int16 samples into fixed-size little-endian frames, carrying a partial frame over to the
 * next call. `flush` pads the last partial frame with silence so every frame is exactly
 * `frameBytes` long.
 */
export class Framer {
  private readonly samplesPerFrame: number;
  private pending: Int16Array;
  private filled = 0;

  constructor(readonly frameBytes: number = FRAME_BYTES) {
    if (frameBytes <= 0 || frameBytes % 2 !== 0) throw new RangeError("frame size must be a positive even number of bytes");
    this.samplesPerFrame = frameBytes / 2;
    this.pending = new Int16Array(this.samplesPerFrame);
  }

  /** Samples waiting for the next frame. */
  get buffered(): number {
    return this.filled;
  }

  push(samples: Int16Array): ArrayBuffer[] {
    const frames: ArrayBuffer[] = [];
    let i = 0;
    while (i < samples.length) {
      const take = Math.min(this.samplesPerFrame - this.filled, samples.length - i);
      this.pending.set(samples.subarray(i, i + take), this.filled);
      this.filled += take;
      i += take;
      if (this.filled === this.samplesPerFrame) {
        frames.push(this.encode());
        this.filled = 0;
      }
    }
    return frames;
  }

  /** The last partial frame, padded with silence; `null` when nothing is waiting. */
  flush(): ArrayBuffer | null {
    if (this.filled === 0) return null;
    this.pending.fill(0, this.filled);
    const frame = this.encode();
    this.filled = 0;
    return frame;
  }

  private encode(): ArrayBuffer {
    const buf = new ArrayBuffer(this.frameBytes);
    const view = new DataView(buf);
    for (let s = 0; s < this.samplesPerFrame; s++) view.setInt16(s * 2, this.pending[s] ?? 0, true);
    return buf;
  }
}

/**
 * PCM16 little-endian bytes → Float32 samples in [-1, 1). An odd trailing byte is ignored here;
 * the player carries it over to the next chunk.
 */
export function pcm16ToFloat32(bytes: Uint8Array): Float32Array {
  const n = bytes.length >> 1;
  const view = new DataView(bytes.buffer, bytes.byteOffset, n * 2);
  const out = new Float32Array(n);
  for (let i = 0; i < n; i++) out[i] = view.getInt16(i * 2, true) / 0x8000;
  return out;
}
