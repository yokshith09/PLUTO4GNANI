import { describe, expect, it } from "vitest";
import type { DailyLog, LogEntry, TaskRow } from "@/ipc";
import { yesterdaySummary } from "./YesterdayLine";

const e = (id: string): LogEntry => ({ id, title: id });
const row = (id: string, status: TaskRow["task"]["status"]): TaskRow => ({
  overdue: false, done_today: false,
  task: { id, title: id, description: "", status, priority: null, project: null, planned_date: null, due_date: null, note_id: null, source_action_item_id: null, created_at: 0, updated_at: 0, started_at: null, completed_at: null, cancelled_at: null, starred: false, repeat: null, repeat_until: null, days: [], links: [] },
});
const log = (left: LogEntry[]): DailyLog => ({
  date: "2026-10-11", live: false, reconstructed: false,
  snapshot: {
    date: "2026-10-11", planned: left.slice(0, 1), in_progress: left.slice(1, 2), pending: left.slice(2), completed: [e("done")], cancelled: [], overdue: [],
    stats: { planned_count: 4, completed_count: 1, completion_ratio: 0.25, tasks_created: 0, notes_created: 0, notes_edited: 0, meetings: 0 },
    activity: null,
  },
} as DailyLog);

describe("yesterday's summary", () => {
  it("counts what got done and lists what is still open now", () => {
    const now = [row("a", "PLANNED"), row("b", "COMPLETED"), row("c", "IN_PROGRESS"), row("other", "PLANNED")];
    expect(yesterdaySummary(log([e("a"), e("b"), e("c")]), now)).toEqual({ done: 1, planned: 4, open: ["a", "c"] });
  });
  it("says nothing without a log", () => {
    expect(yesterdaySummary(null, [])).toBeNull();
  });
});
