import { describe, expect, it } from "vitest";
import { must, nth } from "./testing";
import { CAPTURE_SAMPLE_RATE, Downsampler, FRAME_BYTES, Framer, floatToInt16, pcm16ToFloat32 } from "./pcm";

function sine(freq: number, rate: number, seconds: number, amp = 0.5): Float32Array {
  const n = Math.round(rate * seconds);
  const out = new Float32Array(n);
  for (let i = 0; i < n; i++) out[i] = amp * Math.sin((2 * Math.PI * freq * i) / rate);
  return out;
}

/** Feeds `input` in blocks of `block` samples (the worklet's render quantum is 128). */
function runInBlocks(d: Downsampler, input: Float32Array, block: number): Int16Array {
  const parts: Int16Array[] = [];
  for (let i = 0; i < input.length; i += block) parts.push(d.process(input.subarray(i, i + block)));
  const total = parts.reduce((s, p) => s + p.length, 0);
  const out = new Int16Array(total);
  let o = 0;
  for (const p of parts) {
    out.set(p, o);
    o += p.length;
  }
  return out;
}

describe("floatToInt16", () => {
  it("maps the full range and clamps beyond it", () => {
    expect(floatToInt16(0)).toBe(0);
    expect(floatToInt16(1)).toBe(32767);
    expect(floatToInt16(-1)).toBe(-32768);
    expect(floatToInt16(2)).toBe(32767);
    expect(floatToInt16(-3)).toBe(-32768);
    expect(floatToInt16(Number.NaN)).toBe(0);
  });
});

describe("Downsampler", () => {
  it("turns one second at 48 kHz into exactly one second at 16 kHz", () => {
    const out = runInBlocks(new Downsampler(48_000), new Float32Array(48_000), 128);
    expect(out.length).toBe(CAPTURE_SAMPLE_RATE);
  });

  it("handles 44.1 kHz (a fractional ratio) without drift", () => {
    const out = runInBlocks(new Downsampler(44_100), new Float32Array(44_100 * 3), 128);
    expect(Math.abs(out.length - 3 * CAPTURE_SAMPLE_RATE)).toBeLessThanOrEqual(1);
  });

  it("passes 16 kHz through unchanged", () => {
    const input = sine(440, 16_000, 0.1);
    const out = new Downsampler(16_000).process(input);
    expect(Array.from(out)).toEqual(Array.from(input, floatToInt16));
  });

  it("gives the same output however the input is split into blocks", () => {
    const input = sine(700, 48_000, 0.25);
    const whole = new Downsampler(48_000).process(input);
    for (const block of [1, 7, 128, 1000]) {
      expect(Array.from(runInBlocks(new Downsampler(48_000), input, block))).toEqual(Array.from(whole));
    }
    const input44 = sine(700, 44_100, 0.25);
    const whole44 = new Downsampler(44_100).process(input44);
    expect(Array.from(runInBlocks(new Downsampler(44_100), input44, 128))).toEqual(Array.from(whole44));
  });

  it("keeps a speech-band tone's frequency and level", () => {
    const out = new Downsampler(48_000).process(sine(1_000, 48_000, 0.5, 0.5));
    const expected = sine(1_000, 16_000, 0.5, 0.5);
    // A 3-sample box filter attenuates 1 kHz by about 1% and delays it by one input sample.
    let maxErr = 0;
    for (let i = 10; i < out.length - 10; i++) {
      const shifted = 0.5 * Math.sin((2 * Math.PI * 1_000 * (i * 3 + 1)) / 48_000);
      maxErr = Math.max(maxErr, Math.abs((out[i] ?? 0) / 32768 - shifted));
    }
    expect(maxErr).toBeLessThan(0.02);
    expect(out.length).toBe(expected.length);
  });

  it("removes most energy above the new Nyquist frequency", () => {
    // 15 kHz at 48 kHz is exactly three samples per two cycles; the box filter cancels most of it.
    const out = new Downsampler(48_000).process(sine(15_000, 48_000, 0.1, 0.5));
    const peak = out.reduce((m, s) => Math.max(m, Math.abs(s)), 0) / 32768;
    expect(peak).toBeLessThan(0.2);
  });

  it("refuses to upsample", () => {
    expect(() => new Downsampler(8_000)).toThrow(RangeError);
    expect(() => new Downsampler(48_000, 0)).toThrow(RangeError);
  });
});

describe("Framer", () => {
  it("emits frames of exactly 1,024 bytes (512 samples) and carries the rest over", () => {
    const f = new Framer();
    expect(f.push(new Int16Array(300))).toEqual([]);
    expect(f.buffered).toBe(300);
    const frames = f.push(new Int16Array(800));
    expect(frames).toHaveLength(2);
    for (const frame of frames) expect(frame.byteLength).toBe(FRAME_BYTES);
    expect(f.buffered).toBe(300 + 800 - 1024);
  });

  it("writes little-endian samples in order across frame boundaries", () => {
    const f = new Framer(8); // 4 samples per frame
    const frames = f.push(Int16Array.from([1, -2, 300, -32768, 32767, 5]));
    expect(frames).toHaveLength(1);
    const v = new DataView(nth(frames, 0));
    expect([0, 2, 4, 6].map((o) => v.getInt16(o, true))).toEqual([1, -2, 300, -32768]);
    expect(new Uint8Array(nth(frames, 0), 0, 2)).toEqual(Uint8Array.from([1, 0]));
    const last = must(f.flush());
    const lv = new DataView(last);
    expect([0, 2, 4, 6].map((o) => lv.getInt16(o, true))).toEqual([32767, 5, 0, 0]);
  });

  it("flush pads with silence and returns null when empty", () => {
    const f = new Framer();
    expect(f.flush()).toBeNull();
    f.push(Int16Array.from([7]));
    const frame = must(f.flush());
    expect(frame.byteLength).toBe(FRAME_BYTES);
    expect(new DataView(frame).getInt16(0, true)).toBe(7);
    expect(new Int16Array(frame).slice(1).every((s) => s === 0)).toBe(true);
    expect(f.flush()).toBeNull();
  });

  it("rejects odd or empty frame sizes", () => {
    expect(() => new Framer(0)).toThrow(RangeError);
    expect(() => new Framer(1023)).toThrow(RangeError);
  });

  it("48 kHz render quanta become 31.25 frames per second end to end", () => {
    const d = new Downsampler(48_000);
    const f = new Framer();
    const input = sine(300, 48_000, 2);
    let frames = 0;
    for (let i = 0; i < input.length; i += 128) frames += f.push(d.process(input.subarray(i, i + 128))).length;
    expect(frames).toBe(62); // 32,000 samples / 512, with 256 samples still buffered
    expect(f.buffered).toBe(256);
  });
});

describe("pcm16ToFloat32", () => {
  it("decodes little-endian samples and ignores an odd trailing byte", () => {
    const bytes = Uint8Array.from([0x00, 0x80, 0xff, 0x7f, 0x00, 0x00, 0x12]);
    expect(Array.from(pcm16ToFloat32(bytes))).toEqual([-1, 32767 / 32768, 0]);
  });

  it("respects the view's offset into a larger buffer", () => {
    const backing = Uint8Array.from([9, 9, 0x00, 0x40]);
    expect(Array.from(pcm16ToFloat32(backing.subarray(2)))).toEqual([0.5]);
  });
});
