import { useEffect, useId, useState } from "react";
import { CalendarClock, ListChecks, Plus, Users, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { PageHeader } from "@/features/shell/PageHeader";
import { useMeetings, useParticipantSuggestions } from "@/hooks/usePluto";
import { mod } from "@/lib/platform";
import { cn } from "@/lib/utils";
import type { MeetingFilters, MeetingSummary } from "@/ipc";
import { MeetingEditor } from "./MeetingEditor";

const DAY = 86_400_000;
const startOf = (ymd: string) => new Date(`${ymd}T00:00:00`).getTime();
const shortDate = (ymd: string) => new Date(`${ymd}T00:00:00`).toLocaleDateString([], { month: "short", day: "numeric" });

function when(ms: number) {
  const d = new Date(ms);
  return {
    day: d.toLocaleDateString([], { weekday: "short", month: "short", day: "numeric" }),
    time: d.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" }),
  };
}

function Row({ m, onOpen }: { m: MeetingSummary; onOpen: () => void }) {
  const w = when(m.starts_at);
  return (
    <li className="border-b last:border-b-0">
      <button
        type="button" onClick={onOpen} aria-label={`Open meeting “${m.title}”`}
        className="flex w-full items-center gap-4 px-4 py-3 text-left outline-none hover:bg-accent/40 focus-visible:bg-accent/40 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
      >
        <span className="w-24 shrink-0">
          <span className="block text-[12px] font-medium">{w.day}</span>
          <span className="block text-[11px] tabular-nums text-muted-foreground">{w.time}</span>
        </span>
        <span className="min-w-0 flex-1">
          <span className="block truncate text-[13.5px] font-medium">{m.title}</span>
          {m.participants.length > 0 && (
            <span className="mt-0.5 flex items-center gap-1 truncate text-[12px] text-muted-foreground"><Users className="size-3 shrink-0" />{m.participants.join(", ")}</span>
          )}
        </span>
        <span className="hidden shrink-0 flex-col items-end gap-0.5 text-[11px] text-muted-foreground sm:flex">
          {m.action_items > 0 && (
            <span className={cn("flex items-center gap-1", m.open_action_items > 0 && "text-foreground")}>
              <ListChecks className="size-3" />{m.open_action_items === 0 ? "All actions done" : `${m.open_action_items} of ${m.action_items} actions open`}
            </span>
          )}
          {m.follow_up_date && <span className="flex items-center gap-1"><CalendarClock className="size-3" />Follow up {shortDate(m.follow_up_date)}</span>}
        </span>
      </button>
    </li>
  );
}

/**
 * Meetings (R1-35): newest first, filtered by participant and dates. Opening one, or starting a
 * new one (`newTick` changes, e.g. Mod+M), shows the editor in place of the list.
 */
export function MeetingsPage({ newTick = 0 }: { newTick?: number }) {
  const [open, setOpen] = useState<{ id: string | null; key: number } | null>(null);
  const [who, setWho] = useState("");
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const list = useId();
  const { data: names = [] } = useParticipantSuggestions(who);
  const filters: MeetingFilters = { participant: who.trim() || null, from: from ? startOf(from) : null, to: to ? startOf(to) + DAY : null };
  const { data: meetings, isPending } = useMeetings(filters);
  const filtered = Boolean(filters.participant || filters.from || filters.to);

  useEffect(() => {
    if (newTick > 0) setOpen({ id: null, key: newTick });
  }, [newTick]);

  if (open) {
    return (
      <MeetingEditor
        key={open.key} meetingId={open.id} onBack={() => setOpen(null)}
        onCreated={(id) => setOpen((o) => (o ? { ...o, id } : o))}
      />
    );
  }

  return (
    <div className="mx-auto max-w-4xl">
      <PageHeader
        title="Meetings" description="Decisions and action items, with action items one click from becoming tasks."
        actions={<Button onClick={() => setOpen({ id: null, key: Date.now() })} title={`New meeting (${mod}+M)`}><Plus />New meeting</Button>}
      />
      <div className="mb-3 flex flex-wrap items-end gap-2">
        <div className="space-y-1">
          <label htmlFor="meetings-who" className="text-[11px] font-medium text-muted-foreground">Participant</label>
          <Input id="meetings-who" list={list} value={who} onChange={(e) => setWho(e.target.value)} placeholder="Anyone" className="w-44" />
          <datalist id={list}>{names.map((n) => <option key={n} value={n} />)}</datalist>
        </div>
        <div className="space-y-1">
          <label htmlFor="meetings-from" className="text-[11px] font-medium text-muted-foreground">From</label>
          <Input id="meetings-from" type="date" value={from} onChange={(e) => setFrom(e.target.value)} className="w-40" />
        </div>
        <div className="space-y-1">
          <label htmlFor="meetings-to" className="text-[11px] font-medium text-muted-foreground">To</label>
          <Input id="meetings-to" type="date" value={to} onChange={(e) => setTo(e.target.value)} className="w-40" />
        </div>
        {filtered && <Button variant="ghost" size="sm" onClick={() => { setWho(""); setFrom(""); setTo(""); }}><X />Clear</Button>}
      </div>

      {isPending ? null : !meetings?.length ? (
        <div className="grid place-items-center rounded-xl border bg-card px-4 py-14 text-center">
          <span className="mb-2 grid size-9 place-items-center rounded-lg bg-muted text-muted-foreground"><Users className="size-4" /></span>
          <p className="text-[13px] font-medium">{filtered ? "No meetings match" : "No meetings yet"}</p>
          <p className="mt-0.5 max-w-xs text-xs text-muted-foreground">
            {filtered ? "Try another name or date range." : "Log a meeting to keep decisions and turn action items into tasks."}
          </p>
          {!filtered && <Button className="mt-4" size="sm" onClick={() => setOpen({ id: null, key: Date.now() })}><Plus />New meeting</Button>}
        </div>
      ) : (
        <ul aria-label="Meetings" className="overflow-hidden rounded-xl border bg-card">
          {meetings.map((m) => <Row key={m.id} m={m} onOpen={() => setOpen({ id: m.id, key: Date.now() })} />)}
        </ul>
      )}
    </div>
  );
}
