/**
 * Where to put accessories on a sprite-pack character. A pack is a baked picture, so Pluto looks
 * at it: for every frame it finds the top of the head, how wide the head is and roughly where
 * the neck is, from which pixels are drawn. The same accessory drawings the bear wears are then
 * scaled onto that spot, frame by frame, so they follow the animation. Thin things (an antenna,
 * sparkles) are ignored on purpose. The size comes from the calm idle row only, so a pose with
 * the arms thrown out cannot make the gear too big (owner's penguin, 2026-10-27).
 */
export const CELL_W = 192;
export const CELL_H = 208;
export const COLS = 8;

export interface Anchor {
  /** Horizontal middle of the head, in cell pixels. */
  cx: number;
  /** The top of the head. */
  top: number;
  headW: number;
  neckY: number;
}
/** What is used until the picture has been looked at, or when it can't be (no canvas). */
export const DEFAULT_ANCHOR: Anchor = { cx: 96, top: 30, headW: 110, neckY: 120 };

/** The anchor of one cell, from its alpha values (w x h, row by row). `null` when nothing is drawn. */
export function anchorFromAlpha(alpha: ArrayLike<number>, w: number, h: number, threshold = 128): Anchor | null {
  const rows = Array.from({ length: h }, (_, y) => {
    let n = 0, lo = w, hi = -1;
    for (let x = 0; x < w; x++) {
      if ((alpha[y * w + x] ?? 0) < threshold) continue;
      n++;
      lo = Math.min(lo, x);
      hi = Math.max(hi, x);
    }
    return { n, lo, hi };
  });
  const widest = Math.max(0, ...rows.map((r) => r.n));
  if (widest === 0) return null;
  const solid = (y: number) => (rows[y]?.n ?? 0) >= widest * 0.3;
  let top = 0;
  while (top < h && !solid(top)) top++;
  let bottom = h - 1;
  while (bottom > top && !solid(bottom)) bottom--;
  const height = bottom - top + 1;
  if (height < 8) return null;
  const band = rows.slice(Math.round(top + height * 0.1), Math.round(top + height * 0.3) + 1).filter((r) => r.hi >= 0);
  if (band.length === 0) return null;
  const mean = (f: (r: (typeof band)[number]) => number) => band.reduce((t, r) => t + f(r), 0) / band.length;
  return { cx: mean((r) => (r.hi + r.lo) / 2), top, headW: mean((r) => r.hi - r.lo + 1), neckY: top + height * 0.55 };
}

/** One anchor per frame: `[row][column]`; a frame with nothing in it has none. */
export type SheetAnchors = (Anchor | null)[][];

/**
 * Makes the frames agree on size: every frame keeps its own head top and middle (the pet moves),
 * but the head width and the neck's distance below the head top are the idle row's average.
 */
export function steady(raw: SheetAnchors): SheetAnchors {
  const idle = (raw[0] ?? []).filter((a): a is Anchor => a !== null);
  if (idle.length === 0) return raw;
  const mean = (f: (a: Anchor) => number) => idle.reduce((t, a) => t + f(a), 0) / idle.length;
  const headW = mean((a) => a.headW);
  const neckDy = mean((a) => a.neckY - a.top);
  return raw.map((line) => line.map((a) => (a ? { ...a, headW, neckY: a.top + neckDy } : null)));
}

const cache = new Map<string, Promise<SheetAnchors | null>>();

/** Looks at a whole sheet once (and remembers). `null` where the browser can't read pixels. */
export function loadAnchors(sheet: string): Promise<SheetAnchors | null> {
  let p = cache.get(sheet);
  if (!p) {
    p = new Promise<SheetAnchors | null>((resolve) => {
      try {
        const img = new Image();
        img.onerror = () => resolve(null);
        img.onload = () => {
          try {
            const canvas = document.createElement("canvas");
            canvas.width = img.naturalWidth;
            canvas.height = img.naturalHeight;
            const ctx = canvas.getContext("2d", { willReadFrequently: true });
            if (!ctx) return resolve(null);
            ctx.drawImage(img, 0, 0);
            const rows = Math.floor(img.naturalHeight / CELL_H);
            const out: SheetAnchors = [];
            for (let r = 0; r < rows; r++) {
              const line: (Anchor | null)[] = [];
              for (let c = 0; c < COLS; c++) {
                const px = ctx.getImageData(c * CELL_W, r * CELL_H, CELL_W, CELL_H).data;
                const alpha = new Uint8Array(CELL_W * CELL_H);
                for (let i = 0; i < alpha.length; i++) alpha[i] = px[i * 4 + 3] ?? 0;
                line.push(anchorFromAlpha(alpha, CELL_W, CELL_H));
              }
              out.push(line);
            }
            resolve(steady(out));
          } catch {
            resolve(null);
          }
        };
        img.src = sheet;
      } catch {
        resolve(null);
      }
    });
    cache.set(sheet, p);
  }
  return p;
}

/**
 * The two places gear goes, as SVG transforms. Gear is drawn in the bear's 200x200 space (head top
 * at y 28, neck gear centred on y 128), so it is scaled by head width relative to the bear's 120.
 */
export function placement(a: Anchor): { head: string; neck: string } {
  const s = a.headW / 120;
  const x = a.cx - 100 * s;
  return {
    head: `translate(${x} ${a.top - 28 * s}) scale(${s})`,
    neck: `translate(${x} ${a.neckY - 128 * s}) scale(${s})`,
  };
}
