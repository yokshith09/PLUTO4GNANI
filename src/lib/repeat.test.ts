import { describe, expect, it } from "vitest";
import { isoWeekday, occursOn, repeatDays, repeatLabel, taskOccurs } from "./repeat";

describe("repeat rules (FR-109), same answers as the core", () => {
  it("knows ISO weekdays", () => {
    expect(isoWeekday("2026-10-19")).toBe(1); // Monday
    expect(isoWeekday("2026-10-25")).toBe(7); // Sunday
  });

  it("repeats only between the start and the end", () => {
    const mwf = { kind: "weekly" as const, days: [1, 3, 5] };
    expect(repeatDays(mwf, "2026-10-19", "2026-10-30", "2026-10-01", "2026-11-30")).toEqual(
      ["2026-10-19", "2026-10-21", "2026-10-23", "2026-10-26", "2026-10-28", "2026-10-30"]);
    expect(repeatDays({ kind: "every", n: 3 }, "2026-10-19", null, "2026-10-19", "2026-10-28")).toEqual(
      ["2026-10-19", "2026-10-22", "2026-10-25", "2026-10-28"]);
    expect(occursOn({ kind: "daily" }, "2026-10-19", null, "2026-10-18")).toBe(false);
    expect(occursOn({ kind: "daily" }, "2026-10-19", "2026-10-30", "2026-10-31")).toBe(false);
  });

  it("counts every-N days across a clock change", () => {
    // Europe moves clocks on 25 Oct 2026; days must still count as whole days.
    expect(occursOn({ kind: "every", n: 2 }, "2026-10-24", null, "2026-10-26")).toBe(true);
  });

  it("a task without a rule or a start never repeats", () => {
    expect(taskOccurs({ repeat: null, planned_date: "2026-10-19", repeat_until: null }, "2026-10-19")).toBe(false);
    expect(taskOccurs({ repeat: { kind: "daily" }, planned_date: null, repeat_until: null }, "2026-10-19")).toBe(false);
  });

  it("names rules plainly", () => {
    expect(repeatLabel({ kind: "daily" })).toBe("Every day");
    expect(repeatLabel({ kind: "weekly", days: [1, 2, 3, 4, 5] })).toBe("Weekdays");
    expect(repeatLabel({ kind: "weekly", days: [1, 3, 5] })).toBe("Mon, Wed, Fri");
    expect(repeatLabel({ kind: "every", n: 3 })).toBe("Every 3 days");
  });
});
