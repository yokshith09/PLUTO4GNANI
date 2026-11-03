import { useState } from "react";
import { ChevronRight, ClipboardCopy } from "lucide-react";
import { notify } from "@/lib/notify";
import { Tile } from "@/features/shell/PageHeader";
import { useRecentLogs } from "@/hooks/usePluto";
import { dayLabel } from "@/lib/dates";
import { dailyLogMarkdown } from "@/lib/dailyLogMarkdown";
import { cn } from "@/lib/utils";
import type { DailyLog, LogEntry } from "@/ipc";

const DAYS = 14;

function title(date: Date): string {
  const today = new Date();
  const diff = Math.round((new Date(today.getFullYear(), today.getMonth(), today.getDate()).getTime() - date.getTime()) / 86_400_000);
  if (diff === 0) return "Today";
  if (diff === 1) return "Yesterday";
  return date.toLocaleDateString([], { weekday: "short", month: "short", day: "numeric" });
}

function List({ label, entries }: { label: string; entries: LogEntry[] }) {
  if (entries.length === 0) return null;
  return (
    <div>
      <p className="mb-1 text-[11px] font-medium text-muted-foreground">{label} <span className="tabular-nums">{entries.length}</span></p>
      <ul className="space-y-0.5 text-[12px]">
        {entries.slice(0, 6).map((e) => <li key={e.id} className="truncate">{e.title}</li>)}
        {entries.length > 6 && <li className="text-muted-foreground">+{entries.length - 6} more</li>}
      </ul>
    </div>
  );
}

async function copy(log: DailyLog) {
  try {
    await navigator.clipboard.writeText(dailyLogMarkdown(log));
    notify.info("Copied the day as Markdown");
  } catch {
    notify.error("Couldn’t copy. Your system blocked clipboard access.");
  }
}

/** Past days at a glance: done of planned, change from the day before, and the day as Markdown. */
export function DailyLogs({ className }: { className?: string }) {
  const logs = useRecentLogs(DAYS);
  const [open, setOpen] = useState<string | null>(null);
  const rows = logs
    .map((d, i) => ({ ...d, prev: logs[i - 1]?.log ?? null }))
    .filter((d): d is typeof d & { log: DailyLog } => d.log !== null)
    .reverse();

  return (
    <Tile className={className} title="Daily log" count={rows.length} bodyClassName="p-2">
      {rows.length === 0 ? (
        <p className="px-4 py-10 text-center text-[13px] text-muted-foreground">Days with tasks or notes appear here once they have happened.</p>
      ) : (
        <ul>
          {rows.map(({ date, log, prev }) => {
            const s = log.snapshot.stats;
            const key = log.date;
            const isOpen = open === key;
            const delta = prev ? s.completed_count - prev.snapshot.stats.completed_count : null;
            const left = [...log.snapshot.planned, ...log.snapshot.in_progress, ...log.snapshot.pending, ...log.snapshot.overdue];
            return (
              <li key={key} className="rounded-md hover:bg-accent/30">
                <div className="flex items-center gap-1">
                  <button
                    type="button" aria-expanded={isOpen} aria-label={`${dayLabel(date)}: ${s.completed_count} of ${s.planned_count} done`} onClick={() => setOpen(isOpen ? null : key)}
                    className="flex min-w-0 flex-1 items-center gap-3 rounded-md px-2 py-2 text-left outline-none focus-visible:ring-2 focus-visible:ring-ring"
                  >
                    <ChevronRight className={cn("size-3.5 shrink-0 text-muted-foreground transition-transform", isOpen && "rotate-90")} />
                    <span className="w-28 shrink-0 text-[13px]">{title(date)}</span>
                    <span className="w-24 shrink-0 text-[13px] tabular-nums"><span className="font-semibold">{s.completed_count}</span> of {s.planned_count} done</span>
                    <span aria-hidden className="h-1.5 min-w-12 flex-1 overflow-hidden rounded-full bg-muted">
                      <span className="block h-full rounded-full bg-primary" style={{ width: `${Math.round((s.completion_ratio ?? 0) * 100)}%` }} />
                    </span>
                    <span className={cn("w-10 shrink-0 text-right text-[11px] tabular-nums", delta && delta > 0 ? "text-success" : "text-muted-foreground")} title="Compared with the day before">
                      {delta === null || delta === 0 ? "–" : delta > 0 ? `+${delta}` : `−${Math.abs(delta)}`}
                    </span>
                  </button>
                  <button
                    type="button" aria-label={`Copy ${dayLabel(date)} as Markdown`} title="Copy as Markdown" onClick={() => void copy(log)}
                    className="grid size-7 shrink-0 place-items-center rounded text-muted-foreground outline-none hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring [&_svg]:size-3.5"
                  >
                    <ClipboardCopy />
                  </button>
                </div>
                {isOpen && (
                  <div className="grid gap-4 px-9 pb-3 pt-1 sm:grid-cols-2">
                    <List label="Done" entries={log.snapshot.completed} />
                    <List label="Not finished" entries={left} />
                    {log.snapshot.completed.length === 0 && left.length === 0 && <p className="text-xs text-muted-foreground">No tasks on this day.</p>}
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </Tile>
  );
}
