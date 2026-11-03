import { cn } from "@/lib/utils";

export interface Bar { key: string; value: number; label: string; title: string; active?: boolean }

/** Vertical bars with an optional dashed average line. The active bar uses the accent. */
export function Bars({
  bars, average, className, onSelect, selectedKey, ariaLabel,
}: { bars: Bar[]; average?: number; className?: string; onSelect?: (key: string) => void; selectedKey?: string; ariaLabel: string }) {
  const max = Math.max(1, ...bars.map((b) => b.value), average ?? 0);
  return (
    <div className={cn("relative flex items-end gap-2", className)} role="img" aria-label={ariaLabel}>
      {average !== undefined && average > 0 && (
        <div className="pointer-events-none absolute inset-x-0 border-t border-dashed border-foreground/30" style={{ bottom: `calc(${(average / max) * 100}% * (100% - 20px) / 100% + 20px)` }} />
      )}
      {bars.map((b) => {
        const selected = selectedKey ? selectedKey === b.key : b.active;
        const inner = (
          <>
            <div className="flex w-full flex-1 items-end">
              <div
                title={b.title}
                className={cn("w-full rounded-[3px] transition-colors", selected ? "bg-primary" : "bg-foreground/15 group-hover:bg-foreground/25")}
                style={{ height: `${b.value === 0 ? 2 : Math.max(6, (b.value / max) * 100)}%` }}
              />
            </div>
            <span className={cn("text-[10px]", selected ? "font-medium text-foreground" : "text-muted-foreground")}>{b.label}</span>
          </>
        );
        return onSelect ? (
          <button key={b.key} type="button" onClick={() => onSelect(b.key)} aria-label={b.title} className="group flex h-full min-w-0 flex-1 flex-col items-center gap-1.5 rounded outline-none focus-visible:ring-2 focus-visible:ring-ring">
            {inner}
          </button>
        ) : (
          <div key={b.key} className="group flex h-full min-w-0 flex-1 flex-col items-center gap-1.5">{inner}</div>
        );
      })}
    </div>
  );
}
