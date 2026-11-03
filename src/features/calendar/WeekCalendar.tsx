import { useMemo } from "react";
import { Bell, CalendarDays } from "lucide-react";
import { Tile } from "@/features/shell/PageHeader";
import { useAllTasks, useReminders } from "@/hooks/usePluto";
import { parseYmd, ymd } from "@/lib/dates";
import { cn } from "@/lib/utils";

type Kind = "task" | "reminder" | "deadline";
interface Item { id: string; title: string; kind: Kind; at: number; done: boolean; taskId?: string }

const DOT: Record<Kind, string> = { task: "bg-foreground/40", reminder: "bg-primary", deadline: "bg-destructive" };
const MAX_PER_DAY = 3;
const EDGE: Record<Kind, string> = { task: "border-foreground/30", reminder: "border-primary", deadline: "border-destructive" };

/**
 * The next 7 days, starting today: tasks by planned date, deadlines by due date, reminders by time.
 * `large` is the Tasks page tile (owner 2026-10-25): every item, bigger, and tasks open on click.
 */
export function WeekCalendar({ className, large = false, onOpenTask }: { className?: string; large?: boolean; onOpenTask?: (id: string) => void }) {
  const { data: tasks = [] } = useAllTasks();
  const { data: reminders = [] } = useReminders(false);
  const todayKey = ymd(new Date());
  const now = new Date();
  const first = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const days = Array.from({ length: 7 }, (_, i) => new Date(first.getFullYear(), first.getMonth(), first.getDate() + i));

  const byDay = useMemo(() => {
    const map = new Map<string, Item[]>();
    const add = (key: string, item: Item) => map.set(key, [...(map.get(key) ?? []), item]);
    for (const { task } of tasks) {
      if (task.status === "CANCELLED") continue;
      const done = task.status === "COMPLETED";
      if (task.due_date) add(task.due_date, { id: `${task.id}-due`, title: task.title, kind: "deadline", at: parseYmd(task.due_date).getTime(), done, taskId: task.id });
      else if (task.planned_date) add(task.planned_date, { id: task.id, title: task.title, kind: "task", at: parseYmd(task.planned_date).getTime(), done, taskId: task.id });
    }
    for (const r of reminders) add(ymd(new Date(r.remind_at)), { id: r.id, title: r.title, kind: "reminder", at: r.remind_at, done: false });
    for (const list of map.values()) list.sort((a, b) => a.at - b.at);
    return map;
  }, [tasks, reminders]);

  const last = days[6] ?? first;
  const range = `${first.toLocaleDateString([], { month: "short", day: "numeric" })} – ${last.toLocaleDateString([], { month: "short", day: "numeric" })}`;

  return (
    <Tile
      className={className} title="Next 7 days" count={[...byDay.entries()].filter(([k]) => days.some((d) => ymd(d) === k)).reduce((n, [, v]) => n + v.length, 0)}
      action={<span className="flex items-center gap-1.5 text-[11px] text-muted-foreground"><CalendarDays className="size-3.5" />{range}</span>}
      bodyClassName="p-0" footer={
        <ul className="flex flex-wrap gap-x-4 gap-y-1 text-[11px] text-muted-foreground" aria-label="Legend">
          {([["task", "Task"], ["deadline", "Deadline"], ["reminder", "Reminder"]] as const).map(([k, l]) => (
            <li key={k} className="flex items-center gap-1.5"><span className={cn("size-1.5 rounded-full", DOT[k])} />{l}</li>
          ))}
        </ul>
      }
    >
      <div className={cn("grid h-full grid-cols-7 divide-x", large && "min-w-[56rem]")} role="list" aria-label="Next 7 days">
        {days.map((d) => {
          const key = ymd(d);
          const items = byDay.get(key) ?? [];
          const isToday = key === todayKey;
          return (
            <div key={key} role="listitem" aria-label={d.toDateString()} className={cn("relative flex min-w-0 flex-col p-2", large && "min-h-80 p-3", isToday && "bg-primary/[0.06]")}>
              {isToday && <span aria-hidden className="absolute inset-x-0 top-0 h-0.5 bg-primary" />}
              <div className="mb-2 flex items-baseline justify-between px-0.5">
                <span className={cn(large ? "text-xs" : "text-[11px]", isToday ? "font-semibold text-primary" : "text-muted-foreground")}>{isToday && large ? "Today" : d.toLocaleDateString([], { weekday: "short" })}</span>
                <span className={cn("font-semibold tabular-nums", large ? "text-xl" : "text-sm", isToday ? "text-primary" : "text-foreground")}>{d.getDate()}</span>
              </div>
              <ul className={cn(large ? "space-y-1.5" : "space-y-1")}>
                {(large ? items : items.slice(0, MAX_PER_DAY)).map((it) => {
                  const body = (
                    <>
                      {it.kind === "reminder" && !large && <Bell className="mt-px size-2.5 shrink-0 text-primary" />}
                      <span className="min-w-0">
                        {large && it.kind === "reminder" && (
                          <span className="mb-0.5 flex items-center gap-1 text-[11px] tabular-nums text-primary"><Bell className="size-3" />{new Date(it.at).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}</span>
                        )}
                        <span className="line-clamp-2">{it.title}</span>
                      </span>
                    </>
                  );
                  const cls = cn("flex w-full items-start gap-1.5 rounded-md border-l-2 bg-muted/60 text-left", large ? "px-2 py-1.5 text-[12.5px] leading-[17px]" : "px-1.5 py-1 text-[11px] leading-[14px]", EDGE[it.kind], it.done && "opacity-50 line-through");
                  return (
                    <li key={it.id} title={it.kind === "deadline" ? `Deadline: ${it.title}` : it.title}>
                      {large && it.taskId && onOpenTask ? (
                        <button type="button" onClick={() => onOpenTask(it.taskId ?? "")} className={cn(cls, "outline-none hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring")}>{body}</button>
                      ) : <div className={cls}>{body}</div>}
                    </li>
                  );
                })}
                {!large && items.length > MAX_PER_DAY && <li className="px-1 text-[11px] text-muted-foreground">+{items.length - MAX_PER_DAY} more</li>}
                {large && items.length === 0 && <li className="px-1 text-xs text-muted-foreground/70">Free</li>}
              </ul>
            </div>
          );
        })}
      </div>
    </Tile>
  );
}
