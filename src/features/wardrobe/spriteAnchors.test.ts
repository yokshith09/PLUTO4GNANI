import { describe, expect, it } from "vitest";
import { DEFAULT_ANCHOR, anchorFromAlpha, placement, steady } from "./spriteAnchors";

const W = 60, H = 100;
function draw(parts: { x0: number; x1: number; y0: number; y1: number }[]) {
  const a = new Uint8Array(W * H);
  for (const p of parts) for (let y = p.y0; y <= p.y1; y++) for (let x = p.x0; x <= p.x1; x++) a[y * W + x] = 255;
  return a;
}

describe("looking at a sprite frame", () => {
  it("finds the head top, width and neck, ignoring a thin antenna and a faint shadow", () => {
    const a = draw([
      { x0: 29, x1: 30, y0: 2, y1: 19 }, // antenna: 2 px wide
      { x0: 10, x1: 49, y0: 20, y1: 49 }, // head: 40 wide
      { x0: 15, x1: 44, y0: 50, y1: 89 }, // body
    ]);
    a[95 * W + 30] = 40; // faint shadow below the threshold
    const r = anchorFromAlpha(a, W, H);
        expect(r?.top).toBe(20);
    expect(r?.cx).toBeCloseTo(29.5, 0);
    expect(r?.headW).toBeCloseTo(40, 0);
    expect(r?.neckY).toBeGreaterThan(20);
    expect(r?.neckY).toBeLessThan(90);
  });

  it("says nothing for an empty or tiny frame", () => {
    expect(anchorFromAlpha(new Uint8Array(W * H), W, H)).toBeNull();
    expect(anchorFromAlpha(draw([{ x0: 5, x1: 40, y0: 10, y1: 14 }]), W, H)).toBeNull();
  });

  it("scales the bear-sized gear onto a head", () => {
    expect(placement({ cx: 96, top: 30, headW: 120, neckY: 128 }).head).toBe("translate(-4 2) scale(1)");
    expect(placement({ cx: 96, top: 30, headW: 60, neckY: 100 }).neck).toBe("translate(46 36) scale(0.5)");
    expect(placement(DEFAULT_ANCHOR).neck).toMatch(/^translate\(/);
  });

  it("sizes gear from the calm idle row, so a pose with the arms out cannot make it huge", () => {
    const idle = { cx: 96, top: 40, headW: 60, neckY: 80 };
    const arms = { cx: 100, top: 20, headW: 170, neckY: 90 }; // flippers thrown wide
    const out = steady([[idle, null], [arms, null]]);
    expect(out[1]?.[0]).toEqual({ cx: 100, top: 20, headW: 60, neckY: 60 }); // own top and middle, idle's size
    expect(out[0]?.[1]).toBeNull();
    expect(steady([[null], [arms]])).toEqual([[null], [arms]]); // no idle frame: left alone
  });
});
