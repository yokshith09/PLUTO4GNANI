/** Local calendar day as `YYYY-MM-DD`. */
export function ymd(d: Date): string {
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${d.getFullYear()}-${m}-${day}`;
}

export function daysAgo(n: number, from = new Date()): Date {
  const d = new Date(from.getFullYear(), from.getMonth(), from.getDate());
  d.setDate(d.getDate() - n);
  return d;
}

export function parseYmd(s: string): Date {
  const [y = 0, m = 1, d = 1] = s.split("-").map(Number);
  return new Date(y, m - 1, d);
}

export const weekdayShort = (d: Date) => d.toLocaleDateString([], { weekday: "short" });
export const weekdayNarrow = (d: Date) => d.toLocaleDateString([], { weekday: "narrow" });
export const dayLabel = (d: Date) => d.toLocaleDateString([], { weekday: "long", month: "long", day: "numeric" });

/** Whole days from `from` to `to` (both YYYY-MM-DD); negative when `to` is earlier. */
export function daysBetween(from: string, to: string): number {
  return Math.round((parseYmd(to).getTime() - parseYmd(from).getTime()) / 86_400_000);
}

/** "1 day overdue", "3 days overdue" for a deadline before `today`. */
export function overdueLabel(due: string, today: string): string {
  const n = Math.max(1, daysBetween(due, today));
  if (n > 365) return "Over a year overdue";
  return `${n} ${n === 1 ? "day" : "days"} overdue`;
}
