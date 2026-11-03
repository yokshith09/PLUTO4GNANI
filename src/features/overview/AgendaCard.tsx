import { CalendarDays } from "lucide-react";
import { useTileHandle } from "@/features/shell/TileHandle";
import { cn } from "@/lib/utils";
import { TodayAgenda, TodayComposer } from "./TodayAgenda";

/** The main agenda: tasks and reminders for today, with a quick add. */
export function AgendaCard({ className }: { className?: string }) {
  const handle = useTileHandle();
  const week = new Date().toLocaleDateString([], { weekday: "long", month: "long", day: "numeric" });
  return (
    <section aria-label="Today’s agenda" className={cn("flex min-h-0 flex-col overflow-hidden rounded-xl border bg-card", className)}>
      <header className="flex h-12 shrink-0 items-center gap-2 border-b px-5">
        <CalendarDays className="size-4 text-primary" />
        <h2 className="text-[13px] font-semibold">Today</h2>
        <span className="truncate text-xs text-muted-foreground">{week}</span>
        {handle && <div className="ml-auto">{handle}</div>}
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto px-3 py-4"><TodayAgenda /></div>
      <footer className="shrink-0 border-t px-4 py-3"><TodayComposer /></footer>
    </section>
  );
}
