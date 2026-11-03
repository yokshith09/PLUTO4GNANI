import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { errorMessage, ipc, type FocusKind } from "@/ipc";
import { remainingMs } from "@/lib/focus";
import { notify } from "@/lib/notify";

export const FOCUS_KEY = ["focus"] as const;

/**
 * The running focus session (FR-05), its time left (ticking once a second while it runs), and
 * the controls. At zero it asks the core to finish it; both the pet and the dashboard may, the
 * core keeps only the first.
 */
export function useFocus() {
  const qc = useQueryClient();
  const { data: session = null } = useQuery({ queryKey: FOCUS_KEY, queryFn: () => ipc.focusCurrent() });
  const [now, setNow] = useState(() => Date.now());
  const ticking = !!session && session.paused_at === null;
  useEffect(() => {
    if (!ticking) return;
    setNow(Date.now());
    const t = window.setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, [ticking, session?.id]);
  const remaining = session ? remainingMs(session, now) : 0;
  const ranOut = ticking && remaining === 0;
  useEffect(() => {
    if (!ranOut || !session) return;
    void ipc.focusFinish(session.id).catch(() => undefined).finally(() => void qc.invalidateQueries({ queryKey: FOCUS_KEY }));
  }, [ranOut, session?.id]);

  const run = async <T,>(p: Promise<T>) => {
    try {
      const r = await p;
      await qc.invalidateQueries({ queryKey: FOCUS_KEY });
      return r;
    } catch (e) {
      notify.error(errorMessage(e));
      return null;
    }
  };
  return {
    session: ranOut ? null : session,
    remaining,
    start: (kind: FocusKind, minutes: number, taskId: string | null = null) => run(ipc.focusStart({ kind, minutes, task_id: taskId })),
    pause: (paused: boolean) => run(ipc.focusPause(paused)),
    extend: (minutes = 5) => run(ipc.focusExtend(minutes)),
    stop: () => run(ipc.focusStop()),
  };
}

const LENGTH_KEY = ["prefs", "ui.focus_minutes"] as const;
export const FOCUS_MIN = 5;
export const FOCUS_MAX = 240;
export const clampFocus = (m: number) => Math.min(FOCUS_MAX, Math.max(FOCUS_MIN, Math.round(m)));

/** The focus length last picked (Focus tile and task editor share it; 25 at first). */
export function useFocusLength() {
  const qc = useQueryClient();
  const { data } = useQuery({
    queryKey: LENGTH_KEY,
    queryFn: async () => {
      const v = await ipc.prefsGet<number>("ui.focus_minutes");
      return typeof v === "number" && Number.isFinite(v) ? clampFocus(v) : 25;
    },
  });
  const set = async (m: number) => {
    const v = clampFocus(m);
    qc.setQueryData(LENGTH_KEY, v);
    await ipc.prefsSet("ui.focus_minutes", v);
  };
  return { minutes: data ?? 25, set };
}
