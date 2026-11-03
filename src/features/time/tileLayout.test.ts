import { describe, expect, it } from "vitest";
import { DEFAULT_LAYOUT, WIDTH_CLASS, parseLayout } from "./tileLayout";

describe("Time tile layout (FR-44): two choices", () => {
  it("defaults to side by side and survives broken prefs", () => {
    expect(parseLayout(null)).toEqual(DEFAULT_LAYOUT);
    expect(parseLayout({ width: "huge", height: "tall" })).toEqual({ width: "side" });
  });
  it("reads the old three widths as the two", () => {
    expect(parseLayout({ width: "wide" })).toEqual({ width: "side" });
    expect(parseLayout({ width: "even" })).toEqual({ width: "side" });
    expect(parseLayout({ width: "stacked" })).toEqual({ width: "full" });
    expect(parseLayout({ width: "full" })).toEqual({ width: "full" });
  });
  it("always fills the 12-column row with no gap", () => {
    expect(Object.keys(WIDTH_CLASS)).toHaveLength(2);
    for (const [a, b] of Object.values(WIDTH_CLASS)) {
      const n = (c: string) => Number(c.split("-").pop());
      expect(n(a) + n(b) === 12 || (n(a) === 12 && n(b) === 12)).toBe(true);
    }
  });
});
