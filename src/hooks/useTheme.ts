import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { ipc, type Settings, type SettingsKey } from "@/ipc";
import { keys, useSettings } from "./usePluto";

const FONT_SCALE = { S: 0.9, M: 1, L: 1.15 } as const;

/** Applies `general.theme` and `general.font_size` to <html>. */
export function useTheme() {
  const qc = useQueryClient();
  const { data } = useSettings();
  const pref = data?.["general.theme"] ?? "dark";
  const size = data?.["general.font_size"] ?? "M";

  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () =>
      document.documentElement.classList.toggle("dark", pref === "dark" || (pref === "system" && mq.matches));
    apply();
    mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
  }, [pref]);

  useEffect(() => {
    document.documentElement.style.fontSize = `${FONT_SCALE[size] * 100}%`;
  }, [size]);

  const isDark = pref === "dark" || (pref === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
  return {
    isDark,
    toggle: async () => {
      await ipc.settingSet("general.theme", isDark ? "light" : "dark");
      await qc.invalidateQueries({ queryKey: keys.settings });
    },
  };
}

/** One setting as [value, set]. Writes through IPC, then refreshes. */
export function useSetting<K extends SettingsKey>(key: K): [Settings[K] | undefined, (v: Settings[K]) => Promise<void>] {
  const qc = useQueryClient();
  const { data } = useSettings();
  return [
    data?.[key],
    async (v) => {
      await ipc.settingSet(key, v);
      await qc.invalidateQueries({ queryKey: keys.settings });
    },
  ];
}
