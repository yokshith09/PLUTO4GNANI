import type { TurnedPose } from "@/features/shell/BearViews";

/** One beat of an emote: show `pose` for `ms`. */
export interface EmoteStep { pose: TurnedPose; ms: number }

/**
 * Back-turn shake (FR-40): turn away, look back over the shoulder, rock the hips, turn
 * round again. Drawn views only (BearViews.tsx), never a mirrored front pose.
 */
export const BACK_TURN_SHAKE: readonly EmoteStep[] = [
  { pose: "turn", ms: 320 },
  { pose: "back", ms: 320 },
  { pose: "back_look", ms: 1800 },
  { pose: "back", ms: 240 },
  { pose: "turn", ms: 320 },
];

/** How long the quick exit and the reduced-motion stand-in last. */
export const EMOTE_EXIT_MS = 120;
export const EMOTE_STATIC_MS = 700;

export interface EmoteRun { cancel: () => void }

interface PlayOptions<P extends string> {
  /** The pose to go back to when the emote ends or is cancelled. */
  restore: P;
  set: (pose: P | TurnedPose) => void;
  reducedMotion?: boolean;
  onDone?: (completed: boolean) => void;
}

/**
 * Plays `steps` with timers (no polling, no frame loop). Reduced motion swaps the sequence for
 * one still three-quarter pose. `cancel()` takes a short exit through the three-quarter view,
 * or goes straight back under reduced motion. Calling `cancel()` after the end does nothing.
 */
export function playEmote<P extends string>(steps: readonly EmoteStep[], { restore, set, reducedMotion = false, onDone }: PlayOptions<P>): EmoteRun {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let finished = false;

  const finish = (completed: boolean) => {
    if (finished) return;
    finished = true;
    timer = undefined;
    set(restore);
    onDone?.(completed);
  };

  const run = reducedMotion ? [{ pose: "turn", ms: EMOTE_STATIC_MS } as EmoteStep] : steps;
  const next = (i: number) => {
    const step = run[i];
    if (!step) return finish(true);
    set(step.pose);
    timer = setTimeout(() => next(i + 1), step.ms);
  };
  next(0);

  return {
    cancel: () => {
      if (finished) return;
      if (timer) clearTimeout(timer);
      if (reducedMotion) return finish(false);
      set("turn");
      timer = setTimeout(() => finish(false), EMOTE_EXIT_MS);
    },
  };
}
