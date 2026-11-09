import { BEAR_POSES, type BearPose } from "@/features/shell/Bear";
import type { PetSpecial } from "@/ipc";
import type { Reaction } from "./reactions";

/**
 * A character's one signature move (owner 2026-10-28: "one special reaction per character").
 * The pack names a pose, a line and what sets it off: every few clicks, every few finished
 * tasks, or the first time it is seen each day. Counts live for as long as the pet runs, and
 * start again after each time it plays, so it stays a surprise.
 */
export interface SpecialCounts { clicks: number; tasks: number }
export type SpecialSignal = "click" | "task-done" | "day-start";

export const NO_COUNTS: SpecialCounts = { clicks: 0, tasks: 0 };

/** The new counts, and whether the special plays now. */
export function advance(counts: SpecialCounts, signal: SpecialSignal, special: PetSpecial | null | undefined): { counts: SpecialCounts; fire: boolean } {
  if (!special) return { counts, fire: false };
  if (special.trigger === "daily") return { counts, fire: signal === "day-start" };
  if (special.trigger === "click" && signal === "click") {
    const clicks = counts.clicks + 1;
    return clicks >= special.every ? { counts: { ...counts, clicks: 0 }, fire: true } : { counts: { ...counts, clicks }, fire: false };
  }
  if (special.trigger === "tasks" && signal === "task-done") {
    const tasks = counts.tasks + 1;
    return tasks >= special.every ? { counts: { ...counts, tasks: 0 }, fire: true } : { counts: { ...counts, tasks }, fire: false };
  }
  return { counts, fire: false };
}

/** The reaction a special plays. A pose the pet does not know becomes a wave. */
export function reactionForSpecial(special: PetSpecial): Reaction {
  const pose = ((BEAR_POSES as readonly string[]).includes(special.pose) ? special.pose : "wave") as BearPose;
  return { pose, bubble: special.line, ms: 5000, priority: 3 };
}
