import { useState } from "react";
import { ConfirmAction } from "@/components/ConfirmAction";
import { Bell, Minus, Plus, Volume2, VolumeX, X } from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { useReminders } from "@/hooks/usePluto";
import { errorMessage, ipc, type Task } from "@/ipc";
import { notify } from "@/lib/notify";
import { fromLocalInput, toLocalInput } from "@/lib/time";

/** How early, in minutes. "+" walks up this list (owner 2026-10-25: 5 min before, more with +). */
export const LEADS = [0, 5, 10, 15, 20, 30, 45, 60, 90, 120, 180, 240, 1440] as const;
export const nextLead = (m: number, dir: 1 | -1) => {
  const i = LEADS.findIndex((x) => x >= m);
  const at = i < 0 ? LEADS.length - 1 : LEADS[i] === m ? i : dir > 0 ? i - 1 : i;
  return LEADS[Math.min(LEADS.length - 1, Math.max(0, at + dir))] ?? m;
};
export function leadLabel(m: number): string {
  if (m === 0) return "At the time";
  if (m === 1440) return "1 day before";
  if (m >= 60 && m % 60 === 0) return `${m / 60} h before`;
  if (m > 60) return `${Math.floor(m / 60)} h ${m % 60} min before`;
  return `${m} min before`;
}

const whenText = (ms: number) => new Date(ms).toLocaleString([], { weekday: "short", day: "numeric", month: "short", hour: "numeric", minute: "2-digit" });

/** A start time for a new reminder: the task's day at 9:00, or the next round hour today. */
function defaultStart(task: Task): string {
  const now = new Date();
  if (task.planned_date && task.planned_date > toLocalInput(now.getTime()).slice(0, 10)) return `${task.planned_date}T09:00`;
  const d = new Date(now);
  d.setHours(d.getHours() + 1, 0, 0, 0);
  return toLocalInput(d.getTime());
}

/** Reminders for this task: when it starts, how long before to remind, with or without sound. */
export function TaskReminders({ task }: { task: Task }) {
  const qc = useQueryClient();
  const { data: all = [] } = useReminders(false);
  const mine = all.filter((r) => r.task_id === task.id).sort((a, b) => a.remind_at - b.remind_at);
  const [start, setStart] = useState(() => defaultStart(task));
  const [lead, setLead] = useState(5);
  const [sound, setSound] = useState(true);
  const refresh = () => qc.invalidateQueries({ queryKey: ["reminders"] });

  const add = async () => {
    const at = fromLocalInput(start) - lead * 60_000;
    if (!Number.isFinite(at)) return notify.warning("Pick a date and time.");
    if (at <= Date.now()) return notify.warning("That time has already passed.");
    try {
      await ipc.reminderCreate({ title: task.title, remind_at: at, task_id: task.id, lead_min: lead, sound });
      await refresh();
      notify.success(`Reminder set: ${whenText(at)}`);
    } catch (e) {
      notify.error(errorMessage(e));
    }
  };
  const run = (p: Promise<unknown>) => void p.then(refresh).catch((e: unknown) => notify.error(errorMessage(e)));

  return (
    <div className="space-y-3">
      {mine.length > 0 && (
        <ul className="space-y-1.5" aria-label="Reminders for this task">
          {mine.map((r) => (
            <li key={r.id} className="flex items-center gap-2 rounded-md bg-muted/40 px-2.5 py-1.5 text-[13px]">
              <Bell className="size-3.5 shrink-0 text-primary" />
              <span className="min-w-0 flex-1 truncate">{whenText(r.remind_at)}{r.lead_min ? <span className="text-muted-foreground"> · {leadLabel(r.lead_min)}</span> : null}</span>
              <button type="button" aria-label={r.sound ? "Sound on. Turn it off" : "Sound off. Turn it on"} aria-pressed={r.sound}
                onClick={() => run(ipc.reminderUpdate(r.id, { sound: !r.sound }))}
                className="grid size-6 shrink-0 place-items-center rounded text-muted-foreground outline-none hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring">
                {r.sound ? <Volume2 className="size-3.5" /> : <VolumeX className="size-3.5" />}
              </button>
              <ConfirmAction message="Remove this reminder?" verb="Remove" onConfirm={() => run(ipc.reminderDelete(r.id))}>
                <button type="button" aria-label="Remove reminder"
                  className="grid size-6 shrink-0 place-items-center rounded text-muted-foreground outline-none hover:bg-destructive/15 hover:text-destructive focus-visible:ring-2 focus-visible:ring-ring">
                  <X className="size-3.5" />
                </button>
              </ConfirmAction>
            </li>
          ))}
        </ul>
      )}
      <div className="space-y-2">
        <label className="block space-y-1 text-xs text-muted-foreground">It starts at
          <Input type="datetime-local" value={start} onChange={(e) => setStart(e.target.value)} aria-label="It starts at" />
        </label>
        <div className="flex flex-wrap items-center gap-3">
          <div className="inline-flex h-8 items-center overflow-hidden rounded-md border" role="group" aria-label="How early">
            <button type="button" aria-label="Remind later" disabled={lead === 0} onClick={() => setLead(nextLead(lead, -1))}
              className="grid h-full w-8 place-items-center text-muted-foreground outline-none hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-40"><Minus className="size-3.5" /></button>
            <span className="min-w-28 px-1 text-center text-[13px] font-medium" aria-live="polite">{leadLabel(lead)}</span>
            <button type="button" aria-label="Remind earlier" disabled={lead === 1440} onClick={() => setLead(nextLead(lead, 1))}
              className="grid h-full w-8 place-items-center text-muted-foreground outline-none hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-40"><Plus className="size-3.5" /></button>
          </div>
          <label className="flex items-center gap-2 text-[13px]">
            <Switch checked={sound} onCheckedChange={setSound} aria-label="Play a sound" />Sound
          </label>
        </div>
        <Button type="button" size="sm" variant="secondary" onClick={() => void add()}><Bell />Add reminder</Button>
      </div>
    </div>
  );
}
