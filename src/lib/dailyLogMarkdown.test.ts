import { describe, expect, it } from "vitest";
import type { DailyLog } from "@/ipc";
import { dailyLogMarkdown } from "./dailyLogMarkdown";

const log = (over: Partial<DailyLog["snapshot"]> = {}, live = false): DailyLog => ({
  date: "2026-10-04", live, reconstructed: false,
  snapshot: {
    date: "2026-10-04", planned: [{ id: "p", title: "Write spec", priority: "HIGH" }], in_progress: [],
    completed: [{ id: "c", title: "Ship it" }], pending: [], cancelled: [], overdue: [],
    stats: { planned_count: 2, completed_count: 1, completion_ratio: 0.5, tasks_created: 2, notes_created: 1, notes_edited: 0, meetings: 0 },
    activity: null, ...over,
  },
});

describe("dailyLogMarkdown", () => {
  it("lists the stats and each non-empty section", () => {
    const md = dailyLogMarkdown(log());
    expect(md).toContain("# Daily log — Sunday, October 4, 2026");
    expect(md).toContain("Completed 1 of 2 (50%)");
    expect(md).toContain("## Completed\n- [x] Ship it");
    expect(md).toContain("## Planned\n- [ ] Write spec (High)");
    expect(md).not.toContain("## Cancelled");
  });
  it("says when the day is still open, and handles no plan", () => {
    const md = dailyLogMarkdown(log({ stats: { planned_count: 0, completed_count: 0, completion_ratio: null, tasks_created: 0, notes_created: 0, notes_edited: 0, meetings: 0 }, planned: [], completed: [] }, true));
    expect(md).toContain("still open");
    expect(md).toContain("Completed 0 of 0 ·");
  });
});
