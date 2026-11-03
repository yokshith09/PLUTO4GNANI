import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ipc } from "@/ipc";
import { useCompanion } from "@/features/companion/useCompanion";
import { parseWardrobe, wornNow, type ItemId, type Wardrobe } from "./wardrobe";

export const WARDROBE_KEY = ["prefs", "ui.wardrobe"] as const;

/** The saved look. The pet window refetches it whenever it is told something changed. */
export function useWardrobe() {
  const qc = useQueryClient();
  const { data } = useQuery({ queryKey: WARDROBE_KEY, queryFn: async () => parseWardrobe(await ipc.prefsGet("ui.wardrobe").catch(() => null)) });
  const save = async (next: Wardrobe) => {
    qc.setQueryData(WARDROBE_KEY, next);
    await ipc.prefsSet("ui.wardrobe", next);
  };
  /** Shows a change right away (while a slider is dragged) without saving it yet. */
  const preview = (next: Wardrobe) => qc.setQueryData(WARDROBE_KEY, next);
  return { wardrobe: data, save, preview };
}

/** What the bear has on right now, birthday hat included. */
export function useWorn(): ItemId[] {
  const { wardrobe } = useWardrobe();
  const { companion } = useCompanion();
  return wardrobe ? wornNow(wardrobe, companion, new Date()) : [];
}
