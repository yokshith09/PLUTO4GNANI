import { describe, expect, it } from "vitest";
import { daysAgo, parseYmd, ymd } from "./dates";

describe("dates", () => {
  it("formats and parses local days", () => {
    const d = new Date(2026, 9, 4, 23, 59);
    expect(ymd(d)).toBe("2026-10-04");
    expect(ymd(parseYmd("2026-10-04"))).toBe("2026-10-04");
  });
  it("steps back across a month boundary", () => {
    expect(ymd(daysAgo(5, new Date(2026, 9, 3)))).toBe("2026-09-28");
  });
});

describe("overdue days (owner 2026-10-25)", () => {
  it("counts whole days past the deadline", async () => {
    const { daysBetween, overdueLabel } = await import("./dates");
    expect(daysBetween("2026-10-22", "2026-10-25")).toBe(3);
    expect(daysBetween("2026-10-25", "2026-10-22")).toBe(-3);
    // Across the end of daylight saving time (Europe and the US move clocks in Oct/Nov).
    expect(daysBetween("2026-10-24", "2026-11-03")).toBe(10);
    expect(overdueLabel("2026-10-24", "2026-10-25")).toBe("1 day overdue");
    expect(overdueLabel("2026-10-20", "2026-10-25")).toBe("5 days overdue");
    expect(overdueLabel("2024-01-01", "2026-10-25")).toBe("Over a year overdue");
  });
});
