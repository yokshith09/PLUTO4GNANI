import { useState } from "react";
import { ChevronRight, Globe, ShieldCheck } from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { Tile } from "@/features/shell/PageHeader";
import { useTrackingStatus, useUsage } from "@/hooks/usePluto";
import { useSetting } from "@/hooks/useTheme";
import { formatDuration } from "@/lib/time";
import { cn } from "@/lib/utils";
import type { AppUsage, DomainUsage } from "@/ipc";
import { categoryStyle } from "./categories";
import { SitesEmpty } from "./SitesEmpty";
import { SessionsDialog, type Detail } from "./SessionsDialog";

interface Props {
  from: number;
  to: number;
  /** Limit to one category (the Time page filter). */
  category?: string | null;
  className?: string;
  title?: string;
  /** Rows to show before scrolling. */
  scroll?: boolean;
  /** "apps" groups browsers over their sites; "sites" lists every site on its own. */
  view?: "apps" | "sites";
}

function Row({ name, category, seconds, max, sessions, expandable, open, onToggle, onOpen, indent, tracker }: {
  name: string; category: string; seconds: number; max: number; sessions: number;
  expandable?: boolean; tracker?: boolean; open?: boolean; onToggle?: () => void; onOpen: () => void; indent?: boolean;
}) {
  const style = categoryStyle(category);
  return (
    <li className={cn("group flex items-center gap-1", indent && "pl-9")}>
      {expandable ? (
        <button type="button" aria-label={`${open ? "Hide" : "Show"} sites in ${name}`} aria-expanded={open} onClick={onToggle}
          className="grid size-6 shrink-0 place-items-center rounded text-muted-foreground outline-none hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring">
          <ChevronRight className={cn("size-3.5 transition-transform", open && "rotate-90")} />
        </button>
      ) : (
        !indent && <span className="size-6 shrink-0" />
      )}
      <button type="button" onClick={onOpen} aria-label={`${name}, ${formatDuration(seconds)}. Show when you were active`}
        className="flex min-w-0 flex-1 items-center gap-3 rounded-lg px-2 py-2 text-left outline-none hover:bg-accent/50 focus-visible:ring-2 focus-visible:ring-ring">
        <span className={cn("grid size-8 shrink-0 place-items-center rounded-md text-[13px] font-semibold", style.tile)}>
          {indent ? <Globe className="size-4" /> : name.charAt(0).toUpperCase()}
        </span>
        <span className="min-w-0 flex-1">
          <span className="flex items-baseline gap-2">
            <span className="truncate text-[13px] font-medium">{name}</span>
            <span className="text-[11px] text-muted-foreground">{category}</span>
            {tracker && <span className="rounded bg-amber-500/15 px-1.5 text-[10px] font-medium text-amber-700 dark:text-amber-400">Known tracker</span>}
            <span className="ml-auto text-[13px] font-medium tabular-nums">{formatDuration(seconds)}</span>
          </span>
          <span className="mt-1.5 block h-1 overflow-hidden rounded-full bg-muted">
            <span className={cn("block h-full rounded-full", style.bar)} style={{ width: `${Math.max(2, (seconds / Math.max(1, max)) * 100)}%` }} />
          </span>
          <span className="mt-1 block text-[11px] text-muted-foreground">{sessions} {sessions === 1 ? "session" : "sessions"}</span>
        </span>
      </button>
    </li>
  );
}

/** Where the time went, per app. Browsers expand into sites; any row opens its active spans. */
export function TimeCard({ from, to, category = null, className, title = "Where your time went", scroll = true, view = "apps" }: Props) {
  const { data: status } = useTrackingStatus();
  const { data: usage, isLoading } = useUsage(from, to);
  const [, setEnabled] = useSetting("tracking.apps");
  const [open, setOpen] = useState<Record<string, boolean>>({});
  const [detail, setDetail] = useState<Detail | null>(null);

  const sitesView = view === "sites";
  const apps = (usage?.apps ?? []).filter((a) => !category || a.category === category);
  const sites = (usage?.domains ?? []).filter((d) => !category || d.category === category);
  const domainsOf = (a: AppUsage): DomainUsage[] => (usage?.domains ?? []).filter((d) => d.browser === a.app && (!category || d.category === category));
  const total = sitesView ? sites.reduce((n, d) => n + d.total_seconds, 0) : apps.reduce((n, a) => n + a.total_seconds, 0);
  const max = (sitesView ? sites[0]?.total_seconds : apps[0]?.total_seconds) ?? 1;
  const rows = sitesView ? sites.length : apps.length;
  const tracking = status?.enabled ?? false;

  return (
    <Tile
      className={className} title={title} {...(tracking && total > 0 ? { count: `${rows} ${sitesView ? "sites" : "apps"}` } : {})}
      action={tracking && total > 0 ? <span className="text-[13px] font-semibold tabular-nums">{formatDuration(total)}</span> : undefined}
      bodyClassName={cn("p-2", !scroll && "overflow-visible")}
    >
      {status && status.supported === false ? (
        <p className="px-4 py-10 text-center text-[13px] text-muted-foreground">App time tracking isn’t available on this system yet.</p>
      ) : !tracking ? (
        <div className="mx-auto flex max-w-sm flex-col items-center px-4 py-8 text-center">
          <span className="mb-3 grid size-10 place-items-center rounded-lg bg-muted text-muted-foreground"><ShieldCheck className="size-5" /></span>
          <p className="text-sm font-medium">Track where your time goes</p>
          <p className="mt-1 text-[13px] leading-relaxed text-muted-foreground">
            Pluto can record which app is in front and for how long. It stays on this computer, never stores window titles or full web addresses, and you can delete it any time.
          </p>
          <label className="mt-4 flex items-center gap-2.5 text-[13px] font-medium">
            <Switch checked={false} onCheckedChange={(v) => void setEnabled(v)} aria-label="Turn on app tracking" />
            Turn on app tracking
          </label>
        </div>
      ) : isLoading ? (
        <div className="space-y-3 p-2" aria-hidden>{[0, 1, 2, 3].map((i) => <div key={i} className="h-10 animate-pulse rounded-lg bg-muted" />)}</div>
      ) : rows === 0 ? (
        sitesView ? <SitesEmpty /> : <p className="px-4 py-10 text-center text-[13px] text-muted-foreground">Nothing recorded for this period yet.</p>
      ) : sitesView ? (
        <ul className="space-y-0.5">
          {sites.map((d) => (
            <Row key={d.domain} name={d.domain} category={d.category} seconds={d.total_seconds} max={max} sessions={d.sessions} tracker={d.is_tracker}
              onOpen={() => setDetail({ kind: "domain", name: d.domain, category: d.category })} />
          ))}
        </ul>
      ) : (
        <ul className="space-y-0.5">
          {apps.map((a) => {
            const sites = a.is_browser ? domainsOf(a) : [];
            return (
              <li key={a.app}>
                <ul>
                  <Row
                    name={a.app} category={a.category} seconds={a.total_seconds} max={max} sessions={a.sessions}
                    expandable={a.is_browser && sites.length > 0} open={!!open[a.app]} onToggle={() => setOpen((o) => ({ ...o, [a.app]: !o[a.app] }))}
                    onOpen={() => setDetail({ kind: "app", name: a.app, category: a.category })}
                  />
                  {open[a.app] && sites.map((d) => (
                    <Row key={d.domain} indent name={d.domain} category={d.category} seconds={d.total_seconds} max={a.total_seconds} sessions={d.sessions} tracker={d.is_tracker}
                      onOpen={() => setDetail({ kind: "domain", name: d.domain, category: d.category })} />
                  ))}
                </ul>
              </li>
            );
          })}
        </ul>
      )}
      <SessionsDialog detail={detail} from={from} to={to} onClose={() => setDetail(null)} />
    </Tile>
  );
}
