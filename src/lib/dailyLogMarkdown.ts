import type { DailyLog, LogEntry } from "@/ipc";
import { parseYmd } from "./dates";

const PRIORITY: Record<string, string> = { HIGH: "High", MEDIUM: "Medium", LOW: "Low" };

const line = (e: LogEntry, box: " " | "x") => `- [${box}] ${e.title}${e.priority ? ` (${PRIORITY[e.priority] ?? e.priority})` : ""}`;

/** One day's log as Markdown (R1-45). Empty sections are left out. */
export function dailyLogMarkdown(log: DailyLog): string {
  const { snapshot: s } = log;
  const date = parseYmd(s.date).toLocaleDateString("en-US", { weekday: "long", year: "numeric", month: "long", day: "numeric" });
  const pct = s.stats.completion_ratio === null ? "" : ` (${Math.round(s.stats.completion_ratio * 100)}%)`;
  const out = [
    `# Daily log — ${date}`,
    "",
    `Completed ${s.stats.completed_count} of ${s.stats.planned_count}${pct} · Tasks created ${s.stats.tasks_created} · Notes created ${s.stats.notes_created}, edited ${s.stats.notes_edited} · Meetings ${s.stats.meetings}`,
    ...(log.live ? ["", "_Today's log is still open and may change._"] : []),
  ];
  const section = (title: string, entries: LogEntry[], box: " " | "x") => {
    if (entries.length) out.push("", `## ${title}`, ...entries.map((e) => line(e, box)));
  };
  section("Completed", s.completed, "x");
  section("In progress", s.in_progress, " ");
  section("Planned", s.planned, " ");
  section("Pending", s.pending, " ");
  section("Overdue", s.overdue, " ");
  section("Cancelled", s.cancelled, " ");
  return `${out.join("\n")}\n`;
}
