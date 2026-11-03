import { describe, expect, it } from "vitest";
import { DAILY_MAX, canNudge, emptyNudges, inQuietHours, markNudged, parseNudges } from "./nudges";

const at = (h: number, m = 0, day = "2026-10-23") => new Date(`${day}T${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}:00`);

describe("notification budget (FR-96)", () => {
  it("allows each kind once a day", () => {
    let s = emptyNudges();
    expect(canNudge(s, "routine", at(18))).toBe(true);
    s = markNudged(s, "routine", at(18));
    expect(canNudge(s, "routine", at(20))).toBe(false);
    expect(canNudge(s, "routine", at(18, 0, "2026-10-24"))).toBe(true);
  });

  it("keeps at least an hour between unasked nudges", () => {
    const s = markNudged(emptyNudges(), "greeting", at(9));
    expect(canNudge(s, "routine", at(9, 30))).toBe(false);
    expect(canNudge(s, "routine", at(10, 1))).toBe(true);
  });

  it(`stops after ${DAILY_MAX} nudges a day`, () => {
    let s = emptyNudges();
    ["a", "b", "c", "d"].forEach((k, i) => { s = markNudged(s, k, at(9 + i * 2)); });
    expect(canNudge(s, "e", at(19))).toBe(false);
  });

  it("stays quiet at night (22:00-08:00 by default)", () => {
    const s = emptyNudges();
    expect(inQuietHours(s, at(23))).toBe(true);
    expect(inQuietHours(s, at(7, 59))).toBe(true);
    expect(inQuietHours(s, at(8))).toBe(false);
    expect(canNudge(s, "routine", at(23))).toBe(false);
    expect(canNudge({ ...s, quiet: { on: false, from: "22:00", to: "08:00" } }, "routine", at(23))).toBe(true);
  });

  it("reads broken stored data safely and forgets old days", () => {
    expect(parseNudges(null)).toEqual(emptyNudges());
    const s = markNudged(markNudged(emptyNudges(), "x", at(9, 0, "2026-10-01")), "y", at(9));
    expect(Object.keys(s.days)).toEqual(["2026-10-23"]);
  });

  it("uses the user's own limits (FR-96)", () => {
    let s = { ...emptyNudges(), maxPerDay: 1, gapMin: 15 };
    s = markNudged(s, "a", at(9));
    expect(canNudge(s, "b", at(12))).toBe(false); // one a day
    const t = markNudged({ ...emptyNudges(), gapMin: 15 }, "a", at(9));
    expect(canNudge(t, "b", at(9, 16))).toBe(true); // 15 minutes apart is enough
    expect(parseNudges({ maxPerDay: 99, gapMin: 7 })).toMatchObject({ maxPerDay: 4, gapMin: 60 });
    expect(parseNudges({ maxPerDay: 2, gapMin: 30 })).toMatchObject({ maxPerDay: 2, gapMin: 30 });
  });
});
