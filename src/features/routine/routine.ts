import { ymd } from "@/lib/dates";

/**
 * Daily routine (FR-85): a checklist that starts fresh every day. Pure functions only; the hook
 * (`useRoutine`) stores the state in preferences under `ui.routine`.
 */
export const MAX_ITEMS = 30;
export const TITLE_MAX = 60;
export const HISTORY_DAYS = 60;
export const DEFAULT_REMIND_AT = "18:00";

export interface RoutineItem { id: string; title: string }
export interface RoutineState {
  items: RoutineItem[];
  /** The day `done` belongs to (local YYYY-MM-DD). */
  day: string;
  done: string[];
  /** Past days: day → ids done that day (kept for the weekly count, FR-90). */
  history: Record<string, string[]>;
  remind: { on: boolean; at: string };
  /** The day the pending reminder was last shown, so it fires at most once a day. */
  remindedOn: string | null;
}

export const emptyRoutine = (day: string): RoutineState => ({
  items: [], day, done: [], history: {}, remind: { on: true, at: DEFAULT_REMIND_AT }, remindedOn: null,
});

/** Read whatever is stored; anything broken falls back to an empty routine. */
export function parseRoutine(raw: unknown, today: string): RoutineState {
  if (!raw || typeof raw !== "object") return emptyRoutine(today);
  const r = raw as Partial<RoutineState>;
  const items = Array.isArray(r.items)
    ? r.items.filter((i): i is RoutineItem => !!i && typeof i.id === "string" && typeof i.title === "string").slice(0, MAX_ITEMS)
    : [];
  const at = typeof r.remind?.at === "string" && /^\d{2}:\d{2}$/.test(r.remind.at) ? r.remind.at : DEFAULT_REMIND_AT;
  return rollOver({
    items,
    day: typeof r.day === "string" ? r.day : today,
    done: Array.isArray(r.done) ? r.done.filter((x): x is string => typeof x === "string") : [],
    history: r.history && typeof r.history === "object" ? r.history : {},
    remind: { on: r.remind?.on !== false, at },
    remindedOn: typeof r.remindedOn === "string" ? r.remindedOn : null,
  }, today);
}

/** A new day: yesterday's ticks move to history (last 60 days) and today starts empty. */
export function rollOver(s: RoutineState, today: string): RoutineState {
  if (s.day === today) return s;
  const history = { ...s.history, [s.day]: s.done };
  const keep = Object.keys(history).sort().slice(-HISTORY_DAYS);
  return { ...s, day: today, done: [], history: Object.fromEntries(keep.map((d) => [d, history[d] ?? []])) };
}

export const addItem = (s: RoutineState, title: string, id: string): RoutineState => {
  const t = title.trim().slice(0, TITLE_MAX);
  if (!t || s.items.length >= MAX_ITEMS) return s;
  return { ...s, items: [...s.items, { id, title: t }] };
};
export const renameItem = (s: RoutineState, id: string, title: string): RoutineState => {
  const t = title.trim().slice(0, TITLE_MAX);
  return t ? { ...s, items: s.items.map((i) => (i.id === id ? { ...i, title: t } : i)) } : s;
};
export const removeItem = (s: RoutineState, id: string): RoutineState =>
  ({ ...s, items: s.items.filter((i) => i.id !== id), done: s.done.filter((d) => d !== id) });
export const toggle = (s: RoutineState, id: string): RoutineState =>
  ({ ...s, done: s.done.includes(id) ? s.done.filter((d) => d !== id) : [...s.done, id] });

/** Items not ticked today. */
export const pending = (s: RoutineState): RoutineItem[] => s.items.filter((i) => !s.done.includes(i.id));

/**
 * On how many of the last 7 days (today included) this item was done (FR-90, no-guilt rule):
 * a missed day never wipes the count the way a streak would.
 */
export function weekCount(s: RoutineState, id: string, today: string): number {
  let n = s.done.includes(id) ? 1 : 0;
  const d = new Date(`${today}T12:00:00`);
  for (let i = 1; i < 7; i++) {
    d.setDate(d.getDate() - 1);
    if (s.history[ymd(d)]?.includes(id)) n += 1;
  }
  return n;
}

/** The reminder text, or null when there is nothing to say. */
export function pendingMessage(s: RoutineState): string | null {
  const n = pending(s).length;
  if (n === 0) return null;
  return `You have ${n} daily routine ${n === 1 ? "task" : "tasks"} pending`;
}

/** Should the reminder fire now? Only when on, past the time, something pending, and not yet today. */
export function shouldRemind(s: RoutineState, now: Date, today: string): boolean {
  if (!s.remind.on || s.remindedOn === today || pending(s).length === 0) return false;
  const [h, m] = s.remind.at.split(":").map(Number);
  return now.getHours() * 60 + now.getMinutes() >= (h ?? 18) * 60 + (m ?? 0);
}

/** Milliseconds until today's reminder time (0 if it has passed). */
export function msUntil(at: string, now: Date): number {
  const [h, m] = at.split(":").map(Number);
  const t = new Date(now);
  t.setHours(h ?? 18, m ?? 0, 0, 0);
  return Math.max(0, t.getTime() - now.getTime());
}
