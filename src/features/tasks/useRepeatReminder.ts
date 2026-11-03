import { useEffect } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ipc, onWake, petSay } from "@/ipc";
import { ymd } from "@/lib/dates";
import { tryNudge } from "@/features/companion/useCompanion";
import { msUntil } from "@/features/routine/routine";
import { dueNow, openRepeatsMessage, parseRepeatRemind, type RepeatRemind } from "./repeatReminder";

export const REPEAT_REMIND_KEY = ["prefs", "ui.repeatRemind"] as const;

export function useRepeatRemindPrefs() {
  const qc = useQueryClient();
  const { data } = useQuery({ queryKey: REPEAT_REMIND_KEY, queryFn: async () => parseRepeatRemind(await ipc.prefsGet("ui.repeatRemind")) });
  const state = data ?? parseRepeatRemind(null);
  const save = async (next: RepeatRemind) => {
    qc.setQueryData(REPEAT_REMIND_KEY, next);
    await ipc.prefsSet("ui.repeatRemind", next);
  };
  return { state, save, loaded: !!data };
}

/**
 * Once an evening (default 19:00), the pet says which repeating tasks are still open today
 * (FR-109). Through the notification budget, so never in quiet hours and never spammy. One timer
 * to the time, plus a re-check on wake; no polling.
 */
export function useRepeatReminder() {
  const { state, save, loaded } = useRepeatRemindPrefs();
  useEffect(() => {
    if (!loaded) return;
    const check = async () => {
      const now = new Date();
      const today = ymd(now);
      if (!dueNow(state, now, today)) return;
      await save({ ...state, remindedOn: today });
      const message = openRepeatsMessage(await ipc.tasksQuery("today"));
      if (!message) return;
      await tryNudge("repeat", now, () => petSay({ text: message, pose: "wait", ms: 10_000 }));
    };
    void check();
    const wait = msUntil(state.at, new Date());
    const t = wait > 0 ? window.setTimeout(() => void check(), wait + 500) : undefined;
    const offWake = onWake(() => void check());
    return () => { window.clearTimeout(t); offWake(); };
  }, [loaded, state.on, state.at, state.remindedOn]);
}
