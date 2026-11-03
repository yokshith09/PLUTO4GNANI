import type { Task } from "@/ipc";
import { daysBetween } from "@/lib/dates";
import { isFinished } from "@/lib/taskStatus";

/** One thing the editor can suggest. `action` names the one-click fix the editor offers, if any. */
export interface Hint {
  id: "overdue" | "due-soon" | "start" | "no-day" | "finish";
  text: string;
  action?: { id: "move-today" | "start" | "set-today"; label: string };
}

/**
 * Plain, kind suggestions for an open task (no-guilt rule: say what can be done, never what was
 * not). At most two, most useful first. Pure, so it is tested.
 */
export function taskHints(task: Task, today: string): Hint[] {
  if (isFinished(task.status)) return [];
  const out: Hint[] = [];
  const left = task.due_date ? daysBetween(today, task.due_date) : null;
  if (left !== null && left < 0) {
    out.push({ id: "overdue", text: `The deadline was ${-left === 1 ? "yesterday" : `${-left} days ago`}. Make it due today, or pick a new deadline.`, action: { id: "move-today", label: "Make it due today" } });
  } else if (left !== null && left <= 2) {
    out.push({ id: "due-soon", text: left === 0 ? "Due today." : left === 1 ? "Due tomorrow." : "Due in 2 days." });
  }
  const plannedToday = task.planned_date === today || task.days.includes(today);
  if (task.status === "PLANNED" && plannedToday) {
    out.push({ id: "start", text: "Planned for today. Start it when you are ready.", action: { id: "start", label: "Start now" } });
  } else if (task.status === "PLANNED" && !task.planned_date && task.days.length === 0 && !task.repeat) {
    out.push({ id: "no-day", text: "No day yet, so it will not show on Today.", action: { id: "set-today", label: "Plan for today" } });
  } else if (task.status === "IN_PROGRESS") {
    out.push({ id: "finish", text: "In progress. Mark it complete when it is done." });
  }
  return out.slice(0, 2);
}

/** "Overdue by 2 days", "Due today", "Due in 3 days", or "" with no deadline. */
export function dueLine(task: Task, today: string): string {
  if (!task.due_date) return "";
  const n = daysBetween(today, task.due_date);
  if (n < 0) return `Overdue by ${-n} ${-n === 1 ? "day" : "days"}`;
  if (n === 0) return "Due today";
  if (n === 1) return "Due tomorrow";
  return `Due in ${n} days`;
}

/** The three steps a task moves through, for the progress track in the editor. */
export const STEPS = [
  { id: "PLANNED", label: "Planned" },
  { id: "IN_PROGRESS", label: "In progress" },
  { id: "COMPLETED", label: "Completed" },
] as const;

/** Which step is lit: Pending counts as in progress, Cancelled shows none. */
export function stepIndex(status: Task["status"]): number {
  if (status === "PLANNED") return 0;
  if (status === "IN_PROGRESS" || status === "PENDING") return 1;
  if (status === "COMPLETED") return 2;
  return -1;
}
