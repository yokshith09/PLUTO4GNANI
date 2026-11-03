import type { TaskStatus } from "@/ipc";

export const STATUS_LABEL: Record<TaskStatus, string> = {
  PLANNED: "Planned",
  IN_PROGRESS: "In progress",
  PENDING: "Pending",
  COMPLETED: "Completed",
  CANCELLED: "Cancelled",
};

export const STATUSES: readonly TaskStatus[] = ["PLANNED", "IN_PROGRESS", "PENDING", "COMPLETED", "CANCELLED"];

/** PRD §6.2.1. `pluto-core` is the authority and rejects anything else; this only decides which buttons to show. */
const MOVES: Record<TaskStatus, readonly TaskStatus[]> = {
  PLANNED: ["IN_PROGRESS", "COMPLETED", "CANCELLED"],
  IN_PROGRESS: ["PENDING", "COMPLETED", "CANCELLED"],
  PENDING: ["IN_PROGRESS", "COMPLETED", "CANCELLED"],
  COMPLETED: ["PLANNED"],
  CANCELLED: ["PLANNED"],
};

function label(from: TaskStatus, to: TaskStatus): string {
  if (from === "PLANNED" && to === "IN_PROGRESS") return "Start";
  if (from === "IN_PROGRESS" && to === "PENDING") return "Pause";
  if (from === "PENDING" && to === "IN_PROGRESS") return "Resume";
  if (to === "COMPLETED") return "Complete";
  if (to === "CANCELLED") return "Cancel";
  if (to === "PLANNED") return "Reopen";
  return STATUS_LABEL[to];
}

/** The status buttons to offer, in display order. */
export function transitionsFrom(from: TaskStatus): { to: TaskStatus; label: string }[] {
  return MOVES[from].map((to) => ({ to, label: label(from, to) }));
}

export const isFinished = (s: TaskStatus) => s === "COMPLETED" || s === "CANCELLED";
