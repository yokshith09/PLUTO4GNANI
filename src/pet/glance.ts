import type { FocusSession, Reminder, TaskRow } from "@/ipc";
import { clock } from "@/lib/focus";
import { topTasks } from "./PetCard";

const short = (s: string, n = 28) => (s.length > n ? `${s.slice(0, n - 1).trimEnd()}…` : s);

/**
 * Today at a glance in one bubble, for when the hover card is turned off (FR-110):
 * focus time left first, then the task that matters most, then the next reminder.
 */
export function glanceText(o: { session: FocusSession | null; remaining: number; tasks: TaskRow[]; next?: Reminder | undefined; now: number }): string {
  const parts: string[] = [];
  if (o.session) parts.push(`${o.session.kind === "BREAK" ? "Break" : "Focus"} ${clock(o.remaining)} left`);
  const top = topTasks(o.tasks, 1)[0];
  if (top) parts.push(`${o.tasks.length} today · next: ${short(top.task.title)}`);
  if (o.next) {
    const at = new Date(o.next.remind_at).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
    parts.push(`${short(o.next.title, 20)} at ${at}`);
  }
  return parts.length ? parts.join(" · ") : "Nothing planned. Rest counts too.";
}
