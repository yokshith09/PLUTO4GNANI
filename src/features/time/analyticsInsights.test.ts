import { describe, expect, it } from "vitest";
import { changeLabel, insights, type AnalyticsFacts } from "./analyticsInsights";

const base: AnalyticsFacts = {
  total: 36_000, prevTotal: 30_000, days: 7,
  categories: [["Coding", 21_600], ["Research", 9_000]],
  hourly: Array.from({ length: 24 }, (_, h) => (h === 14 ? 9_000 : h === 10 ? 4_000 : 0)),
  perDay: [{ date: new Date("2026-10-20T12:00:00"), seconds: 5_000 }, { date: new Date("2026-10-21T12:00:00"), seconds: 12_000 }],
  topApp: { app: "VS Code", seconds: 18_000 },
};

describe("analytics insights", () => {
  it("says what happened in plain words, at most four sentences", () => {
    const out = insights(base);
    expect(out.length).toBeLessThanOrEqual(4);
    expect(out[0]).toBe("Screen time was 20% higher than the previous 7 days.");
    expect(out).toContain("Coding took the biggest share of your time: 60%.");
    expect(out).toContain("VS Code was your most used app, at 5h.");
  });

  it("calls a small change 'about the same' and names the day before for one day", () => {
    expect(insights({ ...base, prevTotal: 35_000, days: 1 })[0]).toBe("Screen time was about the same as the day before.");
  });

  it("has nothing to say about an empty period, and no comparison without a before", () => {
    expect(insights({ ...base, total: 0 })).toEqual([]);
    expect(insights({ ...base, prevTotal: 0 })[0]).toMatch(/^Coding took/);
  });

  it("never judges: a drop is stated, not scolded", () => {
    const out = insights({ ...base, total: 20_000, prevTotal: 40_000 }).join(" ");
    expect(out).toContain("50% lower");
    expect(out).not.toMatch(/only|just|less than you should|slacking|worse|failed/i);
  });

  it("labels the change against the period before", () => {
    expect(changeLabel(36_000, 30_000)).toBe("+20%");
    expect(changeLabel(15_000, 30_000)).toBe("−50%");
    expect(changeLabel(30_000, 30_000)).toBe("0%");
    expect(changeLabel(10, 0)).toBeNull();
  });
});
