import type { BearPose } from "@/features/shell/Bear";
import type { PlutoEvent, TaskRow } from "@/ipc";
import type { Event } from "@/ipc/generated/Event";
import { BREAK_MINUTES, endLine } from "@/lib/focus";

/**
 * What the pet does when something happens. One row per moment: add a row to react to
 * something new. `priority` decides who wins when two arrive together (higher plays first).
 */
export interface Reaction { pose: BearPose; bubble: string; ms: number; priority: number; action?: { id: string; label: string } }

/** What a reaction may look at: today's open tasks as they were just before the event. */
export interface ReactionContext { openToday: TaskRow[] }

const short = (s: string, n = 26) => (s.length > n ? `${s.slice(0, n - 1).trimEnd()}…` : s);
const quote = (s: string) => `“${short(s)}”`;

type Rule<T> = (e: T, ctx: ReactionContext) => Reaction | null;
type Signal = Event;
type Rules = { [K in Signal["type"]]?: Rule<Extract<Signal, { type: K }>> };

export const RULES: Rules = {
  TaskCreated: (e) => ({ pose: "catch", bubble: `Got it: ${quote(e.task.title)}`, ms: 3000, priority: 2 }),
  TaskStatusChanged: (e, ctx) => {
    if (e.to === "COMPLETED") {
      const left = ctx.openToday.filter((r) => r.task.id !== e.id).length;
      // The one bubble for a completion also offers Undo (click the pet), so no second one is needed.
      const undo = { id: `task-undo:${e.id}`, label: "Undo" };
      return left === 0
        ? { pose: "party", bubble: "All done for today! 🎉", ms: 4500, priority: 4, action: undo }
        : { pose: "cheer", bubble: `Done! ${left} left today`, ms: 4500, priority: 3, action: undo };
    }
    if (e.to === "IN_PROGRESS") {
      const row = ctx.openToday.find((r) => r.task.id === e.id);
      return { pose: "work", bubble: row ? `On it: ${quote(row.task.title)}` : "On it!", ms: 3000, priority: 1 };
    }
    return null;
  },
  TaskDeleted: () => ({ pose: "toss", bubble: "Tossed it", ms: 2400, priority: 1 }),
  // Long enough to be seen when the system notification was swallowed (Do not disturb).
  ReminderDue: (e) => ({ pose: "ring", bubble: `Reminder: ${quote(e.reminder.title)}`, ms: 15_000, priority: 5, action: { id: `reminder-done:${e.reminder.id}`, label: "Done" } }),
  // FR-110: a finished focus cheers and offers a break (click the pet to start it). You started
  // the session, so this isn't a nudge and doesn't count toward the daily limit.
  FocusEnded: (e) => {
    const line = endLine(e.session);
    if (!line) return null;
    return { pose: line.pose as BearPose, bubble: line.text, ms: 12_000, priority: 4, ...(line.offerBreak ? { action: { id: `focus-break:${BREAK_MINUTES}`, label: "Break" } } : {}) };
  },
};

/**
 * A message from the dashboard (FR-43), e.g. "You have 3 daily routine tasks pending". Unknown
 * poses fall back to a wave; it stays up long enough to be read.
 */
export function reactionForSay(say: { text: string; pose?: string; ms?: number; actionId?: string; actionLabel?: string }, poses: readonly string[]): Reaction | null {
  const text = say.text.trim();
  if (!text) return null;
  const pose = (say.pose && poses.includes(say.pose) ? say.pose : "wave") as BearPose;
  const r: Reaction = { pose, bubble: text.length > 160 ? `${text.slice(0, 159)}…` : text, ms: Math.min(Math.max(say.ms ?? 8000, 2000), 20_000), priority: 4 };
  if (say.actionId && say.actionLabel) r.action = { id: say.actionId, label: say.actionLabel.slice(0, 20) };
  return r;
}

/** The reaction for an event, or null when the pet stays calm. */
export function reactionFor(e: PlutoEvent, ctx: ReactionContext): Reaction | null {
  // Events come from the core as generated `Event`s; the transport types them loosely.
  const rule = (RULES as Record<string, Rule<Signal> | undefined>)[e.type];
  return rule ? rule(e as Signal, ctx) : null;
}

/** Up to three waiting reactions, highest priority first; on a tie the older one plays first. */
export function enqueue(queue: Reaction[], r: Reaction, max = 3): Reaction[] {
  const next = [...queue, r].sort((a, b) => b.priority - a.priority);
  return next.slice(0, max);
}

/** The bear sleeps after this long with nothing happening (no hover, no click, no event). */
export const SLEEP_AFTER_MS = 15 * 60_000;
export const isAsleep = (lastActive: number, now = Date.now()) => now - lastActive >= SLEEP_AFTER_MS;
