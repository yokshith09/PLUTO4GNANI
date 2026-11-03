import { useEffect } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ipc, onWake, type RetentionCounts, type RetentionRules } from "@/ipc";

/** Auto-delete rules (FR-46), in prefs `ui.autoDelete`. Never by default. */
export const AUTO_DELETE_KEY = ["prefs", "ui.autoDelete"] as const;
export const AGES = [
  { days: 0, label: "Never" }, { days: 30, label: "30 days" }, { days: 90, label: "90 days" },
  { days: 180, label: "6 months" }, { days: 365, label: "1 year" },
] as const;
const OK = AGES.map((a) => a.days) as readonly number[];

export function parseRules(raw: unknown): RetentionRules {
  const r = (raw && typeof raw === "object" ? raw : {}) as Partial<RetentionRules>;
  const pick = (v: unknown) => (typeof v === "number" && OK.includes(v) ? v : 0);
  return { time_days: pick(r.time_days), tasks_days: pick(r.tasks_days) };
}

/** "12 time records will be deleted and 3 completed tasks moved to the Bin." */
export function previewText(c: RetentionCounts): string {
  const parts: string[] = [];
  if (c.time_sessions) parts.push(`${c.time_sessions} time ${c.time_sessions === 1 ? "record" : "records"} will be deleted`);
  if (c.tasks_binned) parts.push(`${c.tasks_binned} completed ${c.tasks_binned === 1 ? "task" : "tasks"} will move to the Bin`);
  return parts.length ? `${parts.join(" and ")}.` : "Nothing is old enough yet; it will start when something is.";
}

export function useAutoDeleteRules() {
  const qc = useQueryClient();
  const { data } = useQuery({ queryKey: AUTO_DELETE_KEY, queryFn: async () => parseRules(await ipc.prefsGet("ui.autoDelete")) });
  const save = async (next: RetentionRules) => {
    qc.setQueryData(AUTO_DELETE_KEY, next);
    await ipc.prefsSet("ui.autoDelete", next);
  };
  return { rules: data ?? parseRules(null), loaded: !!data, save };
}

const SIX_HOURS = 6 * 3_600_000;

/** Run the rules when Pluto starts, every few hours, and after the computer wakes. */
export function useAutoDelete() {
  const { rules, loaded } = useAutoDeleteRules();
  useEffect(() => {
    if (!loaded) return;
    // Always run: tasks that spent 30 days in the Bin are emptied even with both rules off.
    const run = () => void ipc.retentionApply(rules).catch(() => undefined);
    run();
    const t = window.setInterval(run, SIX_HOURS);
    const off = onWake(run);
    return () => { clearInterval(t); off(); };
  }, [loaded, rules.time_days, rules.tasks_days]);
}
