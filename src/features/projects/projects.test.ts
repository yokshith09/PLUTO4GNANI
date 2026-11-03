import { describe, expect, it } from "vitest";
import type { NoteSummary, Task, TaskRow } from "@/ipc";
import { projectsFrom } from "./ProjectsPage";

const task = (id: string, project: string | null, status: Task["status"] = "PLANNED", meeting = false): TaskRow =>
  ({ task: { id, title: id, project, status, source_action_item_id: meeting ? "a1" : null, created_at: 1 } as Task, overdue: false, done_today: false });
const note = (id: string, project: string | null) => ({ id, project } as NoteSummary);

describe("projects from tasks and notes", () => {
  it("groups by project name, counts open tasks, and keeps a project just started", () => {
    const ps = projectsFrom(
      [task("a", "Thesis"), task("b", "Thesis", "COMPLETED"), task("c", null), task("m", "Standup", "PLANNED", true)],
      [note("n1", "Thesis"), note("n2", "Website"), note("n3", null)],
      ["New one"],
    );
    expect(ps.map((p) => [p.name, p.tasks.length, p.open, p.notes.length])).toEqual([
      ["New one", 0, 0, 0],
      ["Thesis", 2, 1, 1],
      ["Website", 0, 0, 1],
    ]);
  });
});
