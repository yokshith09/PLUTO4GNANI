import { useQueries } from "@tanstack/react-query";
import { Lightbulb } from "lucide-react";
import { Bars, type Bar } from "@/features/charts/Bars";
import { StackedBars, type StackedBar } from "@/features/charts/StackedBars";
import { Tile } from "@/features/shell/PageHeader";
import { ipc } from "@/ipc";
import { useRecentLogs, useUsage } from "@/hooks/usePluto";
import { dayLabel, weekdayNarrow, ymd } from "@/lib/dates";
import { formatDuration } from "@/lib/time";
import { cn } from "@/lib/utils";
import { categoryStyle } from "./categories";
import { changeLabel, insights } from "./analyticsInsights";

const DAY = 86_400_000;

function Figure({ label, value, sub }: { label: string; value: React.ReactNode; sub?: string | undefined }) {
  return (
    <div className="rounded-xl border bg-card p-4">
      <p className="text-xs font-medium text-muted-foreground">{label}</p>
      <p className="mt-2 truncate text-[26px] font-semibold leading-none tracking-tight tabular-nums">{value}</p>
      {sub && <p className="mt-1.5 truncate text-xs text-muted-foreground">{sub}</p>}
    </div>
  );
}

/** The colours of the categories shown, in the tile header. */
function Legend({ categories }: { categories: string[] }) {
  return (
    <ul className="flex flex-wrap items-center justify-end gap-x-3 gap-y-0.5 text-[11px] text-muted-foreground" aria-label="Categories">
      {categories.slice(0, 5).map((c) => <li key={c} className="flex items-center gap-1"><span className={cn("size-2 rounded-full", categoryStyle(c).dot)} />{c}</li>)}
    </ul>
  );
}

/** Time, analysed: per-app shares, screen time per day and by hour, and task completion. All from local data. */
export function TimeAnalytics({ from, to, days, tracking }: { from: number; to: number; days: number; tracking: boolean }) {
  const { data: usage } = useUsage(from, to);
  // The period just before, the same length, to compare with.
  const { data: before } = useUsage(from - (to - from), from);
  const logs = useRecentLogs(30);
  const dayStarts = Array.from({ length: days }, (_, i) => new Date(new Date(from).getFullYear(), new Date(from).getMonth(), new Date(from).getDate() + i).getTime());
  const daily = useQueries({
    queries: dayStarts.map((s) => ({
      queryKey: ["usage", s, s + DAY],
      queryFn: () => ipc.usageSummary(s, new Date(new Date(s).getFullYear(), new Date(s).getMonth(), new Date(s).getDate() + 1).getTime()),
      enabled: days > 1,
    })),
  });

  const apps = usage?.apps ?? [];
  const total = usage?.total_seconds ?? 0;
  const top = apps.slice(0, 8);
  const topMax = Math.max(1, ...top.map((a) => a.total_seconds));
  const byCategory = new Map<string, number>();
  for (const a of apps) byCategory.set(a.category, (byCategory.get(a.category) ?? 0) + a.total_seconds);
  const categories = [...byCategory.entries()].sort((a, b) => b[1] - a[1]);

  const perDay = dayStarts.map((s, i) => ({ date: new Date(s), seconds: days > 1 ? (daily[i]?.data?.total_seconds ?? 0) : total }));
  const busiest = perDay.reduce<(typeof perDay)[number] | null>((m, d) => (d.seconds > (m?.seconds ?? 0) ? d : m), null);
  const dayBars: StackedBar[] = perDay.map(({ date, seconds }, i) => {
    const byCat = new Map<string, number>();
    for (const a of daily[i]?.data?.apps ?? []) byCat.set(a.category, (byCat.get(a.category) ?? 0) + a.total_seconds);
    return {
      key: ymd(date), label: days > 7 ? String(date.getDate()) : weekdayNarrow(date), active: true,
      title: `${dayLabel(date)}: ${formatDuration(seconds)}`,
      parts: categories.map(([c]) => ({ name: c, seconds: byCat.get(c) ?? 0, cls: categoryStyle(c).bar })),
    };
  });
  const hourly = usage?.hourly_seconds ?? new Array<number>(24).fill(0);
  const hourBars: Bar[] = hourly.map((v, h) => ({ key: String(h), value: v, label: h % 6 === 0 ? String(h) : "", title: `${String(h).padStart(2, "0")}:00: ${formatDuration(v)}`, active: v === Math.max(...hourly) && v > 0 }));

  const facts = insights({ total, prevTotal: before?.total_seconds ?? 0, days, categories, hourly, perDay, topApp: apps[0] ? { app: apps[0].app, seconds: apps[0].total_seconds } : undefined });
  const delta = tracking ? changeLabel(total, before?.total_seconds ?? 0) : null;

  const inRange = logs.filter(({ date }) => date.getTime() >= from && date.getTime() < to);
  const done = inRange.reduce((n, d) => n + (d.log?.snapshot.stats.completed_count ?? 0), 0);
  const planned = inRange.reduce((n, d) => n + (d.log?.snapshot.stats.planned_count ?? 0), 0);
  const rate = planned > 0 ? Math.min(100, Math.round((done / planned) * 100)) : null;
  const taskBars: Bar[] = dayStarts.map((s) => {
    const entry = logs.find(({ date }) => date.getTime() === s);
    const n = entry?.log?.snapshot.stats.completed_count ?? 0;
    const date = new Date(s);
    return { key: ymd(date), value: n, label: days > 7 ? String(date.getDate()) : weekdayNarrow(date), active: true, title: `${dayLabel(date)}: ${n} completed` };
  });
  const empty = <p className="py-10 text-center text-[13px] text-muted-foreground">{tracking ? "Nothing recorded yet." : "Turn on app tracking to see this."}</p>;

  return (
    <div className="space-y-4" data-testid="time-analytics">
      <div className="grid grid-cols-2 gap-4 xl:grid-cols-4">
        <Figure label="Screen time" value={tracking ? formatDuration(total) : "—"} sub={tracking ? `${days > 1 ? `~${formatDuration(total / days)} a day` : "in the day"}${delta ? ` · ${delta} vs before` : ""}` : "Tracking is off"} />
        <Figure label="Busiest day" value={tracking && busiest && busiest.seconds > 0 ? dayLabel(busiest.date) : "—"} sub={busiest && busiest.seconds > 0 ? formatDuration(busiest.seconds) : undefined} />
        <Figure label="Tasks done" value={done} sub={planned ? `of ${planned} planned` : "in this period"} />
        <Figure label="Completion" value={rate === null ? "—" : `${rate}%`} sub={rate === null ? "Nothing planned" : "of planned tasks"} />
      </div>

      {tracking && facts.length > 0 && (
        <section aria-label="What stands out" className="rounded-xl border bg-card p-4">
          <h2 className="flex items-center gap-1.5 text-[13px] font-semibold"><Lightbulb className="size-4 text-primary" aria-hidden />What stands out</h2>
          <ul className="mt-2 grid gap-x-8 gap-y-1.5 text-[13px] sm:grid-cols-2">
            {facts.map((f) => <li key={f} className="flex gap-2"><span aria-hidden className="mt-2 size-1 shrink-0 rounded-full bg-primary" />{f}</li>)}
          </ul>
        </section>
      )}

      <div className="grid gap-4 lg:grid-cols-12 lg:[&>*]:h-72">
        <Tile className={days > 1 ? "lg:col-span-7" : "lg:col-span-12"} title={days > 1 ? "Screen time per day" : "Screen time by hour"} action={days > 1 && categories.length > 0 ? <Legend categories={categories.map(([c]) => c)} /> : undefined} bodyClassName="flex p-4">
          {tracking && total > 0 ? (days > 1
            ? <StackedBars bars={dayBars} className="flex-1" ariaLabel={`Screen time per day: ${perDay.map((d) => formatDuration(d.seconds)).join(", ")}`} />
            : <Bars bars={hourBars} className="flex-1" ariaLabel="Screen time by hour of the day" />) : empty}
        </Tile>
        {days > 1 && (
          <Tile className="lg:col-span-5" title="Busiest hours" bodyClassName="flex p-4">
            {tracking && total > 0 ? <Bars bars={hourBars} className="flex-1" ariaLabel="Screen time by hour of the day" /> : empty}
          </Tile>
        )}
      </div>

      <div className="grid gap-4 lg:grid-cols-12">
        <Tile className="lg:col-span-7" title="Top apps" bodyClassName="p-4">
          {top.length === 0 ? empty : (
            <ul className="space-y-3">
              {top.map((a) => (
                <li key={a.app}>
                  <div className="flex items-baseline justify-between gap-2 text-[13px]">
                    <span className="min-w-0 flex-1 truncate">{a.app}</span>
                    <span className="tabular-nums text-muted-foreground">{total > 0 ? Math.round((a.total_seconds / total) * 100) : 0}%</span>
                    <span className="w-14 text-right font-medium tabular-nums">{formatDuration(a.total_seconds)}</span>
                  </div>
                  <div className="mt-1 h-1.5 overflow-hidden rounded-full bg-muted">
                    <div className={cn("h-full rounded-full", categoryStyle(a.category).bar)} style={{ width: `${(a.total_seconds / topMax) * 100}%` }} />
                  </div>
                </li>
              ))}
            </ul>
          )}
        </Tile>
        <Tile className="lg:col-span-5" title="By category" bodyClassName="p-4">
          {categories.length === 0 ? empty : (
            <ul className="space-y-2.5">
              {categories.map(([c, v]) => (
                <li key={c} className="flex items-center gap-2.5 text-[13px]">
                  <span className={cn("size-2 rounded-full", categoryStyle(c).dot)} />
                  <span className="flex-1">{c}</span>
                  <span className="tabular-nums text-muted-foreground">{total > 0 ? Math.round((v / total) * 100) : 0}%</span>
                  <span className="w-14 text-right font-medium tabular-nums">{formatDuration(v)}</span>
                </li>
              ))}
            </ul>
          )}
        </Tile>
      </div>

      {days > 1 && <Tile className="h-64" title="Tasks completed" action={<span className="text-[11px] text-muted-foreground">{done} in this period</span>} bodyClassName="flex p-4">
        <Bars bars={taskBars} className="flex-1" ariaLabel={`Tasks completed per day: ${taskBars.map((b) => b.value).join(", ")}`} />
      </Tile>}
    </div>
  );
}
