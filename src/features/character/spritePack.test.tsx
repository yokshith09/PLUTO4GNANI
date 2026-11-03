import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";
import { Bear } from "@/features/shell/Bear";
import { CHARACTERS, SPRITE_POSES } from "./Characters";
import { ATLAS_H, ATLAS_W, COLS, ROWS, buildManifest, buildPetJson, frameTransform, relightForMirror } from "./spritePack";

describe("sprite pack layout", () => {
  it("is 8 columns x 9 rows of 192x208 cells", () => {
    expect(ROWS).toHaveLength(9);
    expect(ATLAS_W).toBe(1536);
    expect(ATLAS_H).toBe(1872);
  });
  it("never has more frames than columns", () => {
    for (const r of ROWS) expect(r.frames).toBeLessThanOrEqual(COLS);
  });
  it("uses the expected frame counts", () => {
    expect(ROWS.map((r) => r.frames)).toEqual([6, 8, 8, 4, 5, 8, 6, 6, 6]);
  });
  it("has drawn art for every row; only running-left is a mirror", () => {
    const m = buildManifest("x", "X", "d");
    expect(m.rows.filter((r) => r.source !== "drawn").map((r) => [r.name, r.source])).toEqual([["running-left", "mirrored"]]);
    expect(m.rows.find((r) => r.name === "jumping")?.pose).toBe("jump");
    expect(m.rows.find((r) => r.name === "failed")?.pose).toBe("sad");
    expect(m.rows.find((r) => r.name === "review")?.pose).toBe("review");
    expect(m.rows.find((r) => r.name === "idle")?.source).toBe("drawn");
    expect(m.status).toBe("Unverified");
  });
  it("places frames inside their cell", () => {
    expect(frameTransform(0, 0)).toContain("translate(0,8)");
    expect(frameTransform(2, 1)).toContain("scale(-1,1)");
  });
  it("squashes the jump on take-off and landing", () => {
    expect(frameTransform(4, 0)).toMatch(/scale\(1\.1,0\.9\)/);
    expect(frameTransform(4, 2)).not.toMatch(/scale\(1\.1,0\.9\)/);
  });
  it("re-lights mirrored art so the key light stays top left", () => {
    const out = relightForMirror('<radialGradient id="a" cx="36%" cy="28%" fx="30%"></radialGradient><linearGradient x1="0" y1="0" x2="1" y2="1">');
    expect(out).toContain('cx="64%"');
    expect(out).toContain('fx="70%"');
    expect(out).toContain('cy="28%"');
    expect(out).toContain('x1="1" y1="0" x2="0"');
  });
});

describe("pet.json", () => {
  it("has only the OpenPets fields and points at the WebP", () => {
    expect(Object.keys(buildPetJson("pip", "Pip", "Pip, the penguin")).sort()).toEqual(["description", "displayName", "id", "spritesheetPath"]);
    expect(buildPetJson("a", "A", "d").spritesheetPath).toBe("spritesheet.webp");
  });
});

describe("sprite-only poses", () => {
  for (const pose of SPRITE_POSES) {
    it(`draws ${pose} for every character and the bear`, () => {
      for (const c of CHARACTERS) {
        const { container, unmount } = render(<c.Component pose={pose} />);
        expect(container.querySelector("svg")?.getAttribute("aria-label")).toContain(pose);
        if (pose === "review") expect(container.querySelector('[data-part="clipboard"]')).not.toBeNull();
        if (pose === "sad" && c.key !== "bolt") expect(container.querySelector('[data-part="tear"]')).not.toBeNull();
        unmount();
      }
      const { container } = render(<Bear pose={pose} />);
      expect(container.querySelector("svg")?.getAttribute("aria-label")).toContain(pose);
    });
  }
});
