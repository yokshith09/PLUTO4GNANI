import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { ipc, type PetPack } from "@/ipc";
import { useWorn } from "@/features/wardrobe/useWardrobe";
import type { BearPose } from "@/features/shell/Bear";
import { ROW_FOR_POSE, SPRITE_ROWS, SpritePet, type SpriteRow } from "./SpritePet";

/** What each animation row of a pack is for, in words a person would use. */
export const ROW_LABELS: Record<SpriteRow, string> = {
  idle: "Idle", "running-right": "Walk right", "running-left": "Walk left", waving: "Hello", jumping: "Cheer",
  failed: "Sad", waiting: "Waiting", running: "Working", review: "Reviewing",
};

const SOURCE: Record<PetPack["source"], string> = { builtin: "Built in", codex: "From Codex / Petdex", pluto: "Imported" };

const WHEN = {
  click: (n: number) => `every ${n} clicks on the pet`,
  tasks: (n: number) => `every ${n} tasks you finish`,
  daily: () => "the first time you see it each day",
} as const;

/** The character's one special move: what it is called, what sets it off, and a way to watch it. */
function SpecialLine({ special, onTry }: { special: NonNullable<PetPack["special"]>; onTry: (pose: string) => void }) {
  return (
    <div className="flex w-full items-center justify-between gap-2 rounded-lg border px-3 py-2">
      <p className="min-w-0 text-xs"><span className="font-semibold">Special: {special.name}</span> <span className="text-muted-foreground">· {WHEN[special.trigger](special.every)}</span></p>
      <Button size="sm" variant="outline" onClick={() => onTry(special.pose)}>Watch</Button>
    </div>
  );
}

/** The Character page preview for a chosen pack: the big pet and a button for each animation it has. */
export function PackPreview({ pack }: { pack: PetPack }) {
  const [row, setRow] = useState<SpriteRow>("idle");
  const worn = useWorn();
  const { data: sheet, isError } = useQuery({ queryKey: ["pets", "sheet", pack.id], queryFn: () => ipc.petsSheet(pack.id), staleTime: Infinity });
  // Packs can have fewer rows than the full nine; only offer the ones that exist.
  const rows = (Object.keys(SPRITE_ROWS) as SpriteRow[]).filter((r) => SPRITE_ROWS[r].row < pack.rows);
  return (
    <>
      <div className="grid min-h-72 w-full flex-1 place-items-center rounded-lg border bg-muted/40 [background-image:radial-gradient(circle_at_center,var(--border)_1px,transparent_1px)] [background-size:16px_16px]">
        {sheet ? <SpritePet key={pack.id} sheet={sheet} row={row} size={224} wear={worn} label={`${pack.display_name} (${ROW_LABELS[row]})`} />
          : <p className="text-xs text-muted-foreground">{isError ? "Couldn’t load this character." : "Loading…"}</p>}
      </div>
      <div className="flex w-full items-center justify-between">
        <div>
          <p className="text-sm font-semibold">{pack.display_name}</p>
          <p className="text-xs text-muted-foreground">{pack.description || SOURCE[pack.source]}</p>
        </div>
        <p className="text-xs text-muted-foreground">{rows.length} animations</p>
      </div>
      {pack.special && <SpecialLine special={pack.special} onTry={(pose) => { const r = ROW_FOR_POSE[pose as BearPose] ?? "waving"; if (rows.includes(r)) setRow(r); }} />}
      <div className="flex w-full flex-wrap gap-1.5" role="group" aria-label="Preview animation">
        {rows.map((r) => (
          <Button key={r} size="sm" variant={r === row ? "default" : "outline"} aria-pressed={r === row} onClick={() => setRow(r)}>
            {ROW_LABELS[r]}
          </Button>
        ))}
      </div>
    </>
  );
}

/** A small, still view of a character in its current look, for the Wardrobe page. */
export function PackMirror({ pack }: { pack: PetPack }) {
  const worn = useWorn();
  const { data: sheet } = useQuery({ queryKey: ["pets", "sheet", pack.id], queryFn: () => ipc.petsSheet(pack.id), staleTime: Infinity });
  return sheet ? <SpritePet key={pack.id} sheet={sheet} row="idle" size={144} wear={worn} label={`${pack.display_name} in your current look`} /> : null;
}
