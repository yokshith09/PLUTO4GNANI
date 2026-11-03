import { useState } from "react";
import { ConfirmAction } from "@/components/ConfirmAction";
import { Presets } from "@/components/Presets";
import { dayPresets } from "@/lib/datePresets";
import { CalendarDays, Check, ExternalLink, Link2, Plus, Repeat2, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { useRepeatDays, useTaskActions } from "@/hooks/usePluto";
import { daysAgo, parseYmd, ymd } from "@/lib/dates";
import { WEEKDAYS, isoWeekday, repeatLabel } from "@/lib/repeat";
import { cn } from "@/lib/utils";
import type { Repeat, Task, TaskPlan } from "@/ipc";
import { linkLabel, openLink } from "./TaskBits";

// ---- when: one day, chosen days, or repeating (FR-108, FR-109) ------------------------------

export type PlanMode = "one" | "days" | "repeat";
export type PlanState = { mode: PlanMode; days: string[]; repeat: Repeat; until: string };

export function planStateOf(task: Pick<Task, "days" | "repeat" | "repeat_until" | "planned_date">): PlanState {
  const mode: PlanMode = task.repeat ? "repeat" : task.days.length ? "days" : "one";
  const fallback: Repeat = { kind: "weekly", days: task.planned_date ? [isoWeekday(task.planned_date)] : [1] };
  return { mode, days: task.days, repeat: task.repeat ?? fallback, until: task.repeat_until ?? "" };
}

/** What to send to the core for this state. */
export function planOf(s: PlanState): TaskPlan {
  if (s.mode === "days") return { days: s.days, repeat: null, repeat_until: null };
  if (s.mode === "repeat") return { days: [], repeat: s.repeat, repeat_until: s.until || null };
  return { days: [], repeat: null, repeat_until: null };
}

export const samePlan = (a: TaskPlan, b: TaskPlan) => JSON.stringify(a) === JSON.stringify(b);

const short = (d: string) => parseYmd(d).toLocaleDateString([], { weekday: "short", month: "short", day: "numeric" });

export function PlanEditor({ value, onChange, planned, onPlanned }: {
  value: PlanState; onChange: (s: PlanState) => void; planned: string; onPlanned: (d: string) => void;
}) {
  const [pick, setPick] = useState("");
  const today = ymd(new Date());
  const next7 = Array.from({ length: 7 }, (_, i) => ymd(daysAgo(-i)));
  const toggleDay = (d: string) =>
    onChange({ ...value, days: value.days.includes(d) ? value.days.filter((x) => x !== d) : [...value.days, d].sort() });
  const rule = value.repeat;
  const weekly = rule.kind === "weekly" ? rule.days : [];
  const setRule = (repeat: Repeat) => onChange({ ...value, repeat });

  return (
    <div className="space-y-3" role="group" aria-label="When">
      <ToggleGroup type="single" value={value.mode} onValueChange={(m) => m && onChange({ ...value, mode: m as PlanMode })} aria-label="Plan it for" className="w-full">
        <ToggleGroupItem value="one" className="flex-1 px-2">One day</ToggleGroupItem>
        <ToggleGroupItem value="days" className="flex-1 px-2"><CalendarDays />Chosen days</ToggleGroupItem>
        <ToggleGroupItem value="repeat" className="flex-1 px-2"><Repeat2 />Repeats</ToggleGroupItem>
      </ToggleGroup>

      {value.mode === "one" && (
        <div className="flex items-center gap-2">
          <Input type="date" value={planned} onChange={(e) => onPlanned(e.target.value)} aria-label="Planned for" className="w-44" />
          <span className="text-xs text-muted-foreground">Empty: someday</span>
        </div>
      )}
      {value.mode === "one" && (
        <Presets label="Planned day presets" options={dayPresets()} value={planned} onPick={onPlanned} />
      )}

      {value.mode === "days" && (
        <div className="space-y-2">
          <div className="flex flex-wrap gap-1.5" role="group" aria-label="Next 7 days">
            {next7.map((d) => (
              <button key={d} type="button" aria-pressed={value.days.includes(d)} onClick={() => toggleDay(d)}
                className={cn("h-8 rounded-md border px-2.5 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring",
                  value.days.includes(d) ? "border-primary bg-primary/15 text-foreground" : "border-border text-muted-foreground hover:border-foreground/30")}>
                {d === today ? "Today" : parseYmd(d).toLocaleDateString([], { weekday: "short", day: "numeric" })}
              </button>
            ))}
          </div>
          <div className="flex items-center gap-2">
            <Input type="date" value={pick} min={today} onChange={(e) => setPick(e.target.value)} aria-label="Add a day" className="w-44" />
            <Button type="button" size="sm" variant="outline" disabled={!pick || value.days.includes(pick)} onClick={() => { toggleDay(pick); setPick(""); }}><Plus />Add day</Button>
          </div>
          {value.days.length > 0 ? (
            <ul className="flex flex-wrap gap-1.5" aria-label="Chosen days">
              {value.days.map((d) => (
                <li key={d} className="flex h-7 items-center gap-1 rounded-md bg-muted pl-2 text-xs">
                  {short(d)}
                  <ConfirmAction message={`Remove ${short(d)} from the plan?`} verb="Remove" onConfirm={() => toggleDay(d)}>
                    <button type="button" aria-label={`Remove ${short(d)}`} className="grid size-6 place-items-center rounded text-muted-foreground hover:text-foreground"><X className="size-3" /></button>
                  </ConfirmAction>
                </li>
              ))}
            </ul>
          ) : <p className="text-xs text-muted-foreground">Pick the days you want to work on it. Finishing it once finishes it.</p>}
        </div>
      )}

      {value.mode === "repeat" && (
        <div className="space-y-2.5">
          <ToggleGroup type="single" value={rule.kind} onValueChange={(k) => k && setRule(k === "daily" ? { kind: "daily" } : k === "every" ? { kind: "every", n: 2 } : { kind: "weekly", days: weekly.length ? weekly : [1, 3, 5] })} aria-label="Repeat" className="w-full">
            <ToggleGroupItem value="daily" className="flex-1 px-2">Every day</ToggleGroupItem>
            <ToggleGroupItem value="weekly" className="flex-1 px-2">On days of the week</ToggleGroupItem>
            <ToggleGroupItem value="every" className="flex-1 px-2">Every few days</ToggleGroupItem>
          </ToggleGroup>
          {rule.kind === "weekly" && (
            <div className="flex gap-1.5" role="group" aria-label="Days of the week">
              {WEEKDAYS.map((w, i) => {
                const on = weekly.includes(i + 1);
                return (
                  <button key={w} type="button" aria-pressed={on} aria-label={w}
                    onClick={() => setRule({ kind: "weekly", days: on ? weekly.filter((x) => x !== i + 1) : [...weekly, i + 1].sort() })}
                    className={cn("h-8 w-10 rounded-md border text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring", on ? "border-primary bg-primary/15" : "border-border text-muted-foreground")}>
                    {w}
                  </button>
                );
              })}
            </div>
          )}
          {rule.kind === "every" && (
            <label className="flex items-center gap-2 text-xs">Every
              <Input type="number" min={1} max={365} value={rule.n} onChange={(e) => setRule({ kind: "every", n: Math.max(1, Math.min(365, Number(e.target.value) || 1)) })} aria-label="Every how many days" className="w-20" />
              days</label>
          )}
          <div className="grid gap-2 sm:grid-cols-2">
            <label className="space-y-1 text-xs text-muted-foreground">Starts
              <Input type="date" value={planned} onChange={(e) => onPlanned(e.target.value)} aria-label="Starts" placeholder="Today" />
            </label>
            <label className="space-y-1 text-xs text-muted-foreground">Ends (optional)
              <Input type="date" value={value.until} min={planned || today} onChange={(e) => onChange({ ...value, until: e.target.value })} aria-label="Ends" />
            </label>
          </div>
          <p className="text-xs text-muted-foreground">
            {rule.kind === "weekly" && weekly.length === 0 ? "Pick at least one day." : `${repeatLabel(rule)}. Tick it each day it comes up; ticking one day never touches another.`}
          </p>
        </div>
      )}
    </div>
  );
}

// ---- repeat history: ticked or not, kindly (FR-109, FR-92) ----------------------------------

export function RepeatHistory({ task }: { task: Task }) {
  const { data: days = [] } = useRepeatDays(task.id, !!task.repeat);
  const { tick } = useTaskActions();
  if (!task.repeat || days.length === 0) return null;
  const done = days.filter((d) => d.ticked).length;
  const notTicked = days.filter((d) => !d.ticked && d.day !== ymd(new Date()));
  return (
    <section aria-label="Repeat days" className="rounded-lg border bg-muted/20 p-3">
      <h3 className="flex items-center gap-1.5 text-xs font-medium text-muted-foreground"><Repeat2 className="size-3.5" />{repeatLabel(task.repeat)}</h3>
      <p className="mt-1 text-[13px]">Done on <b>{done}</b> of the last {days.length} repeat {days.length === 1 ? "day" : "days"}.</p>
      <ul className="mt-2 flex flex-wrap gap-1.5">
        {days.map((d) => (
          <li key={d.day}>
            <button type="button" aria-pressed={d.ticked} onClick={() => void tick(task.id, d.day, !d.ticked)}
              aria-label={`${short(d.day)}: ${d.ticked ? "ticked" : "not ticked"}. Click to ${d.ticked ? "untick" : "tick"}.`}
              className={cn("flex h-7 items-center gap-1 rounded-md border px-2 text-[11px] outline-none focus-visible:ring-2 focus-visible:ring-ring",
                d.ticked ? "border-success/40 bg-success/12 text-success" : "border-dashed border-border text-muted-foreground")}>
              {d.ticked && <Check className="size-3" />}{d.day === ymd(new Date()) ? "Today" : short(d.day)}
            </button>
          </li>
        ))}
      </ul>
      {notTicked.length > 0 && (
        <p className="mt-2 text-[11px] text-muted-foreground">
          Not ticked: {notTicked.map((d) => short(d.day)).join(", ")}. Did it anyway? Tick the day.
        </p>
      )}
    </section>
  );
}

// ---- links (FR-106) -------------------------------------------------------------------------

export function LinksEditor({ task }: { task: Task }) {
  const { setLinks } = useTaskActions();
  const [url, setUrl] = useState("");
  const [name, setName] = useState("");
  const current = task.links.map((l) => ({ url: l.url, title: l.title }));
  const add = async () => {
    if (!url.trim()) return;
    try {
      await setLinks(task.id, [...current, { url: url.trim(), title: name.trim() }]);
      setUrl("");
      setName("");
    } catch { /* the error is already shown above the pet */ }
  };
  return (
    <section aria-label="Links">
      <h3 className="flex items-center gap-1.5 text-[13px] font-semibold"><Link2 className="size-4 text-muted-foreground" />Links{task.links.length > 0 && <span className="text-xs font-normal tabular-nums text-muted-foreground">{task.links.length}</span>}</h3>
      <p className="mb-2 mt-0.5 text-xs text-muted-foreground">Web pages for this task. Click one to open it.</p>
      {task.links.length > 0 && (
        <ul className="mb-2 space-y-1">
          {task.links.map((l, i) => (
            <li key={l.id} className="group flex items-center gap-2 rounded-md bg-muted/40 px-2.5 py-1.5">
              <button type="button" onClick={() => openLink(l.url)} title={l.url} className="flex min-w-0 flex-1 items-center gap-1.5 text-left text-[13px] outline-none hover:text-primary focus-visible:ring-2 focus-visible:ring-ring">
                <ExternalLink className="size-3.5 shrink-0" /><span className="truncate">{linkLabel(l)}</span>
                {l.title && <span className="truncate text-[11px] text-muted-foreground">{l.url.replace(/^https?:\/\//, "")}</span>}
              </button>
              <ConfirmAction message="Remove this link?" verb="Remove" onConfirm={() => void setLinks(task.id, current.filter((_, j) => j !== i))}>
                <button type="button" aria-label={`Remove link ${linkLabel(l)}`}
                  className="grid size-6 shrink-0 place-items-center rounded text-muted-foreground outline-none hover:bg-destructive/15 hover:text-destructive focus-visible:ring-2 focus-visible:ring-ring"><X className="size-3.5" /></button>
              </ConfirmAction>
            </li>
          ))}
        </ul>
      )}
      <form className="flex gap-2" onSubmit={(e) => { e.preventDefault(); void add(); }}>
        <Input value={url} onChange={(e) => setUrl(e.target.value)} placeholder="Paste a link" aria-label="New link" maxLength={2048} />
        <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="Name (optional)" aria-label="Link name" maxLength={200} className="w-44" />
        <Button type="submit" size="sm" variant="secondary" disabled={!url.trim()} aria-label="Add link">Add</Button>
      </form>
    </section>
  );
}
