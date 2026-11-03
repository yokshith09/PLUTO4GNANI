import { describe, expect, it } from "vitest";
import type { Task } from "@/ipc";
import { dueLine, stepIndex, taskHints } from "./hints";

const T = "2026-10-26";
const task = (p: Partial<Task>): Task => ({
  id: "t", title: "x", description: "", status: "PLANNED", priority: null, project: null, planned_date: null, due_date: null, note_id: null,
  source_action_item_id: null, created_at: 0, updated_at: 0, started_at: null, completed_at: null, cancelled_at: null,
  starred: false, repeat: null, repeat_until: null, days: [], links: [], ...p,
}) as Task;

describe("task hints", () => {
  it("offers to move an overdue task to today, in kind words", () => {
    const [h] = taskHints(task({ due_date: "2026-10-24" }), T);
    expect(h?.id).toBe("overdue");
    expect(h?.text).toContain("2 days ago");
    expect(h?.action?.id).toBe("move-today");
  });
  it("says when a deadline is close", () => {
    expect(taskHints(task({ due_date: T, planned_date: "2026-10-27" }), T)[0]?.text).toBe("Due today.");
    expect(taskHints(task({ due_date: "2026-10-27", planned_date: "2026-10-27" }), T)[0]?.text).toBe("Due tomorrow.");
  });
  it("suggests starting a task planned for today", () => {
    expect(taskHints(task({ planned_date: T }), T)[0]?.action?.id).toBe("start");
  });
  it("points out a task that will not show on Today, and offers a day", () => {
    expect(taskHints(task({}), T)[0]?.action?.id).toBe("set-today");
    expect(taskHints(task({ repeat: { kind: "daily" } as never }), T)).toEqual([]);
  });
  it("shows at most two and nothing for a finished task", () => {
    expect(taskHints(task({ due_date: "2026-10-20", planned_date: T }), T)).toHaveLength(2);
    expect(taskHints(task({ status: "COMPLETED", due_date: "2026-10-01" }), T)).toEqual([]);
    expect(taskHints(task({ status: "CANCELLED" }), T)).toEqual([]);
  });
});

describe("due line and steps", () => {
  it("reads naturally", () => {
    expect(dueLine(task({}), T)).toBe("");
    expect(dueLine(task({ due_date: "2026-10-25" }), T)).toBe("Overdue by 1 day");
    expect(dueLine(task({ due_date: T }), T)).toBe("Due today");
    expect(dueLine(task({ due_date: "2026-10-27" }), T)).toBe("Due tomorrow");
    expect(dueLine(task({ due_date: "2026-10-30" }), T)).toBe("Due in 4 days");
  });
  it("lights the right step", () => {
    expect(stepIndex("PLANNED")).toBe(0);
    expect(stepIndex("PENDING")).toBe(1);
    expect(stepIndex("COMPLETED")).toBe(2);
    expect(stepIndex("CANCELLED")).toBe(-1);
  });
});
