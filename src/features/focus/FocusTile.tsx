import { useState } from "react";
import { Minus, Pause, Play, Plus, Square } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Tile } from "@/features/shell/PageHeader";
import { FOCUS_MAX, FOCUS_MIN, useFocus, useFocusLength } from "@/hooks/useFocus";
import { useTodayTasks } from "@/hooks/usePluto";
import { FOCUS_CHOICES, clock, workedMs } from "@/lib/focus";
import { cn } from "@/lib/utils";

/** − 25 min +, in steps of 5 (FR-05). Shared with the task editor. */
export function FocusLength({ minutes, onChange, className }: { minutes: number; onChange: (m: number) => void; className?: string }) {
  return (
    <div className={cn("inline-flex h-8 items-center overflow-hidden rounded-md border", className)} role="group" aria-label="Focus length">
      <button type="button" aria-label="5 minutes shorter" disabled={minutes <= FOCUS_MIN} onClick={() => onChange(minutes - 5)}
        className="grid h-full w-8 place-items-center text-muted-foreground outline-none hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-40"><Minus className="size-3.5" /></button>
      <span className="min-w-16 text-center text-[13px] font-medium tabular-nums" aria-live="polite">{minutes} min</span>
      <button type="button" aria-label="5 minutes longer" disabled={minutes >= FOCUS_MAX} onClick={() => onChange(minutes + 5)}
        className="grid h-full w-8 place-items-center text-muted-foreground outline-none hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-40"><Plus className="size-3.5" /></button>
    </div>
  );
}

/** The Focus tile on Today (owner 2026-10-25: its own tile, not inside the Today list). */
export function FocusTile({ className }: { className?: string }) {
  const { session, remaining, start, pause, extend, stop } = useFocus();
  const { minutes, set } = useFocusLength();
  const { data: rows = [] } = useTodayTasks();
  const [taskId, setTaskId] = useState("");
  const open = rows.filter((r) => r.task.status !== "COMPLETED" && !r.task.repeat);

  if (session) {
    const paused = session.paused_at !== null;
    const done = workedMs(session, Date.now());
    const pct = Math.min(100, Math.round((done / session.planned_ms) * 100));
    const what = session.kind === "BREAK" ? "Break" : session.label || "Focus";
    return (
      <Tile title="Focus" className={className} bodyClassName="flex flex-col justify-center gap-4 p-5">
        <div id="focus-tile" className="flex items-baseline justify-between gap-3">
          <span className="text-4xl font-semibold tabular-nums tracking-tight" role="timer" aria-label="Time left">{clock(remaining)}</span>
          <span className="min-w-0 truncate text-[13px] text-muted-foreground">{paused ? "Paused · " : ""}{what}</span>
        </div>
        <div className="h-1.5 overflow-hidden rounded-full bg-muted" aria-hidden>
          <div className={cn("h-full rounded-full", session.kind === "BREAK" ? "bg-success" : "bg-primary")} style={{ width: `${pct}%` }} />
        </div>
        <div className="flex gap-2">
          <Button size="sm" variant="secondary" onClick={() => void pause(!paused)}>{paused ? <Play /> : <Pause />}{paused ? "Resume" : "Pause"}</Button>
          <Button size="sm" variant="outline" onClick={() => void extend(5)}><Plus />5 min</Button>
          <Button size="sm" variant="ghost" className="ml-auto" onClick={() => void stop()}><Square />Stop</Button>
        </div>
      </Tile>
    );
  }

  return (
    <Tile title="Focus" className={className} bodyClassName="flex flex-col justify-center gap-4 p-5">
      <div id="focus-tile" className="flex items-center justify-between gap-3">
        <span className="text-4xl font-semibold tabular-nums tracking-tight">{minutes}:00</span>
        <FocusLength minutes={minutes} onChange={(m) => void set(m)} />
      </div>
      <div className="flex gap-1.5" role="group" aria-label="Quick lengths">
        {FOCUS_CHOICES.map((m) => (
          <button key={m} type="button" aria-pressed={minutes === m} onClick={() => void set(m)}
            className={cn("h-7 rounded-md border px-2.5 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring",
              minutes === m ? "border-primary/60 bg-primary/10 text-foreground" : "text-muted-foreground hover:bg-accent hover:text-foreground")}>
            {m} min
          </button>
        ))}
      </div>
      <div className="flex gap-2">
        <select aria-label="Focus on" value={taskId} onChange={(e) => setTaskId(e.target.value)}
          className="h-9 min-w-0 flex-1 rounded-md border border-input bg-card px-2 text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-ring">
          <option value="">No task</option>
          {open.map((r) => <option key={r.task.id} value={r.task.id}>{r.task.title}</option>)}
        </select>
        <Button onClick={() => void start("FOCUS", minutes, taskId || null)}><Play />Start</Button>
      </div>
    </Tile>
  );
}
