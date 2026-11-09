import { useEffect, useRef, useState } from "react";
import { onTextBox, onTyping } from "@/ipc";

/**
 * Typing along (`pet.typing`). The shell says when you start and stop typing, and (where the
 * OS can tell) whether a text box is focused. This turns that into what the pet shows:
 *
 *   off ──start──▶ type ──stop──▶ wait ──(you leave the text box, or a timer)──▶ stow ──▶ off
 *                   ▲               │
 *                   └────start──────┘
 *
 * `type`: the laptop comes out of the pocket and the paws tap. `wait`: it stops and looks up at
 * you, laptop open, for as long as a text box is focused (you might carry on). Once you leave
 * the box it puts the laptop away (`stow`). Where the OS can't tell (macOS without the
 * Accessibility permission) it waits `WAIT_MS` instead.
 */
export type TypingPhase = "off" | "type" | "wait" | "stow";

/** How long it waits when it can't tell whether a text box is focused. */
export const WAIT_MS = 20_000;
/** A short beat after you leave the text box, so a quick click elsewhere doesn't end it. */
export const LEAVE_MS = 1_500;
/** The longest it waits, even with a text box still focused (a box left open all day). */
export const MAX_WAIT_MS = 5 * 60_000;
/** How long putting the laptop away takes (matches `.pet-laptop-away`). */
export const STOW_MS = 500;

export function useTypingAlong(enabled: boolean, subscribe: typeof onTyping = onTyping, subscribeBox: typeof onTextBox = onTextBox): TypingPhase {
  const [phase, setPhaseState] = useState<TypingPhase>("off");
  const current = useRef<TypingPhase>("off");
  const box = useRef<boolean | null>(null); // null: the OS hasn't said
  const timer = useRef<number | undefined>(undefined);

  useEffect(() => {
    if (!enabled) {
      current.current = "off";
      setPhaseState("off");
      return;
    }
    const setPhase = (p: TypingPhase) => {
      current.current = p;
      setPhaseState(p);
    };
    const stowIn = (ms: number) => {
      clearTimeout(timer.current);
      timer.current = window.setTimeout(() => {
        setPhase("stow");
        timer.current = window.setTimeout(() => setPhase("off"), STOW_MS);
      }, ms);
    };
    // While waiting: how long until the laptop goes away, given what we know about the box.
    const planWait = () => stowIn(box.current === null ? WAIT_MS : box.current ? MAX_WAIT_MS : LEAVE_MS);

    const offTyping = subscribe((typing) => {
      if (typing) {
        clearTimeout(timer.current);
        setPhase("type");
        return;
      }
      if (current.current !== "type") return;
      setPhase("wait");
      planWait();
    });
    const offBox = subscribeBox((focused) => {
      box.current = focused;
      if (current.current === "wait") planWait();
    });
    return () => {
      offTyping();
      offBox();
      clearTimeout(timer.current);
    };
  }, [enabled, subscribe, subscribeBox]);

  return enabled ? phase : "off";
}
