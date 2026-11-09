import { useEffect, useId, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { CalendarClock, Check, ListChecks, MoreHorizontal, Mic, Plus, Search, SlidersHorizontal, Upload, Users, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { meetingAi, type TranscriptHit } from "@/ipc/meetingAi";
import { PageHeader } from "@/features/shell/PageHeader";
import { useMeetings, useParticipantSuggestions } from "@/hooks/usePluto";
import { mod } from "@/lib/platform";
import { cn } from "@/lib/utils";
import type { MeetingFilters, MeetingSummary } from "@/ipc";
import { CommitmentsPanel } from "./CommitmentsPanel";
import { Marked } from "./Marked";
import { ImportDialog } from "./ImportDialog";
import { RecordDialog } from "./RecordDialog";
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
          {m.decisions > 0 && <span className="flex items-center gap-1"><Check className="size-3" />{m.decisions} decision{m.decisions === 1 ? "" : "s"}</span>}
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

const clock = (ms: number) => {
  const t = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(t / 60)}:${String(t % 60).padStart(2, "0")}`;
};

/** What was said that matches the search, grouped by meeting; pressing one opens the meeting at that line. */
function TranscriptResults({ query, onOpen }: { query: string; onOpen: (meetingId: string, segmentId: string) => void }) {
  const q = query.trim();
  const { data: hits = [], isFetching } = useQuery({
    queryKey: ["meetings", "transcript-search", q],
    queryFn: () => meetingAi.transcriptSearch(q),
    enabled: q.length >= 2,
  });
  const groups = useMemo(() => {
    const by = new Map<string, TranscriptHit[]>();
    for (const h of hits) by.set(h.meeting_id, [...(by.get(h.meeting_id) ?? []), h]);
    return [...by.values()];
  }, [hits]);
  if (q.length < 2) return null;
  return (
    <section aria-label="In transcripts" className="mb-5">
      <h2 className="mb-2 text-[13px] font-semibold">In what was said</h2>
      {groups.length === 0 ? (
        <p className="rounded-xl border bg-card px-4 py-5 text-[13px] text-muted-foreground">{isFetching ? "Searching…" : "Nothing in any transcript matches."}</p>
      ) : (
        <div className="space-y-3">
          {groups.map((g) => (
            <div key={g[0]?.meeting_id} className="overflow-hidden rounded-xl border bg-card">
              <div className="flex items-baseline gap-2 border-b bg-muted/30 px-4 py-2">
                <span className="truncate text-[13px] font-medium">{g[0]?.meeting_title}</span>
                <span className="text-[11px] text-muted-foreground">{when(g[0]?.meeting_starts_at ?? 0).day}</span>
              </div>
              <ul className="divide-y">
                {g.map((h) => (
                  <li key={h.segment_id}>
                    <button type="button" onClick={() => onOpen(h.meeting_id, h.segment_id)} aria-label={`Open “${h.meeting_title}” at ${clock(h.start_ms)}`}
                      className="flex w-full gap-3 px-4 py-2.5 text-left text-[13px] outline-none hover:bg-accent/40 focus-visible:bg-accent/40 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring">
                      <span className="w-10 shrink-0 pt-0.5 text-right text-[11px] tabular-nums text-muted-foreground">{clock(h.start_ms)}</span>
                      <span className="min-w-0 flex-1">
                        {h.speaker && <span className="mr-2 font-medium">{h.speaker}</span>}
                        <span className="text-muted-foreground"><Marked text={h.text} query={q} /></span>
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

/**
 * Meetings: one search box for everything (titles, people, and what was said), the newest first
 * below it. Filters and the commitments list stay folded away until asked for. Opening one, or
 * starting a new one (`newTick` changes, e.g. Mod+M), shows the editor in place of the list.
 */
export function MeetingsPage({ newTick = 0 }: { newTick?: number }) {
  const [open, setOpen] = useState<{ id: string | null; key: number; segment?: string } | null>(null);
  const [query, setQuery] = useState("");
  const [who, setWho] = useState("");
  const [importing, setImporting] = useState(false);
  const [recording, setRecording] = useState(false);
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const list = useId();
  const { data: names = [] } = useParticipantSuggestions(who);
  const filters: MeetingFilters = { participant: who.trim() || null, from: from ? startOf(from) : null, to: to ? startOf(to) + DAY : null };
  const { data: all, isPending } = useMeetings(filters);
  const filtered = Boolean(filters.participant || filters.from || filters.to);
  const q = query.trim().toLowerCase();
  const meetings = q ? all?.filter((m) => m.title.toLowerCase().includes(q) || m.participants.some((p) => p.toLowerCase().includes(q))) : all;

  useEffect(() => {
    if (newTick > 0) setOpen({ id: null, key: newTick });
  }, [newTick]);

  if (open) {
    return (
      <MeetingEditor
        key={open.key} meetingId={open.id} focusSegment={open.segment ?? null} onBack={() => setOpen(null)}
        onCreated={(id) => setOpen((o) => (o ? { ...o, id } : o))}
      />
    );
  }

  return (
    <div>
      <PageHeader
        title="Meetings" description="Record a call, get the decisions and action items, and search everything that was said."
        actions={(
          <>
            <Button onClick={() => setRecording(true)}><Mic />Record</Button>
            <Button variant="outline" onClick={() => setOpen({ id: null, key: Date.now() })} title={`New meeting (${mod}+M)`}><Plus />New meeting</Button>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="outline" size="icon" aria-label="More"><MoreHorizontal /></Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuItem onSelect={() => setImporting(true)}><Upload />Import a recording</DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </>
        )}
      />
      <RecordDialog open={recording} onClose={() => setRecording(false)} onRecorded={(id) => { setRecording(false); setOpen({ id, key: Date.now() }); }} />
      <ImportDialog open={importing} onClose={() => setImporting(false)} onImported={(id) => { setImporting(false); setOpen({ id, key: Date.now() }); }} />

      <div className="mb-3 flex items-center gap-2">
        <div className="relative flex-1">
          <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input aria-label="Search meetings" value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search meetings, people and what was said" className="pl-9 pr-9" />
          {query && <button type="button" aria-label="Clear search" onClick={() => setQuery("")} className="absolute right-2 top-1/2 grid size-6 -translate-y-1/2 place-items-center rounded text-muted-foreground hover:bg-accent hover:text-foreground"><X className="size-3.5" /></button>}
        </div>
        <Button variant={filtersOpen || filtered ? "secondary" : "outline"} aria-expanded={filtersOpen} onClick={() => setFiltersOpen((v) => !v)}>
          <SlidersHorizontal />Filters{filtered && <span className="ml-1 size-1.5 rounded-full bg-primary" aria-label="Filters on" />}
        </Button>
      </div>
      {filtersOpen && (
        <div className="mb-3 flex flex-wrap items-end gap-2 rounded-xl border bg-card p-3">
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
      )}

      {!q && <CommitmentsPanel />}
      <TranscriptResults query={query} onOpen={(id, segment) => setOpen({ id, key: Date.now(), segment })} />

      {isPending ? null : !meetings?.length ? (
        q ? null : (
          <div className="grid place-items-center rounded-xl border bg-card px-4 py-14 text-center">
            <span className="mb-2 grid size-9 place-items-center rounded-lg bg-muted text-muted-foreground"><Users className="size-4" /></span>
            <p className="text-[13px] font-medium">{filtered ? "No meetings match" : "No meetings yet"}</p>
            <p className="mt-0.5 max-w-xs text-xs text-muted-foreground">
              {filtered ? "Try another name or date range." : "Press Record, or open a Google Meet call and let Pluto offer to record it."}
            </p>
            {!filtered && <Button className="mt-4" size="sm" onClick={() => setRecording(true)}><Mic />Record a meeting</Button>}
          </div>
        )
      ) : (
        <section aria-label={q ? "Matching meetings" : undefined}>
          {q && <h2 className="mb-2 text-[13px] font-semibold">Meetings</h2>}
          <ul aria-label="Meetings" className="overflow-hidden rounded-xl border bg-card">
            {meetings.map((m) => <Row key={m.id} m={m} onOpen={() => setOpen({ id: m.id, key: Date.now() })} />)}
          </ul>
        </section>
      )}
    </div>
  );
}
