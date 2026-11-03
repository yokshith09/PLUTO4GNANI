import type { Repeat, Task } from "@/ipc";
import { parseYmd, ymd } from "./dates";

/** ISO weekday of a `YYYY-MM-DD` day: Monday = 1 … Sunday = 7. Mirrors `task_plans.rs`. */
export function isoWeekday(day: string): number {
  const w = parseYmd(day).getDay();
  return w === 0 ? 7 : w;
}

const dayNumber = (day: string) => Math.round(parseYmd(day).getTime() / 86_400_000);

/** Does a rule starting on `start` (and ending on `until`) repeat on `day`? */
export function occursOn(rule: Repeat, start: string, until: string | null, day: string): boolean {
  if (day < start || (until !== null && day > until)) return false;
  switch (rule.kind) {
    case "daily": return true;
    case "weekly": return rule.days.includes(isoWeekday(day));
    case "every": return (dayNumber(day) - dayNumber(start)) % rule.n === 0;
  }
}

/** Does this task repeat on `day`? False when it doesn't repeat. */
export function taskOccurs(task: Pick<Task, "repeat" | "planned_date" | "repeat_until">, day: string): boolean {
  return !!task.repeat && !!task.planned_date && occursOn(task.repeat, task.planned_date, task.repeat_until, day);
}

/** Repeat days from `from` to `to`, oldest first. */
export function repeatDays(rule: Repeat, start: string, until: string | null, from: string, to: string): string[] {
  const out: string[] = [];
  for (let d = parseYmd(from); ymd(d) <= to; d.setDate(d.getDate() + 1)) {
    const day = ymd(d);
    if (occursOn(rule, start, until, day)) out.push(day);
  }
  return out;
}

const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/** "Every day", "Mon, Wed, Fri", "Weekdays", "Every 3 days". */
export function repeatLabel(rule: Repeat): string {
  switch (rule.kind) {
    case "daily": return "Every day";
    case "every": return rule.n === 1 ? "Every day" : `Every ${rule.n} days`;
    case "weekly": {
      const key = rule.days.join(",");
      if (key === "1,2,3,4,5") return "Weekdays";
      if (key === "6,7") return "Weekends";
      if (key === "1,2,3,4,5,6,7") return "Every day";
      return rule.days.map((d) => WEEKDAYS[d - 1]).join(", ");
    }
  }
}

export { WEEKDAYS };
