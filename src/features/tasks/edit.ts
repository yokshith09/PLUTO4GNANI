import type { Task, TaskEdit } from "@/ipc";

/** The editable state of a task as the edit command wants it (everything is sent, so nothing is lost). */
export const toEdit = (t: Task): TaskEdit => ({
  title: t.title, description: t.description, project: t.project, priority: t.priority,
  planned_date: t.planned_date, due_date: t.due_date, note_id: t.note_id,
});
