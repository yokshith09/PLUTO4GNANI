import { useEffect, useState } from "react";
import { X } from "lucide-react";
import { dismissNotice, watchNotices, type Notice } from "@/lib/notify";
import { cn } from "@/lib/utils";

const TONE: Record<Notice["kind"], string> = { success: "border-l-success", info: "border-l-primary", warning: "border-l-primary", error: "border-l-destructive" };

/** Messages in the dashboard, used only while the pet is hidden (otherwise they go above the pet). */
export function Notices() {
  const [list, setList] = useState<Notice[]>([]);
  useEffect(() => watchNotices(setList), []);
  if (list.length === 0) return null;
  return (
    <ol aria-label="Messages" className="pointer-events-none fixed bottom-4 right-4 z-50 flex w-80 flex-col gap-2">
      {list.map((n) => (
        <li key={n.id} role={n.kind === "error" ? "alert" : "status"}
          className={cn("pointer-events-auto flex items-center gap-2 rounded-lg border border-l-4 bg-popover px-3 py-2.5 text-[13px] text-popover-foreground shadow-lg", TONE[n.kind])}>
          <span className="min-w-0 flex-1">{n.text}</span>
          {n.action && (
            <button type="button" onClick={() => { n.action?.onClick(); dismissNotice(n.id); }} className="rounded px-2 py-1 text-xs font-medium text-primary hover:bg-accent">{n.action.label}</button>
          )}
          <button type="button" aria-label="Dismiss" onClick={() => dismissNotice(n.id)} className="grid size-6 place-items-center rounded text-muted-foreground hover:bg-accent"><X className="size-3.5" /></button>
        </li>
      ))}
    </ol>
  );
}
