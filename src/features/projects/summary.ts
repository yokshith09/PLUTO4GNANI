import type { TaskRow } from "@/ipc";
import { daysBetween } from "@/lib/dates";

const finished = (r: TaskRow) => r.task.status === "COMPLETED" || r.task.status === "CANCELLED";

export interface ProjectSummary {
  open: number;
  done: number;
  total: number;
  /** 0–100. */
  percent: number;
  overdue: number;
  /** The nearest deadline among open tasks, if any. */
  nextDeadline: string | null;
  /** What to do next: in progress first, then starred, then the nearest deadline, then the oldest. */
  next: TaskRow | null;
}

/** Counts and the "next up" task for one project. Pure, so it is tested. */
export function summarize(rows: TaskRow[], today: string): ProjectSummary {
  const open = rows.filter((r) => !finished(r));
  const done = rows.filter((r) => r.task.status === "COMPLETED").length;
  const total = open.length + done; // cancelled tasks are neither open nor done
  const dated = open.filter((r) => r.task.due_date).map((r) => r.task.due_date as string).sort();
  const rank = (r: TaskRow) => [
    r.task.status === "IN_PROGRESS" ? 0 : 1,
    r.task.starred ? 0 : 1,
    r.task.due_date ? daysBetween(today, r.task.due_date) : 9999,
    r.task.created_at,
  ];
  const next = [...open].sort((a, b) => {
    const [x, y] = [rank(a), rank(b)];
    for (let i = 0; i < x.length; i++) if (x[i] !== y[i]) return (x[i] as number) - (y[i] as number);
    return 0;
  })[0] ?? null;
  return {
    open: open.length,
    done,
    total,
    percent: total === 0 ? 0 : Math.round((done / total) * 100),
    overdue: open.filter((r) => r.task.due_date && r.task.due_date < today).length,
    nextDeadline: dated.find((d) => d >= today) ?? null,
    next,
  };
}

/** What changing a project's name touches: every task and note filed under `from`. */
export function planRename(tasks: TaskRow[], notes: { id: string; project: string | null }[], from: string, to: string): { ok: boolean; taskIds: string[]; noteIds: string[] } {
  const name = to.trim().slice(0, 100);
  if (!name || name === from) return { ok: false, taskIds: [], noteIds: [] };
  return {
    ok: true,
    taskIds: tasks.filter((r) => r.task.project === from).map((r) => r.task.id),
    noteIds: notes.filter((n) => n.project === from).map((n) => n.id),
  };
}
