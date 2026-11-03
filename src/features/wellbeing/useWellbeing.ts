import { useEffect } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ipc, onWake, petSay } from "@/ipc";
import { rangeBounds } from "@/features/time/range";
import { parseNudges } from "@/lib/nudges";
import {
  KINDS, SAYS, isDue, markShown, nextCheckMs, parseWellbeing, type Wellbeing, type WellbeingKind,
} from "./wellbeing";

export const WELLBEING_KEY = ["prefs", "ui.wellbeing"] as const;

export function useWellbeingPrefs() {
  const qc = useQueryClient();
  const { data } = useQuery({ queryKey: WELLBEING_KEY, queryFn: async () => parseWellbeing(await ipc.prefsGet("ui.wellbeing")) });
  const save = async (next: Wellbeing) => {
    qc.setQueryData(WELLBEING_KEY, next);
    await ipc.prefsSet("ui.wellbeing", next);
  };
  return { state: data, save, loaded: !!data };
}

/** Today's screen time in seconds, or `null` when app tracking is off. */
async function screenTime(): Promise<number | null> {
  try {
    const status = await ipc.trackingStatus();
    if (!status.enabled) return null;
    const today = rangeBounds("today");
    return (await ipc.usageSummary(today.from, today.to)).total_seconds;
  } catch {
    return null;
  }
}

/**
 * Reminds you to stretch and to drink (owner 2026-10-27). One timer to whichever is due first,
 * plus a look when the PC wakes or the window comes back; no polling. Time at the screen is what
 * counts when app tracking is on, so an hour away from the desk does not earn a break.
 */
export function useWellbeingNudges() {
  const qc = useQueryClient();
  const { state, loaded } = useWellbeingPrefs();
  const on = KINDS.map((k) => (state ? `${k}:${state[k].on}:${state[k].everyMin}:${state[k].lastAt}` : "")).join("|");
  useEffect(() => {
    if (!loaded) return;
    let timer: number | undefined;
    let stopped = false;

    const check = async () => {
      if (stopped) return;
      // Read what is saved, not what this render saw: several checks can be in flight.
      const saved = parseWellbeing(await ipc.prefsGet("ui.wellbeing"));
      if (!KINDS.some((k) => saved[k].on)) return;
      const [nudges, screen] = [parseNudges(await ipc.prefsGet("ui.nudges")), await screenTime()];
      const now = new Date();
      let next = saved;
      for (const kind of KINDS) {
        if (!isDue(next[kind], nudges, now, screen)) continue;
        next = markShown(next, kind as WellbeingKind, now.getTime(), screen);
        await petSay({ text: SAYS[kind].text, pose: SAYS[kind].pose, ms: 8000 });
        break; // one at a time, so two bubbles never pile up
      }
      if (next !== saved) {
        await ipc.prefsSet("ui.wellbeing", next);
        qc.setQueryData(WELLBEING_KEY, next);
      }
      if (stopped) return;
      const wait = nextCheckMs(next, Date.now(), screen);
      if (wait !== null) timer = window.setTimeout(() => void check(), wait);
    };

    void check();
    const onShow = () => document.visibilityState === "visible" && void check();
    document.addEventListener("visibilitychange", onShow);
    const offWake = onWake(() => void check());
    return () => {
      stopped = true;
      window.clearTimeout(timer);
      document.removeEventListener("visibilitychange", onShow);
      offWake();
    };
  }, [loaded, on, qc]);
}
