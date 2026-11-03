import { Radar } from "lucide-react";
import { Tile } from "@/features/shell/PageHeader";
import { useUsage } from "@/hooks/usePluto";
import { formatDuration } from "@/lib/time";
import { categoryStyle } from "./categories";
import { cn } from "@/lib/utils";

/**
 * Which of the sites you visited are themselves well-known advertising or tracking companies.
 * Pluto reads only the site name in the address bar, so it cannot see what a page loads in the
 * background. A site that is not on the list is "not flagged", never "clean" (DECISIONS U16).
 */
export function PrivacyRadar({ from, to, enabled, className }: { from: number; to: number; enabled: boolean; className?: string }) {
  const { data: usage } = useUsage(from, to);
  const sites = usage?.domains ?? [];
  const flagged = sites.filter((d) => d.is_tracker);
  const flaggedSeconds = flagged.reduce((n, d) => n + d.total_seconds, 0);
  const byCategory = new Map<string, number>();
  for (const d of sites) byCategory.set(d.category, (byCategory.get(d.category) ?? 0) + d.total_seconds);
  const categories = [...byCategory.entries()].sort((a, b) => b[1] - a[1]).slice(0, 5);
  const top = categories[0]?.[1] ?? 1;

  return (
    <Tile className={className} title="Privacy radar" count={enabled && sites.length ? `${sites.length} sites` : undefined} bodyClassName="p-4">
      {!enabled ? (
        <p className="py-6 text-center text-[13px] text-muted-foreground">Turn on website tracking to see which sites you visit.</p>
      ) : sites.length === 0 ? (
        <p className="py-6 text-center text-[13px] text-muted-foreground">No sites recorded for this period yet.</p>
      ) : (
        <div className="space-y-4">
          <div className="flex items-start gap-3">
            <span className={cn("grid size-9 shrink-0 place-items-center rounded-lg", flagged.length ? "bg-amber-500/15 text-amber-700 dark:text-amber-400" : "bg-muted text-muted-foreground")}>
              <Radar className="size-4" />
            </span>
            <div>
              <p className="text-sm font-medium">
                {flagged.length === 0
                  ? "No visited site is on the known-tracker list"
                  : `Known ad or tracking companies: ${flagged.length} of ${sites.length} visited sites`}
              </p>
              {flagged.length > 0 && <p className="text-xs text-muted-foreground">{formatDuration(flaggedSeconds)} spent on them in this period.</p>}
            </div>
          </div>
          {flagged.length > 0 && (
            <ul className="divide-y rounded-lg border" aria-label="Known trackers you visited">
              {flagged.map((d) => (
                <li key={d.domain} className="flex items-center gap-2 px-3 py-2 text-[13px]">
                  <span className="min-w-0 flex-1 truncate font-medium">{d.domain}</span>
                  <span className="text-[11px] text-muted-foreground">{d.category}</span>
                  <span className="w-14 text-right tabular-nums">{formatDuration(d.total_seconds)}</span>
                </li>
              ))}
            </ul>
          )}
          <div>
            <p className="mb-2 text-xs font-medium text-muted-foreground">Sites by category</p>
            <ul className="space-y-1.5">
              {categories.map(([c, v]) => (
                <li key={c} className="flex items-center gap-2 text-[13px]">
                  <span className="w-24 truncate">{c}</span>
                  <span className="h-1.5 flex-1 overflow-hidden rounded-full bg-muted">
                    <span className={cn("block h-full rounded-full", categoryStyle(c).bar)} style={{ width: `${Math.max(2, (v / top) * 100)}%` }} />
                  </span>
                  <span className="w-14 text-right tabular-nums text-muted-foreground">{formatDuration(v)}</span>
                </li>
              ))}
            </ul>
          </div>
          <p className="text-[11px] leading-relaxed text-muted-foreground">
            This only flags sites you opened that are themselves ad or tracking companies. Pluto does not see what a page loads in the background, so a site that isn’t flagged is not proven clean.
          </p>
        </div>
      )}
    </Tile>
  );
}
