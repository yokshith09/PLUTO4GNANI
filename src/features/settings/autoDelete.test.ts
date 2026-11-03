import { describe, expect, it } from "vitest";
import { parseRules, previewText } from "./autoDelete";

describe("auto-delete rules (FR-46)", () => {
  it("is never by default and only takes the offered ages", () => {
    expect(parseRules(null)).toEqual({ time_days: 0, tasks_days: 0 });
    expect(parseRules({ time_days: 7, tasks_days: 90 })).toEqual({ time_days: 0, tasks_days: 90 });
  });

  it("says plainly what would happen", () => {
    expect(previewText({ time_sessions: 12, tasks_binned: 1, tasks_purged: 0 })).toBe("12 time records will be deleted and 1 completed task will move to the Bin.");
    expect(previewText({ time_sessions: 0, tasks_binned: 0, tasks_purged: 0 })).toMatch(/^Nothing is old enough/);
  });
});
