import { cn } from "@/lib/utils";

/** One-click choices under a date or time field (FR-45). The field stays for anything else. */
export function Presets({ options, value, onPick, label }: { options: { label: string; value: string }[]; value: string; onPick: (v: string) => void; label: string }) {
  return (
    <div className="mt-1.5 flex flex-wrap gap-1" role="group" aria-label={label}>
      {options.map((o) => (
        <button key={o.label} type="button" onClick={() => onPick(o.value)} aria-pressed={value === o.value}
          className={cn("h-6 rounded-md border px-2 text-[11px] outline-none focus-visible:ring-2 focus-visible:ring-ring",
            value === o.value ? "border-primary bg-primary/15 text-foreground" : "border-border text-muted-foreground hover:border-foreground/30 hover:text-foreground")}>
          {o.label}
        </button>
      ))}
    </div>
  );
}
