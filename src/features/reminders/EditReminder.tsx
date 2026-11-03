import { useState, type ReactNode } from "react";
import { Presets } from "@/components/Presets";
import { timePresets } from "@/lib/datePresets";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { useReminderActions } from "@/hooks/usePluto";
import { fromLocalInput, toLocalInput } from "@/lib/time";
import type { Reminder } from "@/ipc";

const MIN = 60_000;

function tomorrowAt(hour: number): number {
  const d = new Date();
  d.setDate(d.getDate() + 1);
  d.setHours(hour, 0, 0, 0);
  return d.getTime();
}

/** Change a reminder's text or time, missed or not. A new time makes it ring again. */
export function EditReminder({ reminder, children }: { reminder: Reminder; children: ReactNode }) {
  const { update } = useReminderActions();
  const [open, setOpen] = useState(false);
  const [title, setTitle] = useState(reminder.title);
  const [when, setWhen] = useState(() => toLocalInput(reminder.remind_at));
  const presets = [
    { label: "In 10 min", at: () => Date.now() + 10 * MIN },
    { label: "In 1 hour", at: () => Date.now() + 60 * MIN },
    { label: "Tomorrow 9 AM", at: () => tomorrowAt(9) },
  ];
  const save = async (at = fromLocalInput(when)) => {
    if (!title.trim() || !at) return;
    await update(reminder.id, title.trim(), at);
    setOpen(false);
  };

  return (
    <Popover
      open={open}
      onOpenChange={(o) => {
        setOpen(o);
        if (o) { setTitle(reminder.title); setWhen(toLocalInput(reminder.remind_at)); }
      }}
    >
      <PopoverTrigger asChild>{children}</PopoverTrigger>
      <PopoverContent align="start" className="w-72">
        <form className="space-y-3" onSubmit={(e) => { e.preventDefault(); void save(); }}>
          <div className="space-y-1.5">
            <label htmlFor={`rem-edit-title-${reminder.id}`} className="text-xs font-medium text-muted-foreground">Remind me to</label>
            <Input id={`rem-edit-title-${reminder.id}`} autoFocus value={title} maxLength={200} onChange={(e) => setTitle(e.target.value)} />
          </div>
          <div className="space-y-1.5">
            <label htmlFor={`rem-edit-when-${reminder.id}`} className="text-xs font-medium text-muted-foreground">When</label>
            <Input id={`rem-edit-when-${reminder.id}`} type="datetime-local" value={when} onChange={(e) => setWhen(e.target.value)} />
            <Presets label="When presets" options={timePresets()} value={when} onPick={setWhen} />
            <div className="flex flex-wrap gap-1.5 pt-0.5" role="group" aria-label="Snooze">
              {presets.map((p) => (
                <button
                  key={p.label} type="button" onClick={() => void save(p.at())}
                  className="rounded-md border px-2 py-0.5 text-[11px] text-muted-foreground outline-none hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
                >
                  {p.label}
                </button>
              ))}
            </div>
          </div>
          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" size="sm" onClick={() => setOpen(false)}>Cancel</Button>
            <Button type="submit" size="sm" disabled={!title.trim() || !when}>Save</Button>
          </div>
        </form>
      </PopoverContent>
    </Popover>
  );
}
