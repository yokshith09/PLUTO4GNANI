import type { FocusSession } from "@/ipc";

/** Time counted so far, never more than planned. Mirrors `focus.rs`. */
export function workedMs(s: FocusSession, now: number): number {
  const until = s.ended_at ?? s.paused_at ?? now;
  return Math.min(Math.max(until - s.started_at - s.paused_ms, 0), s.planned_ms);
}

export const remainingMs = (s: FocusSession, now: number) => s.planned_ms - workedMs(s, now);

/** "13:12", or "1:05:00" past an hour. Rounds up so it reads 0:00 only at the very end. */
export function clock(ms: number): string {
  const total = Math.ceil(Math.max(ms, 0) / 1000);
  const h = Math.floor(total / 3600), m = Math.floor((total % 3600) / 60), sec = total % 60;
  const ss = String(sec).padStart(2, "0");
  return h ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

export const minutesOf = (ms: number) => Math.round(ms / 60_000);

/** What the pet says when a session ends (FR-110). A finished focus offers a break. */
export function endLine(s: FocusSession): { text: string; pose: string; offerBreak: boolean } | null {
  if (s.outcome !== "DONE") return null;
  if (s.kind === "BREAK") return { text: "Break's over. Ready when you are.", pose: "wave", offerBreak: false };
  const what = s.label ? ` on ${s.label}` : "";
  return { text: `Nice: ${minutesOf(s.planned_ms)} min${what}. Break 5 min?`, pose: "cheer", offerBreak: true };
}

export const BREAK_MINUTES = 5;
export const FOCUS_CHOICES = [15, 25, 50] as const;

export function hoursLabel(ms: number): string {
  const min = Math.round(ms / 60_000);
  return min < 60 ? `${min} min` : `${Math.floor(min / 60)} h ${String(min % 60).padStart(2, "0")} min`;
}
