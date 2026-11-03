import { useState } from "react";
import { Check, ChevronDown, Info } from "lucide-react";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuRadioGroup, DropdownMenuRadioItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { Bars, type Bar } from "@/features/charts/Bars";
import { PageHeader, Tile } from "@/features/shell/PageHeader";
import { useRecentLogs, useSettings, useTrackingStatus, useUsage } from "@/hooks/usePluto";
import { dayLabel, weekdayNarrow, ymd } from "@/lib/dates";
import { formatDuration } from "@/lib/time";
import { cn } from "@/lib/utils";
import { CATEGORIES, categoryStyle } from "./categories";
import { DailyLogs } from "./DailyLogs";
import { OpenTabsPanel } from "./OpenTabsPanel";
import { ROW_HEIGHT, TileLayoutPicker, WIDTH_CLASS, useTileLayout } from "./tileLayout";
import { PrivacyRadar } from "./PrivacyRadar";
import { RANGES, rangeBounds, type RangeKey } from "./range";
import { TimeAnalytics } from "./TimeAnalytics";
import { TimeCard } from "./TimeCard";
import { TrackingSettings } from "./TrackingSettings";

function Stat({ label, value, sub }: { label: string; value: React.ReactNode; sub?: string | undefined }) {
  return (
    <div className="rounded-xl border bg-card p-4">
      <p className="text-xs font-medium text-muted-foreground">{label}</p>
      <p className="mt-2 truncate text-[26px] font-semibold leading-none tracking-tight tabular-nums">{value}</p>
      {sub && <p className="mt-1.5 truncate text-xs text-muted-foreground">{sub}</p>}
    </div>
  );
}

/** Time: where it went, per app and site, with filters. */
export function TimePage() {
  const [range, setRange] = useState<RangeKey>("today");
  const [category, setCategory] = useState<string>("all");
  const [view, setView] = useState<"apps" | "sites">("apps");
  const [mode, setMode] = useState<"general" | "analytics">("general");
  const [layout, setLayout] = useTileLayout();
  const { from, to, days } = rangeBounds(range);
  const { data: status } = useTrackingStatus();
  const { data: settings } = useSettings();
  const { data: usage } = useUsage(from, to);
  const logs = useRecentLogs(30);

  const cat = category === "all" ? null : category;
  const apps = (usage?.apps ?? []).filter((a) => !cat || a.category === cat);
  const total = apps.reduce((n, a) => n + a.total_seconds, 0);
  const byCategory = new Map<string, number>();
  for (const a of usage?.apps ?? []) byCategory.set(a.category, (byCategory.get(a.category) ?? 0) + a.total_seconds);
  const categories = [...byCategory.entries()].sort((a, b) => b[1] - a[1]);
  const all = categories.reduce((n, [, v]) => n + v, 0) || 1;

  const inRange = logs.filter(({ date }) => date.getTime() >= from && date.getTime() < to);
  const done = inRange.reduce((n, d) => n + (d.log?.snapshot.stats.completed_count ?? 0), 0);
  const planned = inRange.reduce((n, d) => n + (d.log?.snapshot.stats.planned_count ?? 0), 0);
  const barDays = logs.slice(-Math.max(7, days));
  const bars: Bar[] = barDays.map(({ date, log }) => ({
    key: ymd(date), value: log?.snapshot.stats.completed_count ?? 0, label: days > 7 ? String(date.getDate()) : weekdayNarrow(date),
    active: date.getTime() >= from && date.getTime() < to, title: `${dayLabel(date)}: ${log?.snapshot.stats.completed_count ?? 0} completed`,
  }));
  const tracking = status?.enabled ?? false;
  const sitesOn = tracking && settings?.["tracking.sites"] === true;
  const filtered = category !== "all" || range !== "today" || view !== "apps";

  return (
    <div className="space-y-4">
      <PageHeader title="Time" description="Where your time went, by app and site." actions={<TrackingSettings />} />
      <div className="flex flex-wrap items-center gap-2" role="group" aria-label="Filters">
        <ToggleGroup type="single" value={mode} onValueChange={(v) => v && setMode(v as "general" | "analytics")} aria-label="Time view">
          <ToggleGroupItem value="general">General</ToggleGroupItem>
          <ToggleGroupItem value="analytics">Analytics</ToggleGroupItem>
        </ToggleGroup>
        <span className="mx-1 hidden h-5 w-px bg-border sm:block" aria-hidden="true" />
        <ToggleGroup type="single" value={range} onValueChange={(v) => v && setRange(v as RangeKey)} aria-label="Period">
          {RANGES.map((r) => <ToggleGroupItem key={r.key} value={r.key}>{r.label}</ToggleGroupItem>)}
        </ToggleGroup>
        {mode === "general" && (<>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="outline" size="sm" className="h-8">
              {category === "all" ? "All categories" : (<><span className={cn("size-2 rounded-full", categoryStyle(category).dot)} />{category}</>)}<ChevronDown />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start">
            <DropdownMenuRadioGroup value={category} onValueChange={setCategory}>
              <DropdownMenuRadioItem value="all">All categories</DropdownMenuRadioItem>
              {CATEGORIES.map((c) => <DropdownMenuRadioItem key={c} value={c}>{c}</DropdownMenuRadioItem>)}
            </DropdownMenuRadioGroup>
          </DropdownMenuContent>
        </DropdownMenu>
        <ToggleGroup type="single" value={view} onValueChange={(v) => v && setView(v as "apps" | "sites")} aria-label="Group by">
          <ToggleGroupItem value="apps">Apps</ToggleGroupItem>
          <ToggleGroupItem value="sites">Sites</ToggleGroupItem>
        </ToggleGroup>
        {filtered && (
          <Button variant="ghost" size="sm" onClick={() => { setRange("today"); setCategory("all"); setView("apps"); }}>Reset filters</Button>
        )}
        </>)}
      </div>

      {mode === "analytics" ? <TimeAnalytics from={from} to={to} days={days} tracking={tracking} /> : (<>

      <div className="grid grid-cols-2 gap-4 xl:grid-cols-4">
        <Stat label="Total time" value={tracking ? formatDuration(total) : "—"} sub={tracking ? `${days === 1 ? "in the day" : `~${formatDuration(total / days)} a day`}` : "Tracking is off"} />
        <Stat label="Apps used" value={tracking ? apps.length : "—"} sub={tracking && usage?.domains.length ? `${usage.domains.length} sites visited` : undefined} />
        <Stat label="Most used" value={tracking ? (apps[0]?.app ?? "—") : "—"} sub={apps[0] ? `${formatDuration(apps[0].total_seconds)} · ${apps[0].category}` : undefined} />
        <Stat label="Tasks done" value={done} sub={planned ? `of ${planned} planned` : "in this period"} />
      </div>

      <TileLayoutPicker value={layout} onChange={setLayout} />
      <div className={cn("grid gap-4 lg:grid-cols-12", ROW_HEIGHT)}>
        <TimeCard from={from} to={to} category={cat} view={view} className={WIDTH_CLASS[layout.width][0]} title={view === "sites" ? "Sites" : "Apps"} />
        <Tile className={WIDTH_CLASS[layout.width][1]} title="By category" bodyClassName="p-4">
          {categories.length === 0 ? (
            <p className="py-10 text-center text-[13px] text-muted-foreground">{tracking ? "Nothing recorded yet." : "Turn on app tracking to see this."}</p>
          ) : (
            <>
              <div className="flex h-3 overflow-hidden rounded-full bg-muted" role="img" aria-label="Share of time by category">
                {categories.map(([c, v]) => <span key={c} className={cn("h-full", categoryStyle(c).bar)} style={{ width: `${(v / all) * 100}%` }} />)}
              </div>
              <ul className="mt-4 space-y-1">
                {categories.map(([c, v]) => (
                  <li key={c}>
                    <button type="button" onClick={() => setCategory(category === c ? "all" : c)} aria-pressed={category === c}
                      className="flex w-full items-center gap-2.5 rounded-md px-2 py-1.5 text-left text-[13px] outline-none hover:bg-accent/50 focus-visible:ring-2 focus-visible:ring-ring">
                      <span className={cn("size-2 rounded-full", categoryStyle(c).dot)} />
                      <span className="flex-1">{c}</span>
                      {category === c && <Check className="size-3.5 text-primary" />}
                      <span className="tabular-nums text-muted-foreground">{Math.round((v / all) * 100)}%</span>
                      <span className="w-14 text-right font-medium tabular-nums">{formatDuration(v)}</span>
                    </button>
                  </li>
                ))}
              </ul>
            </>
          )}
        </Tile>
      </div>

      {view === "sites" && <PrivacyRadar from={from} to={to} enabled={sitesOn} />}
      {/* Two even tiles side by side (FR-62: open tabs is always here, not only under "Sites", so it can be found). */}
      <div className="grid gap-4 lg:grid-cols-2 lg:[&>*]:h-72">
        <Tile title="Tasks completed" action={<span className="text-[11px] text-muted-foreground">{done} in this period</span>} bodyClassName="flex p-4">
          <Bars bars={bars} className="flex-1" ariaLabel={`Tasks completed per day: ${bars.map((b) => b.value).join(", ")}`} />
        </Tile>
        <OpenTabsPanel enabled={sitesOn} />
      </div>
      <DailyLogs className="max-h-[26rem]" />
      </>)}
      <p className="flex items-center gap-1.5 text-xs text-muted-foreground"><Info className="size-3.5" />Time is recorded on this computer only. Window titles and full web addresses are never stored.</p>
    </div>
  );
}
