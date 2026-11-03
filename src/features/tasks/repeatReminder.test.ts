import { describe, expect, it } from "vitest";
import type { TaskRow } from "@/ipc";
import { REPEAT_REMIND_DEFAULT, dueNow, openRepeatsMessage, parseRepeatRemind } from "./repeatReminder";

const row = (title: string, repeats: boolean, doneToday = false): TaskRow => ({
  overdue: false, done_today: doneToday,
  task: { id: title, title, description: "", status: "PLANNED", priority: null, project: null, planned_date: "2026-10-19", due_date: null, note_id: null, source_action_item_id: null, created_at: 0, updated_at: 0, started_at: null, completed_at: null, cancelled_at: null, starred: false, repeat: repeats ? { kind: "daily" } : null, repeat_until: null, days: [], links: [] },
});

describe("evening reminder for repeating tasks (FR-109)", () => {
  it("defaults to on at 19:00 and survives broken data", () => {
    expect(parseRepeatRemind(null)).toEqual(REPEAT_REMIND_DEFAULT);
    expect(parseRepeatRemind({ on: "yes", at: "7pm", remindedOn: 3 })).toEqual(REPEAT_REMIND_DEFAULT);
    expect(parseRepeatRemind({ on: false, at: "20:30", remindedOn: "2026-10-23" })).toEqual({ on: false, at: "20:30", remindedOn: "2026-10-23" });
  });

  it("is due once, after its time, only when switched on", () => {
    const at = (h: number, m = 0) => new Date(2026, 9, 23, h, m);
    const s = REPEAT_REMIND_DEFAULT;
    expect(dueNow(s, at(18, 59), "2026-10-23")).toBe(false);
    expect(dueNow(s, at(19, 0), "2026-10-23")).toBe(true);
    expect(dueNow({ ...s, remindedOn: "2026-10-23" }, at(21), "2026-10-23")).toBe(false);
    expect(dueNow({ ...s, on: false }, at(21), "2026-10-23")).toBe(false);
  });

  it("names only repeating tasks not ticked today, kindly", () => {
    expect(openRepeatsMessage([row("Report", false)])).toBeNull();
    expect(openRepeatsMessage([row("Walk", true, true)])).toBeNull();
    expect(openRepeatsMessage([row("Walk", true)])).toBe("Still open today: Walk — tick it when done.");
    expect(openRepeatsMessage(["A", "B", "C", "D", "E"].map((t) => row(t, true)))).toBe("Still open today: A, B, C +2 more — tick them when done.");
  });
});
