import type { TaskRow } from "@/ipc";

/** Settings and memory for the evening "still open" reminder (FR-109), in prefs `ui.repeatRemind`. */
export type RepeatRemind = { on: boolean; at: string; remindedOn: string | null };
export const REPEAT_REMIND_DEFAULT: RepeatRemind = { on: true, at: "19:00", remindedOn: null };

export function parseRepeatRemind(raw: unknown): RepeatRemind {
  const r = (raw && typeof raw === "object" ? raw : {}) as Partial<RepeatRemind>;
  return {
    on: typeof r.on === "boolean" ? r.on : REPEAT_REMIND_DEFAULT.on,
    at: typeof r.at === "string" && /^\d{2}:\d{2}$/.test(r.at) ? r.at : REPEAT_REMIND_DEFAULT.at,
    remindedOn: typeof r.remindedOn === "string" ? r.remindedOn : null,
  };
}

/** Time to say it: switched on, past the time, not yet said today. */
export function dueNow(s: RepeatRemind, now: Date, today: string): boolean {
  if (!s.on || s.remindedOn === today) return false;
  const [h = 19, m = 0] = s.at.split(":").map(Number);
  return now.getHours() * 60 + now.getMinutes() >= h * 60 + m;
}

/**
 * "Still open today: Walk, Revise — tick them when done." for today's repeating tasks that aren't
 * ticked yet (Today lists only those), or null when there are none. Kind words only (FR-92).
 */
export function openRepeatsMessage(today: readonly TaskRow[]): string | null {
  const names = today.filter((r) => r.task.repeat && !r.done_today).map((r) => r.task.title);
  if (names.length === 0) return null;
  const shown = names.slice(0, 3).join(", ") + (names.length > 3 ? ` +${names.length - 3} more` : "");
  return `Still open today: ${shown} — tick ${names.length === 1 ? "it" : "them"} when done.`;
}
