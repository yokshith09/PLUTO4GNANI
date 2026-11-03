import { useQuery } from "@tanstack/react-query";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { Badge } from "@/components/ui/badge";
import { ipc, type TimeRange } from "@/ipc";
import { clockTime, formatDuration } from "@/lib/time";
import { Bars, type Bar } from "@/features/charts/Bars";
import { categoryStyle } from "./categories";
import { hourName, sessionStats } from "./sessionStats";

export interface Detail { kind: "app" | "domain"; name: string; category: string }

const DAY = 86_400_000;
const dayStart = (ms: number) => new Date(new Date(ms).setHours(0, 0, 0, 0)).getTime();

function dayTitle(start: number, now = Date.now()) {
  const diff = Math.round((dayStart(now) - start) / DAY);
  if (diff === 0) return "Today";
  if (diff === 1) return "Yesterday";
  return new Date(start).toLocaleDateString([], { weekday: "long", month: "short", day: "numeric" });
}

/** When you were active in one app or site: every span, grouped by day, with a 24-hour bar. */
export function SessionsDialog({ detail, from, to, onClose }: { detail: Detail | null; from: number; to: number; onClose: () => void }) {
  const { data = [], isLoading } = useQuery({
    queryKey: ["usage", "sessions", detail?.kind, detail?.name, from, to],
    queryFn: () => (detail?.kind === "domain" ? ipc.usageDomainSessions(detail.name, from, to) : ipc.usageAppSessions(detail?.name ?? "", from, to)),
    enabled: detail !== null,
  });
  const style = categoryStyle(detail?.category ?? "Other");
  const total = data.reduce((n, r) => n + (r.ended_at - r.started_at) / 1000, 0);
  const byDay = new Map<number, TimeRange[]>();
  for (const r of data) byDay.set(dayStart(r.started_at), [...(byDay.get(dayStart(r.started_at)) ?? []), r]);
  const days = [...byDay.entries()].sort((a, b) => b[0] - a[0]);
  const stats = sessionStats(data);
  const hourBars: Bar[] = stats.hourly.map((v, h) => ({ key: String(h), value: v, label: h % 6 === 0 ? String(h) : "", title: `${hourName(h)}: ${formatDuration(v)}`, active: h === stats.peakHour }));
  const dayBars: Bar[] = stats.daily.map((d) => ({
    key: String(d.day), value: d.seconds, label: stats.daily.length > 8 ? String(new Date(d.day).getDate()) : new Date(d.day).toLocaleDateString([], { weekday: "narrow" }), active: true,
    title: `${new Date(d.day).toLocaleDateString([], { weekday: "long", month: "short", day: "numeric" })}: ${formatDuration(d.seconds)}, ${d.sessions} ${d.sessions === 1 ? "session" : "sessions"}`,
  }));
  const first = data[0]?.started_at;
  const last = data[data.length - 1]?.ended_at;

  return (
    <Dialog open={detail !== null} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-2xl gap-0 overflow-hidden">
        <div className="flex items-center gap-3 border-b p-5 pr-12">
          <span className={`grid size-10 place-items-center rounded-lg text-base font-semibold ${style.tile}`}>{detail?.name.charAt(0).toUpperCase()}</span>
          <div className="min-w-0">
            <DialogTitle className="truncate text-base font-semibold">{detail?.name}</DialogTitle>
            <DialogDescription className="flex items-center gap-2 text-xs text-muted-foreground">
              <Badge>{detail?.category}</Badge>
              {data.length > 0 ? <>{formatDuration(total)} in {data.length} {data.length === 1 ? "session" : "sessions"}</> : "No activity in this period"}
            </DialogDescription>
          </div>
        </div>
        <div className="max-h-[60vh] overflow-y-auto p-5">
          {isLoading ? (
            <div className="space-y-2" aria-hidden>{[0, 1, 2].map((i) => <div key={i} className="h-9 animate-pulse rounded-md bg-muted" />)}</div>
          ) : data.length === 0 ? (
            <p className="py-8 text-center text-[13px] text-muted-foreground">Nothing was recorded for {detail?.name} in this period.</p>
          ) : (
            <>
              <ul aria-label="Summary" className="mb-5 grid grid-cols-2 gap-3 sm:grid-cols-4">
                {[
                  ["Total", formatDuration(stats.total)],
                  ["Sessions", String(stats.count)],
                  ["Longest", formatDuration(stats.longest)],
                  ["Average", formatDuration(stats.average)],
                ].map(([label, value]) => (
                  <li key={label} className="rounded-lg border bg-muted/20 px-3 py-2.5">
                    <p className="text-[11px] font-medium text-muted-foreground">{label}</p>
                    <p className="mt-1 text-lg font-semibold leading-none tabular-nums">{value}</p>
                  </li>
                ))}
              </ul>
              <p className="mb-5 text-[13px] text-muted-foreground">
                {stats.peakHour !== null && <>Most used around <span className="font-semibold text-foreground">{hourName(stats.peakHour)}</span>. </>}
                {first && last && days.length === 1 && <>Active from <span className="font-semibold text-foreground">{clockTime(first)}</span> to <span className="font-semibold text-foreground">{clockTime(last)}</span>.</>}
              </p>
              <section aria-label="When you use it" className="mb-5">
                <h3 className="mb-2 text-xs font-medium">When you use it <span className="font-normal text-muted-foreground">· by hour of the day</span></h3>
                <Bars bars={hourBars} className="h-28 gap-1" ariaLabel={`Time by hour of the day. Most used: ${stats.peakHour === null ? "none" : hourName(stats.peakHour)}`} />
              </section>
              {days.length > 1 && (
                <section aria-label="By day" className="mb-5">
                  <h3 className="mb-2 text-xs font-medium">By day</h3>
                  <Bars bars={dayBars} className="h-28" average={stats.total / days.length} ariaLabel={`Time per day: ${dayBars.map((b) => b.value).join(", ")}`} />
                </section>
              )}
              <h3 className="mb-2 text-xs font-medium">Sessions</h3>
              <div className="space-y-5">
                {days.map(([start, list]) => (
                  <section key={start} aria-label={dayTitle(start)}>
                    <div className="mb-1.5 flex items-center justify-between text-xs">
                      <h3 className="font-medium">{dayTitle(start)}</h3>
                      <span className="tabular-nums text-muted-foreground">{formatDuration(list.reduce((n, r) => n + (r.ended_at - r.started_at) / 1000, 0))}</span>
                    </div>
                    <div className="relative h-3 overflow-hidden rounded-[4px] bg-muted" role="img" aria-label="Active spans across the day">
                      {list.map((r) => (
                        <span key={r.started_at} className={`absolute top-0 h-full ${style.bar}`}
                          style={{ left: `${((r.started_at - start) / DAY) * 100}%`, width: `${Math.max(0.4, ((r.ended_at - r.started_at) / DAY) * 100)}%` }} />
                      ))}
                    </div>
                    <div className="mt-1 flex justify-between text-[10px] text-muted-foreground"><span>12 AM</span><span>6 AM</span><span>12 PM</span><span>6 PM</span><span>12 AM</span></div>
                    <ul className="mt-2 divide-y rounded-lg border">
                      {list.map((r) => (
                        <li key={r.started_at} className="flex items-center gap-3 px-3 py-1.5 text-[13px]">
                          <span className={`size-1.5 rounded-full ${style.dot}`} />
                          <span className="tabular-nums">{clockTime(r.started_at)} – {clockTime(r.ended_at)}</span>
                          <span className="ml-auto tabular-nums text-muted-foreground">{formatDuration((r.ended_at - r.started_at) / 1000)}</span>
                        </li>
                      ))}
                    </ul>
                  </section>
                ))}
              </div>
            </>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
