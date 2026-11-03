import { inQuietHours, type NudgeState } from "@/lib/nudges";

/**
 * Break and water nudges (owner 2026-10-27). Each has its own switch and its own gap, and they
 * repeat through the day, so they do **not** go through the daily nudge budget — four a day would
 * swallow them. They do keep quiet hours, and they only count time the person was really at the
 * computer when app tracking can say so.
 */
export type WellbeingKind = "break" | "water";

export interface WellbeingKindState {
  on: boolean;
  /** Minutes between nudges of this kind. */
  everyMin: number;
  /** When this kind was last shown, and the screen time on the clock then. */
  lastAt: number | null;
  lastScreenSec: number | null;
}

export type Wellbeing = Record<WellbeingKind, WellbeingKindState>;

export const KINDS: readonly WellbeingKind[] = ["break", "water"];
/** What each nudge says and how the pet plays it. The pet has no stretch or drink animation yet;
 * those come with the per-character rows, so for now each uses the nearest pose it has. */
export const SAYS: Record<WellbeingKind, { text: string; pose: "stow" | "wave"; everyMin: number }> = {
  break: { text: "You have been at it a while. Stretch your legs for a minute?", pose: "stow", everyMin: 50 },
  water: { text: "Time for a drink of water.", pose: "wave", everyMin: 90 },
};
export const EVERY_CHOICES = [20, 30, 45, 50, 60, 90, 120] as const;
const MIN_EVERY = 10;
const MAX_EVERY = 240;

export const emptyWellbeing = (): Wellbeing => ({
  break: { on: false, everyMin: SAYS.break.everyMin, lastAt: null, lastScreenSec: null },
  water: { on: false, everyMin: SAYS.water.everyMin, lastAt: null, lastScreenSec: null },
});

export function parseWellbeing(raw: unknown): Wellbeing {
  const r = (raw && typeof raw === "object" ? raw : {}) as Partial<Record<WellbeingKind, unknown>>;
  const one = (v: unknown, fallback: WellbeingKindState): WellbeingKindState => {
    const k = (v && typeof v === "object" ? v : {}) as Partial<WellbeingKindState>;
    const every = typeof k.everyMin === "number" && Number.isFinite(k.everyMin)
      ? Math.min(MAX_EVERY, Math.max(MIN_EVERY, Math.round(k.everyMin)))
      : fallback.everyMin;
    return {
      on: k.on === true,
      everyMin: every,
      lastAt: typeof k.lastAt === "number" ? k.lastAt : null,
      lastScreenSec: typeof k.lastScreenSec === "number" ? k.lastScreenSec : null,
    };
  };
  const empty = emptyWellbeing();
  return { break: one(r.break, empty.break), water: one(r.water, empty.water) };
}

/**
 * How long until this nudge is due, in milliseconds; 0 when it is due now and `null` when it is
 * off. `screenSec` is today's tracked screen time, or `null` when tracking is off: then the clock
 * is used instead.
 */
export function dueIn(state: WellbeingKindState, now: number, screenSec: number | null): number | null {
  if (!state.on) return null;
  const everyMs = state.everyMin * 60_000;
  if (state.lastAt === null) return everyMs; // start counting from now, never nudge on opening
  if (screenSec !== null && state.lastScreenSec !== null) {
    // Screen time can only grow within a day; a smaller number means a new day started.
    const used = screenSec >= state.lastScreenSec ? screenSec - state.lastScreenSec : screenSec;
    return Math.max(0, everyMs - used * 1000);
  }
  return Math.max(0, state.lastAt + everyMs - now);
}

/** Show it now? Due, switched on, and not in quiet hours. */
export function isDue(state: WellbeingKindState, nudges: NudgeState, now: Date, screenSec: number | null): boolean {
  return dueIn(state, now.getTime(), screenSec) === 0 && !inQuietHours(nudges, now);
}

export function markShown(w: Wellbeing, kind: WellbeingKind, now: number, screenSec: number | null): Wellbeing {
  return { ...w, [kind]: { ...w[kind], lastAt: now, lastScreenSec: screenSec } };
}

/** When to look again: the soonest nudge that is on, never sooner than a minute. */
export function nextCheckMs(w: Wellbeing, now: number, screenSec: number | null): number | null {
  const waits = KINDS.map((k) => dueIn(w[k], now, screenSec)).filter((v): v is number => v !== null);
  if (waits.length === 0) return null;
  return Math.max(60_000, Math.min(...waits));
}
