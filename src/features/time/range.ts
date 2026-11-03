export type RangeKey = "today" | "yesterday" | "7d" | "30d";

export const RANGES: { key: RangeKey; label: string }[] = [
  { key: "today", label: "Today" },
  { key: "yesterday", label: "Yesterday" },
  { key: "7d", label: "7 days" },
  { key: "30d", label: "30 days" },
];

const startOfDay = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate());

/** `[from, to)` in ms for a range key, in local time. */
export function rangeBounds(key: RangeKey, now = new Date()): { from: number; to: number; days: number } {
  const today = startOfDay(now);
  const plus = (n: number) => new Date(today.getFullYear(), today.getMonth(), today.getDate() + n).getTime();
  switch (key) {
    case "today": return { from: today.getTime(), to: plus(1), days: 1 };
    case "yesterday": return { from: plus(-1), to: today.getTime(), days: 1 };
    case "7d": return { from: plus(-6), to: plus(1), days: 7 };
    case "30d": return { from: plus(-29), to: plus(1), days: 30 };
  }
}
