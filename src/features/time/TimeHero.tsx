import { useState } from "react";
import { Globe, LayoutGrid, ShieldCheck, TrendingDown, TrendingUp } from "lucide-react";
import { Bars } from "@/features/charts/Bars";
import { Switch } from "@/components/ui/switch";
import { Tile } from "@/features/shell/PageHeader";
import { useTrackingStatus, useUsage } from "@/hooks/usePluto";
import { useSetting } from "@/hooks/useTheme";
import { formatDuration } from "@/lib/time";
import { cn } from "@/lib/utils";
import { categoryStyle } from "./categories";
import { rangeBounds } from "./range";
import { SessionsDialog, type Detail } from "./SessionsDialog";
import { SitesEmpty } from "./SitesEmpty";

/** Today's total screen time, large, with the day's shape and where it went by category. */
export function TimeHero({ className }: { className?: string }) {
  const { data: status } = useTrackingStatus();
  const today = rangeBounds("today");
  const yesterday = rangeBounds("yesterday");
  const { data: usage } = useUsage(today.from, today.to);
  const { data: before } = useUsage(yesterday.from, yesterday.to);
  const [, setEnabled] = useSetting("tracking.apps");
  const tracking = status?.enabled ?? false;
  const [detail, setDetail] = useState<Detail | null>(null);

  const total = usage?.total_seconds ?? 0;
  const delta = total - (before?.total_seconds ?? 0);
  const hours = (usage?.hourly_seconds ?? Array<number>(24).fill(0)).map((v, h) => ({
    key: String(h), value: v, label: h % 6 === 0 ? String(h) : "", title: `${String(h).padStart(2, "0")}:00 · ${formatDuration(v)}`,
    active: new Date().getHours() === h,
  }));

  return (
    <Tile className={className} title="Total time today" bodyClassName="flex flex-col p-5">
      {status && !status.supported ? (
        <p className="m-auto text-center text-[13px] text-muted-foreground">App time tracking isn’t available on this system yet.</p>
      ) : !tracking ? (
        <div className="m-auto flex max-w-xs flex-col items-center text-center">
          <span className="mb-3 grid size-10 place-items-center rounded-lg bg-muted text-muted-foreground"><ShieldCheck className="size-5" /></span>
          <p className="text-sm font-medium">See where your day goes</p>
          <p className="mt-1 text-[13px] leading-relaxed text-muted-foreground">Stays on this computer. No window titles, no full web addresses. Delete it any time.</p>
          <label className="mt-4 flex items-center gap-2.5 text-[13px] font-medium">
            <Switch checked={false} onCheckedChange={(v) => void setEnabled(v)} aria-label="Track app usage" />
            Turn on app tracking
          </label>
        </div>
      ) : (
        <>
          <p className="text-6xl font-semibold leading-none tracking-tight tabular-nums" aria-label={`Total time today ${formatDuration(total)}`}>{formatDuration(total)}</p>
          <p className="mt-3 flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-muted-foreground">
            <span className="inline-flex items-center gap-1"><LayoutGrid className="size-3.5" />{usage?.apps.length ?? 0} apps</span>
            <span className="inline-flex items-center gap-1"><Globe className="size-3.5" />{usage?.domains.length ?? 0} sites</span>
            {before && before.total_seconds > 0 && delta !== 0 && (
              <span className={cn("inline-flex items-center gap-1", delta > 0 ? "text-primary" : "text-success")}>
                {delta > 0 ? <TrendingUp className="size-3.5" /> : <TrendingDown className="size-3.5" />}
                {formatDuration(Math.abs(delta))} {delta > 0 ? "more" : "less"} than yesterday
              </span>
            )}
          </p>
          <div className="mt-5 grid min-h-0 flex-1 gap-5 sm:grid-cols-2">
            <UsageList label="Apps" empty={<p className="text-xs text-muted-foreground">No apps yet.</p>} items={(usage?.apps ?? []).map((a) => ({
              key: a.app, name: a.app, category: a.category, seconds: a.total_seconds, kind: "app" as const,
              tabs: a.is_browser ? (usage?.domains ?? []).filter((d) => d.browser === a.app).map((d) => ({ key: d.domain, name: d.domain, category: d.category, seconds: d.total_seconds, kind: "domain" as const })) : [],
            }))} onOpen={setDetail} />
            <UsageList label="Websites" empty={<SitesEmpty compact />} items={(usage?.domains ?? []).map((d) => ({ key: d.domain, name: d.domain, category: d.category, seconds: d.total_seconds, kind: "domain" as const }))} onOpen={setDetail} />
          </div>
          <div className="mt-4 shrink-0 border-t pt-3">
            <Bars bars={hours} className="h-14 gap-1" ariaLabel="Time by hour of the day" />
          </div>
          <SessionsDialog detail={detail} from={today.from} to={today.to} onClose={() => setDetail(null)} />
        </>
      )}
    </Tile>
  );
}

interface Item { key: string; name: string; category: string; seconds: number; kind: "app" | "domain"; /** Sites seen in this browser, with their time. */ tabs?: Item[] }

/** Every app or site with its time, longest first; click for the active spans. */
function UsageList({ label, items, empty, onOpen }: { label: string; items: Item[]; empty: React.ReactNode; onOpen: (d: Detail) => void }) {
  const max = items[0]?.seconds ?? 1;
  const total = items.reduce((n, i) => n + i.seconds, 0);
  return (
    <section aria-label={label} className="flex min-h-0 flex-col">
      <h3 className="mb-2 flex items-center gap-2 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
        {label}
        <span className="rounded-full bg-muted px-1.5 py-px text-[10px] font-medium tabular-nums normal-case tracking-normal">{items.length}</span>
        {items.length > 0 && <span className="ml-auto text-[11px] font-medium normal-case tracking-normal tabular-nums">{formatDuration(total)}</span>}
      </h3>
      {items.length === 0 ? (
        empty
      ) : (
        <div className="relative min-h-0 flex-1">
          <ul className="slim-scroll absolute inset-0 -mx-1 overflow-y-auto pb-4">
            {items.map((it) => (
              <li key={it.key}>
                <Row it={it} max={max} onOpen={onOpen} />
                {it.tabs && it.tabs.length > 0 && (
                  <ul aria-label={`Sites in ${it.name}`} className="mb-1 ml-5 border-l pl-2">
                    {it.tabs.map((t) => <li key={t.key}><Row it={t} max={it.seconds} onOpen={onOpen} small /></li>)}
                  </ul>
                )}
              </li>
            ))}
          </ul>
          {/* A soft fade instead of a scrollbar's edge: more rows are below. */}
          <span aria-hidden className="pointer-events-none absolute inset-x-0 bottom-0 h-5 bg-gradient-to-t from-card to-transparent" />
        </div>
      )}
    </section>
  );
}

function Row({ it, max, onOpen, small }: { it: Item; max: number; onOpen: (d: Detail) => void; small?: boolean }) {
  const style = categoryStyle(it.category);
  return (
    <button type="button" onClick={() => onOpen({ kind: it.kind, name: it.name, category: it.category })}
      aria-label={`${it.name}, ${formatDuration(it.seconds)}. Show when you were active`} title={`${it.name} · ${it.category}`}
      className="flex w-full items-center gap-2.5 rounded-lg px-1.5 py-1.5 text-left outline-none transition-colors hover:bg-accent/50 focus-visible:ring-2 focus-visible:ring-ring">
      <span aria-hidden className={cn("grid shrink-0 place-items-center rounded-md font-semibold uppercase", small ? "size-5 text-[10px]" : "size-7 text-[11px]", style.tile)}>{it.name.charAt(0)}</span>
      <span className="min-w-0 flex-1">
        <span className="flex items-baseline gap-2">
          <span className={cn("truncate", small ? "text-xs text-muted-foreground" : "text-[13px] font-medium")}>{it.name}</span>
          <span className={cn("ml-auto shrink-0 font-semibold tabular-nums", small ? "text-[11px]" : "text-[12px]")}>{formatDuration(it.seconds)}</span>
        </span>
        <span className="mt-1.5 block h-1.5 overflow-hidden rounded-full bg-muted">
          <span className={cn("block h-full rounded-full", style.bar)} style={{ width: `${Math.max(3, (it.seconds / Math.max(1, max)) * 100)}%` }} />
        </span>
      </span>
    </button>
  );
}
