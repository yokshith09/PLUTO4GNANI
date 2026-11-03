import { useState } from "react";
import { Bell, CheckCircle2, Circle, Moon, NotebookPen, Pause, Play, Plus, Square, Star, Sun, Timer } from "lucide-react";
import { ipc } from "@/ipc";
import { PRIORITY } from "@/features/overview/TodayAgenda";
import { rangeBounds } from "@/features/time/range";
import { useDailyLog, useReminders, useTodayTasks, useTrackingStatus, useUsage } from "@/hooks/usePluto";
import { ymd } from "@/lib/dates";
import { clockTime, formatDuration } from "@/lib/time";
import { cn } from "@/lib/utils";
import type { Priority, TaskRow } from "@/ipc";
import { useFocus } from "@/hooks/useFocus";
import { clock } from "@/lib/focus";

const RANK: Record<Priority, number> = { HIGH: 0, MEDIUM: 1, LOW: 2 };

/** The two tasks that matter most right now: overdue first, then in progress, then by priority. */
export function topTasks(rows: TaskRow[], n = 2): TaskRow[] {
  const score = ({ task, overdue }: TaskRow) =>
    (overdue ? 0 : 10) + (task.status === "IN_PROGRESS" ? 0 : 5) + (task.priority ? RANK[task.priority] : 3);
  return [...rows].sort((a, b) => score(a) - score(b)).slice(0, n);
}

/** What the pet shows on hover: today's time, top apps, the two tasks that matter, and two lines of context. */
export function PetCard({ className, style, napping = false, onNap }: { className?: string; style?: React.CSSProperties; napping?: boolean; onNap?: (nap: boolean) => void }) {
  const today = rangeBounds("today");
  const { data: status } = useTrackingStatus();
  const { data: usage } = useUsage(today.from, today.to);
  const { data: tasks = [] } = useTodayTasks();
  const { data: reminders = [] } = useReminders(false);
  const { data: log } = useDailyLog(ymd(new Date()));
  const tracking = status?.enabled ?? false;
  const focus = useFocus();
  // The focus controls take a row; one app fewer keeps the card the same height.
  const apps = (usage?.apps ?? []).slice(0, focus.session ? 2 : 3);
  const top = topTasks(tasks);
  const next = reminders.filter((r) => r.remind_at >= Date.now()).sort((a, b) => a.remind_at - b.remind_at)[0];
  const done = log?.snapshot.stats.completed_count ?? 0;
  const max = apps[0]?.total_seconds ?? 1;

  return (
    <div className={cn("flex flex-col gap-3 overflow-hidden rounded-xl border bg-popover p-3.5 text-popover-foreground shadow-lg", className)} style={style} role="status" aria-label="Today at a glance">
      <div className="flex items-baseline justify-between">
        {focus.session ? (
          <span className="truncate text-[11px] font-medium uppercase tracking-wide text-primary" aria-label="Focus time left">
            {focus.session.kind === "BREAK" ? "Break" : "Focus"} · {clock(focus.remaining)} left{focus.session.label ? ` · ${focus.session.label}` : ""}
          </span>
        ) : (
          <span className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">Today</span>
        )}
        <span className="text-[11px] text-muted-foreground">{new Date().toLocaleDateString([], { weekday: "short", month: "short", day: "numeric" })}</span>
      </div>
      {focus.session && (
        <div className="flex items-center gap-1" role="group" aria-label="Focus controls">
          {focus.session.paused_at === null
            ? <MiniButton label="Pause focus" onClick={() => void focus.pause(true)}><Pause /></MiniButton>
            : <MiniButton label="Resume focus" onClick={() => void focus.pause(false)}><Play /></MiniButton>}
          <MiniButton label="Add 5 minutes" onClick={() => void focus.extend(5)}>+5</MiniButton>
          <MiniButton label="Stop focus" onClick={() => void focus.stop()}><Square /></MiniButton>
          <button type="button" onClick={() => void ipc.petCommand("focus")} onPointerUp={(e) => e.stopPropagation()}
            className="ml-auto rounded-md px-1.5 text-[11px] text-muted-foreground underline-offset-2 outline-none hover:text-foreground hover:underline focus-visible:ring-2 focus-visible:ring-ring">
            Edit
          </button>
        </div>
      )}
      <div className="flex items-end gap-3">
        <p className="text-[28px] font-semibold leading-none tabular-nums">{tracking ? formatDuration(usage?.total_seconds ?? 0) : "—"}</p>
        <p className="pb-0.5 text-[11px] text-muted-foreground">{tracking ? `on screen · ${usage?.apps.length ?? 0} apps` : "Turn on app tracking in Settings"}</p>
      </div>
      {apps.length > 0 && (
        <ul className="space-y-1">
          {apps.map((a) => (
            <li key={a.app} className="flex items-center gap-2 text-[12px]">
              <span className="w-24 truncate">{a.app}</span>
              <span className="h-1 flex-1 overflow-hidden rounded-full bg-muted">
                <span className="block h-full rounded-full bg-primary" style={{ width: `${Math.max(4, (a.total_seconds / max) * 100)}%` }} />
              </span>
              <span className="w-12 text-right tabular-nums text-muted-foreground">{formatDuration(a.total_seconds)}</span>
            </li>
          ))}
        </ul>
      )}
      <div className="border-t pt-2.5">
        <QuickAdd />
        {top.length === 0 ? (
          <p className="text-[12px] text-muted-foreground">No tasks for today.</p>
        ) : (
          <ul className="space-y-1">
            {top.map(({ task, overdue }) => (
              // Tasks stand out on the card (owner 2026-10-23): a tinted row, starred ones marked.
              <li key={task.id} className={cn("flex items-center gap-1 rounded-md pl-1", overdue ? "bg-destructive/10" : "bg-primary/10")}>
                {/* The circle completes the task right here; the rest of the row opens it (owner 2026-10-27). */}
                <button type="button" onClick={() => void ipc.taskTransition(task.id, "COMPLETED")} onPointerUp={(e) => e.stopPropagation()} aria-label={`Complete task: ${task.title}`}
                  className="group grid size-6 shrink-0 place-items-center rounded-full outline-none focus-visible:ring-2 focus-visible:ring-ring">
                  <Circle className="size-3.5 text-primary group-hover:hidden" />
                  <CheckCircle2 className="hidden size-3.5 text-primary group-hover:block" />
                </button>
                <button type="button" onClick={() => void ipc.petCommand("tasks")} onPointerUp={(e) => e.stopPropagation()} aria-label={`Open task: ${task.title}`}
                  className="flex min-w-0 flex-1 items-center gap-2 rounded-md py-1 pr-2 text-left text-[12.5px] font-medium outline-none hover:brightness-95 focus-visible:ring-2 focus-visible:ring-ring">
                  {task.starred && <Star className="size-3 shrink-0 fill-current text-primary" />}
                  <span className="min-w-0 flex-1 truncate">{task.title}</span>
                  {overdue ? <span className="text-[11px] text-destructive">Overdue</span> : task.priority && (
                    <span className={cn("text-[11px]", PRIORITY[task.priority].cls)}>{PRIORITY[task.priority].label}</span>
                  )}
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
      <div className="space-y-0.5 text-[11px] text-muted-foreground">
        <p className="flex items-center gap-1.5 truncate"><Bell className="size-3" />{next ? `Next: ${next.title} · ${clockTime(next.remind_at)}` : "No reminders coming up"}</p>
        <p className="flex items-center gap-1.5"><CheckCircle2 className="size-3" />{done} done today · {tasks.length} still open</p>
      </div>
      <div className="mt-auto grid grid-cols-3 gap-1.5 border-t pt-2.5" role="group" aria-label="Shortcuts">
        <CardButton icon={<Timer />} label="Focus" onClick={() => void ipc.petCommand("focus")} />
        <CardButton icon={<NotebookPen />} label="Add note" onClick={() => void ipc.petCommand("new-note")} />
        {napping ? (
          <CardButton icon={<Sun />} label="Wake" onClick={() => onNap?.(false)} />
        ) : (
          <CardButton icon={<Moon />} label="Sleep" onClick={() => onNap?.(true)} />
        )}
      </div>
    </div>
  );
}

function CardButton({ icon, label, onClick }: { icon: React.ReactNode; label: string; onClick: () => void }) {
  return (
    <button type="button" onClick={onClick} onPointerUp={(e) => e.stopPropagation()}
      className="flex h-8 items-center justify-center gap-1.5 rounded-md border bg-card text-[11.5px] font-medium outline-none hover:border-primary/50 hover:text-primary focus-visible:ring-2 focus-visible:ring-ring [&_svg]:size-3.5">
      {icon}{label}
    </button>
  );
}

function MiniButton({ label, onClick, children }: { label: string; onClick: () => void; children: React.ReactNode }) {
  return (
    <button type="button" aria-label={label} title={label} onClick={onClick} onPointerUp={(e) => e.stopPropagation()}
      className="grid h-6 min-w-6 place-items-center rounded-md border bg-card px-1 text-[11px] font-semibold outline-none hover:border-primary/50 hover:text-primary focus-visible:ring-2 focus-visible:ring-ring [&_svg]:size-3">
      {children}
    </button>
  );
}

/** "+" beside Tasks: one line, Enter adds it for today, Escape closes. No editor (owner 2026-10-27). */
function QuickAdd() {
  const [open, setOpen] = useState(false);
  const [title, setTitle] = useState("");
  const [busy, setBusy] = useState(false);
  const close = () => { setOpen(false); setTitle(""); };
  const add = async () => {
    const t = title.trim();
    if (!t || busy) return;
    setBusy(true);
    try {
      await ipc.taskQuickAdd(t);
      close();
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="mb-1.5 flex h-6 items-center gap-1.5">
      {open ? (
        <input
          autoFocus aria-label="New task for today" placeholder="New task for today…" value={title} maxLength={200} disabled={busy}
          onChange={(e) => setTitle(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter") void add(); else if (e.key === "Escape") close(); }}
          onBlur={() => !title.trim() && close()}
          onPointerUp={(e) => e.stopPropagation()}
          className="h-6 min-w-0 flex-1 rounded-md border bg-card px-2 text-[12px] outline-none focus-visible:ring-2 focus-visible:ring-ring"
        />
      ) : (
        <>
          <span className="flex-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">Tasks</span>
          <MiniButton label="Add a task" onClick={() => setOpen(true)}><Plus /></MiniButton>
        </>
      )}
    </div>
  );
}
