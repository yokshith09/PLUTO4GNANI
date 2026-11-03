import { ArrowRight, CheckCircle2 } from "lucide-react";
import { useAllTasks, useDailyLog } from "@/hooks/usePluto";
import { daysAgo, ymd } from "@/lib/dates";
import type { DailyLog, TaskRow } from "@/ipc";
import { cn } from "@/lib/utils";

/** Tasks shown on the Tasks page instead of a view: "what is still open from yesterday". */
export interface TaskFocus { title: string; ids: string[] }

/**
 * Yesterday in numbers (R1-70): how many of the planned tasks got done, and which ones are
 * still not finished now (done or cancelled since then no longer count).
 */
export function yesterdaySummary(log: DailyLog | null | undefined, now: TaskRow[]) {
  if (!log) return null;
  const s = log.snapshot;
  const left = new Set([...s.planned, ...s.in_progress, ...s.pending, ...s.overdue].map((e) => e.id));
  const open = now.filter(({ task }) => left.has(task.id) && task.status !== "COMPLETED" && task.status !== "CANCELLED");
  return { done: s.stats.completed_count, planned: s.stats.planned_count, open: open.map((r) => r.task.id) };
}

/** One line under the greeting; a click opens Tasks on what yesterday left unfinished. */
export function YesterdayLine({ onShow, className }: { onShow: (focus: TaskFocus) => void; className?: string }) {
  const { data: log } = useDailyLog(ymd(daysAgo(1)));
  const { data: all = [] } = useAllTasks();
  const sum = yesterdaySummary(log, all);
  if (!sum || (sum.planned === 0 && sum.done === 0)) return null;
  const head = `Yesterday: ${sum.done} of ${Math.max(sum.planned, sum.done)} done`;
  if (sum.open.length === 0) {
    return <p className={cn("flex items-center gap-1.5 text-[12.5px] text-muted-foreground", className)}><CheckCircle2 className="size-3.5 text-success" />{head} · nothing left over</p>;
  }
  return (
    <button
      type="button" onClick={() => onShow({ title: "Unfinished from yesterday", ids: sum.open })}
      className={cn("flex items-center gap-1.5 rounded text-[12.5px] text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring", className)}
    >
      {head} · <span className="font-medium text-primary">{sum.open.length} still open</span><ArrowRight className="size-3.5" />
    </button>
  );
}
