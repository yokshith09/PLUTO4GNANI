import type { TimeRange } from "@/ipc";

const HOUR = 3_600_000;
const DAY = 86_400_000;

export interface SessionStats {
  /** Seconds, all sessions. */
  total: number;
  count: number;
  /** Longest single session, seconds. */
  longest: number;
  longestStartedAt: number | null;
  /** Mean session length, seconds (0 with none). */
  average: number;
  /** Seconds in each hour of the local day, summed over the period. */
  hourly: number[];
  /** The hour with the most time, or null with none. */
  peakHour: number | null;
  /** Seconds per local day (key = local midnight in ms), earliest first. */
  daily: { day: number; seconds: number; sessions: number }[];
}

const midnight = (ms: number) => new Date(new Date(ms).setHours(0, 0, 0, 0)).getTime();

/** What one app's or site's spans add up to: totals, the longest, hours of the day and days. Pure, so it is tested. */
export function sessionStats(spans: TimeRange[]): SessionStats {
  const hourly = new Array<number>(24).fill(0);
  const days = new Map<number, { seconds: number; sessions: number }>();
  let total = 0;
  let longest = 0;
  let longestStartedAt: number | null = null;
  for (const r of spans) {
    const secs = Math.max(0, (r.ended_at - r.started_at) / 1000);
    total += secs;
    if (secs > longest) { longest = secs; longestStartedAt = r.started_at; }
    // Split at local hour boundaries so a long session fills every hour it covers.
    let t = r.started_at;
    while (t < r.ended_at) {
      const d = new Date(t);
      const intoHour = (d.getMinutes() * 60 + d.getSeconds()) * 1000 + d.getMilliseconds();
      const end = Math.min(r.ended_at, t + HOUR - intoHour);
      hourly[d.getHours()] = (hourly[d.getHours()] ?? 0) + (end - t) / 1000;
      const key = midnight(t);
      const day = days.get(key) ?? { seconds: 0, sessions: 0 };
      day.seconds += (end - t) / 1000;
      days.set(key, day);
      t = end;
    }
    const first = midnight(r.started_at);
    const day = days.get(first) ?? { seconds: 0, sessions: 0 };
    day.sessions += 1;
    days.set(first, day);
  }
  const max = Math.max(...hourly);
  return {
    total, count: spans.length, longest, longestStartedAt,
    average: spans.length ? total / spans.length : 0,
    hourly,
    peakHour: max > 0 ? hourly.indexOf(max) : null,
    daily: [...days.entries()].sort((a, b) => a[0] - b[0]).map(([day, v]) => ({ day, ...v })),
  };
}

/** "9 AM", "12 PM": an hour of the day for people. */
export function hourName(h: number): string {
  return `${h % 12 === 0 ? 12 : h % 12} ${h < 12 ? "AM" : "PM"}`;
}

export { DAY };
