import { dayLabel } from "@/lib/dates";
import { formatDuration } from "@/lib/time";

export interface AnalyticsFacts {
  /** Seconds in the shown period, and in the same-length period just before it. */
  total: number;
  prevTotal: number;
  days: number;
  /** Category and seconds, biggest first. */
  categories: [string, number][];
  /** Seconds in each hour of the day, summed over the period. */
  hourly: number[];
  perDay: { date: Date; seconds: number }[];
  topApp?: { app: string; seconds: number } | undefined;
}

const pct = (part: number, whole: number) => Math.round((part / whole) * 100);
const hourText = (h: number) => `${String(h).padStart(2, "0")}:00`;

/**
 * A few plain sentences about the period, from the numbers already on screen. Neutral wording:
 * a lower number is not worse, a higher one is not better (no-guilt rule), so there is no verdict,
 * only what happened.
 */
export function insights(f: AnalyticsFacts): string[] {
  if (f.total <= 0) return [];
  const out: string[] = [];
  const unit = f.days === 1 ? "the day before" : `the previous ${f.days} days`;
  if (f.prevTotal > 0) {
    const change = Math.round(((f.total - f.prevTotal) / f.prevTotal) * 100);
    if (Math.abs(change) >= 10) out.push(`Screen time was ${Math.abs(change)}% ${change > 0 ? "higher" : "lower"} than ${unit}.`);
    else out.push(`Screen time was about the same as ${unit}.`);
  }
  const [topCat] = f.categories;
  if (topCat) out.push(`${topCat[0]} took the biggest share of your time: ${pct(topCat[1], f.total)}%.`);
  if (f.topApp) out.push(`${f.topApp.app} was your most used app, at ${formatDuration(f.topApp.seconds)}.`);
  const peak = Math.max(...f.hourly);
  if (peak > 0) {
    const h = f.hourly.indexOf(peak);
    out.push(`You were busiest around ${hourText(h)}.`);
  }
  if (f.days > 1) {
    const best = f.perDay.reduce((m, d) => (d.seconds > m.seconds ? d : m), f.perDay[0] ?? { date: new Date(0), seconds: 0 });
    if (best.seconds > 0) out.push(`Your longest day was ${dayLabel(best.date)}, at ${formatDuration(best.seconds)}.`);
  }
  return out.slice(0, 4);
}

/** "+12%", "-8%" or null when there is nothing to compare with. */
export function changeLabel(total: number, prevTotal: number): string | null {
  if (prevTotal <= 0 || total <= 0) return null;
  const change = Math.round(((total - prevTotal) / prevTotal) * 100);
  return `${change > 0 ? "+" : change < 0 ? "−" : ""}${Math.abs(change)}%`;
}
