import { describe, expect, it } from "vitest";
import type { FocusSession, Reminder, TaskRow } from "@/ipc";
import { glanceText } from "./glance";

const task = (title: string, over: Partial<TaskRow["task"]> = {}): TaskRow => ({
  overdue: false, done_today: false,
  task: { id: title, title, description: "", status: "PLANNED", priority: null, project: null, planned_date: null, due_date: null, note_id: null, source_action_item_id: null, created_at: 0, updated_at: 0, started_at: null, completed_at: null, cancelled_at: null, starred: false, repeat: null, repeat_until: null, days: [], links: [], ...over },
});
const focus: FocusSession = { id: "f", kind: "FOCUS", task_id: null, label: "Physics", planned_ms: 25 * 60_000, started_at: 0, paused_at: null, paused_ms: 0, ended_at: null, outcome: null, local_date: "2026-10-23" };
const reminder = { id: "r", title: "Stand-up", remind_at: new Date(2026, 9, 23, 15, 30).getTime(), note_id: null, fired_at: null, done_at: null, created_at: 0, starred: false } as Reminder;

describe("today at a glance as a bubble, when the hover card is off (FR-110)", () => {
  it("puts focus time first, then the task that matters, then the next reminder", () => {
    const text = glanceText({ session: focus, remaining: 13 * 60_000 + 12_000, tasks: [task("Low", { priority: "LOW" }), task("Lab report", { priority: "HIGH" })], next: reminder, now: 0 });
    expect(text).toMatch(/^Focus 13:12 left · 2 today · next: Lab report · Stand-up at /);
  });

  it("says something kind when nothing is planned", () => {
    expect(glanceText({ session: null, remaining: 0, tasks: [], now: 0 })).toBe("Nothing planned. Rest counts too.");
  });
});
