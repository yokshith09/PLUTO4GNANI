import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { ipc } from "@/ipc";

/**
 * Time tiles (FR-44; two choices only, owner 2026-10-27): the Apps/Sites list beside "By category",
 * or each on its own full-width row. Remembered in prefs `ui.timeLayout`. The row height is fixed.
 */
export type TileLayout = { width: "side" | "full" };
export const DEFAULT_LAYOUT: TileLayout = { width: "side" };

export const WIDTH_CLASS: Record<TileLayout["width"], [string, string]> = {
  side: ["lg:col-span-8", "lg:col-span-4"],
  full: ["lg:col-span-12", "lg:col-span-12"],
};
export const ROW_HEIGHT = "lg:[&>*]:h-[30rem]";

/** Reads the saved choice. The old "wide" and "even" are side by side, the old "stacked" is full width. */
export function parseLayout(raw: unknown): TileLayout {
  const w = raw && typeof raw === "object" ? (raw as { width?: unknown }).width : undefined;
  return { width: w === "full" || w === "stacked" ? "full" : "side" };
}

export function useTileLayout(): [TileLayout, (l: TileLayout) => void] {
  const qc = useQueryClient();
  const key = ["prefs", "ui.timeLayout"];
  const { data } = useQuery({ queryKey: key, queryFn: async () => parseLayout(await ipc.prefsGet("ui.timeLayout")) });
  return [data ?? DEFAULT_LAYOUT, (l) => { qc.setQueryData(key, l); void ipc.prefsSet("ui.timeLayout", l); }];
}

export function TileLayoutPicker({ value, onChange }: { value: TileLayout; onChange: (l: TileLayout) => void }) {
  return (
    <div className="hidden items-center justify-end gap-3 text-xs text-muted-foreground lg:flex" role="group" aria-label="Tile layout">
      <span>Layout</span>
      <ToggleGroup type="single" value={value.width} onValueChange={(v) => v && onChange({ width: v as TileLayout["width"] })} aria-label="Tile width">
        <ToggleGroupItem value="side">Side by side</ToggleGroupItem>
        <ToggleGroupItem value="full">Full width</ToggleGroupItem>
      </ToggleGroup>
    </div>
  );
}
