import { describe, expect, it } from "vitest";
import type { TaskRow } from "@/ipc";
import { summarize } from "./summary";

const T = "2026-10-26";
const row = (id: string, p: Record<string, unknown> = {}): TaskRow =>
  ({ task: { id, title: id, status: "PLANNED", starred: false, due_date: null, created_at: Number(id.replace(/\D/g, "")) || 0, ...p }, overdue: false, done_today: false }) as unknown as TaskRow;

describe("project summary", () => {
  it("counts open, done, overdue and the percent finished", () => {
    const s = summarize([row("a1"), row("a2", { status: "COMPLETED" }), row("a3", { status: "CANCELLED" }), row("a4", { due_date: "2026-10-20" })], T);
    expect(s).toMatchObject({ open: 2, done: 1, total: 3, percent: 33, overdue: 1 });
  });
  it("is empty and 0% with no tasks", () => {
    expect(summarize([], T)).toMatchObject({ open: 0, total: 0, percent: 0, next: null, nextDeadline: null });
  });
  it("picks in progress, then starred, then the nearest deadline, then the oldest", () => {
    expect(summarize([row("a1"), row("a2", { status: "IN_PROGRESS" })], T).next?.task.id).toBe("a2");
    expect(summarize([row("a1"), row("a2", { starred: true })], T).next?.task.id).toBe("a2");
    expect(summarize([row("a1", { due_date: "2026-11-05" }), row("a2", { due_date: "2026-10-28" })], T).next?.task.id).toBe("a2");
    expect(summarize([row("a2"), row("a1")], T).next?.task.id).toBe("a1");
  });
  it("finds the nearest deadline that is not past", () => {
    const s = summarize([row("a1", { due_date: "2026-10-20" }), row("a2", { due_date: "2026-11-02" }), row("a3", { due_date: "2026-10-29" })], T);
    expect(s.nextDeadline).toBe("2026-10-29");
  });
});

import { planRename } from "./summary";

describe("renaming a project", () => {
  const tasks = [row("a1", { project: "Old" }), row("a2", { project: "Other" }), row("a3", { project: "Old", status: "COMPLETED" })];
  const notes = [{ id: "n1", project: "Old" }, { id: "n2", project: null }];
  it("finds every task and note under the old name, finished ones too", () => {
    expect(planRename(tasks, notes, "Old", " New ")).toEqual({ ok: true, taskIds: ["a1", "a3"], noteIds: ["n1"] });
  });
  it("does nothing for an empty or unchanged name", () => {
    expect(planRename(tasks, notes, "Old", "  ").ok).toBe(false);
    expect(planRename(tasks, notes, "Old", "Old").ok).toBe(false);
  });
});
