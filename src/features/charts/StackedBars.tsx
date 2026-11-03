import { cn } from "@/lib/utils";

export interface StackedBar {
  key: string;
  label: string;
  title: string;
  /** Pieces from the bottom up, each with the colour class it is drawn in. */
  parts: { name: string; seconds: number; cls: string }[];
  active?: boolean;
}

/** One column per day, split by category. Heights share one scale; a day with nothing shows a thin line. */
export function StackedBars({ bars, className, ariaLabel }: { bars: StackedBar[]; className?: string; ariaLabel: string }) {
  const totals = bars.map((b) => b.parts.reduce((n, p) => n + p.seconds, 0));
  const max = Math.max(1, ...totals);
  return (
    <div className={cn("flex items-end gap-2", className)} role="img" aria-label={ariaLabel}>
      {bars.map((b, i) => (
        <div key={b.key} className="group flex h-full min-w-0 flex-1 flex-col items-center gap-1.5" title={b.title}>
          <div className="flex w-full flex-1 items-end">
            <div className="flex w-full flex-col-reverse overflow-hidden rounded-[3px]" style={{ height: `${(totals[i] ?? 0) === 0 ? 2 : Math.max(6, ((totals[i] ?? 0) / max) * 100)}%` }}>
              {(totals[i] ?? 0) === 0
                ? <span className="h-full bg-foreground/15" />
                : b.parts.filter((p) => p.seconds > 0).map((p) => (
                  <span key={p.name} data-part={p.name} className={cn("w-full", p.cls, !b.active && "opacity-80")} style={{ height: `${(p.seconds / (totals[i] ?? 1)) * 100}%` }} />
                ))}
            </div>
          </div>
          <span className={cn("text-[10px]", b.active ? "font-medium text-foreground" : "text-muted-foreground")}>{b.label}</span>
        </div>
      ))}
    </div>
  );
}
