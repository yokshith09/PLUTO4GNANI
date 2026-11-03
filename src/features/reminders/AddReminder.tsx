import { useState } from "react";
import { Presets } from "@/components/Presets";
import { timePresets } from "@/lib/datePresets";
import { BellPlus } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { fromLocalInput, toLocalInput } from "@/lib/time";
import { useReminderActions } from "@/hooks/usePluto";

function atHour(daysAhead: number, hour: number): number {
  const d = new Date();
  d.setDate(d.getDate() + daysAhead);
  d.setHours(hour, 0, 0, 0);
  return d.getTime();
}

export function AddReminder() {
  const { create } = useReminderActions();
  const [open, setOpen] = useState(false);
  const [title, setTitle] = useState("");
  const [when, setWhen] = useState(() => toLocalInput(Date.now() + 3_600_000));

  const presets = [
    { label: "In 1 hour", at: () => Date.now() + 3_600_000 },
    { label: "This evening", at: () => atHour(Date.now() > atHour(0, 18) ? 1 : 0, 18) },
    { label: "Tomorrow 9 AM", at: () => atHour(1, 9) },
  ];
  const submit = async () => {
    if (!title.trim()) return;
    await create(title.trim(), fromLocalInput(when));
    setTitle("");
    setOpen(false);
  };

  return (
    <Popover open={open} onOpenChange={(o) => { setOpen(o); if (o) setWhen(toLocalInput(Date.now() + 3_600_000)); }}>
      <PopoverTrigger asChild>
        <Button type="button" variant="secondary" size="icon" aria-label="Add reminder" title="Add reminder"><BellPlus /></Button>
      </PopoverTrigger>
      <PopoverContent align="end" side="top" className="w-72">
        <form className="space-y-3" onSubmit={(e) => { e.preventDefault(); void submit(); }}>
          <div className="space-y-1.5">
            <label htmlFor="rem-title" className="text-xs font-medium text-muted-foreground">Remind me to</label>
            <Input id="rem-title" autoFocus value={title} maxLength={200} onChange={(e) => setTitle(e.target.value)} placeholder="Call the dentist" />
          </div>
          <div className="space-y-1.5">
            <label htmlFor="rem-when" className="text-xs font-medium text-muted-foreground">When</label>
            <Input id="rem-when" type="datetime-local" value={when} onChange={(e) => setWhen(e.target.value)} />
            <Presets label="When presets" options={timePresets()} value={when} onPick={setWhen} />
            <div className="flex flex-wrap gap-1.5 pt-0.5">
              {presets.map((p) => (
                <button key={p.label} type="button" onClick={() => setWhen(toLocalInput(p.at()))}
                  className="rounded-md border px-2 py-0.5 text-[11px] text-muted-foreground outline-none hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring">
                  {p.label}
                </button>
              ))}
            </div>
          </div>
          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" size="sm" onClick={() => setOpen(false)}>Cancel</Button>
            <Button type="submit" size="sm" disabled={!title.trim() || !when}>Set reminder</Button>
          </div>
        </form>
      </PopoverContent>
    </Popover>
  );
}
