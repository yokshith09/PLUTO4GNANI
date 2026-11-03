import { describe, expect, it } from "vitest";
import { rangeBounds } from "./range";

describe("rangeBounds", () => {
  const now = new Date(2026, 9, 4, 15, 30);
  it("covers whole local days", () => {
    const t = rangeBounds("today", now);
    expect(new Date(t.from).getHours()).toBe(0);
    expect(t.to - t.from).toBe(86_400_000);
    expect(rangeBounds("yesterday", now).to).toBe(t.from);
  });
  it("7 and 30 days end tomorrow 00:00 and have that many days", () => {
    expect(rangeBounds("7d", now).to).toBe(t(now).to);
    expect(Math.round((rangeBounds("30d", now).to - rangeBounds("30d", now).from) / 86_400_000)).toBe(30);
  });
});
const t = (n: Date) => rangeBounds("today", n);
