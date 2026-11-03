import { useEffect, useState } from "react";
import type { BearPose } from "@/features/shell/Bear";
import { cn } from "@/lib/utils";
import { headGear, neckGear } from "@/features/wardrobe/Accessories";
import { DEFAULT_ANCHOR, loadAnchors, placement, type SheetAnchors } from "@/features/wardrobe/spriteAnchors";
import type { ItemId } from "@/features/wardrobe/wardrobe";

/**
 * Plays a Codex / OpenPets pet pack (FR-63). The sheet is 8 columns of 192x208 cells; each row
 * is one animation. Frame counts and durations match OpenPets (`reaction-animation-mapping.ts`),
 * so a pack looks the same here as in their app.
 */
export const SPRITE_ROWS = {
  idle: { row: 0, frames: 6, ms: 5500 },
  "running-right": { row: 1, frames: 8, ms: 1060 },
  "running-left": { row: 2, frames: 8, ms: 1060 },
  waving: { row: 3, frames: 4, ms: 700 },
  jumping: { row: 4, frames: 5, ms: 840 },
  failed: { row: 5, frames: 8, ms: 1220 },
  waiting: { row: 6, frames: 6, ms: 1200 },
  running: { row: 7, frames: 6, ms: 820 },
  review: { row: 8, frames: 6, ms: 1030 },
} as const;
export type SpriteRow = keyof typeof SPRITE_ROWS;

/** What a pack plays for each of the bear's poses, so reactions work with any pet. */
export const ROW_FOR_POSE: Record<BearPose, SpriteRow> = {
  idle: "idle", sleep: "idle", turn: "running-right", back: "idle", back_look: "idle",
  wave: "waving", cheer: "jumping", party: "jumping", jump: "jumping",
  work: "running", type: "running", stow: "waiting", wait: "waiting", ring: "waving",
  catch: "review", review: "review", toss: "waving", worried: "failed", sad: "failed",
};

/** Which frame shows `elapsed` ms into a row that loops. */
export function frameAt(row: SpriteRow, elapsed: number): number {
  const { frames, ms } = SPRITE_ROWS[row];
  return Math.floor(((elapsed % ms) / ms) * frames);
}

export function SpritePet({ sheet, row = "idle", animated = true, rest = false, size = 128, className, label, wear = [] }: {
  sheet: string; row?: SpriteRow; animated?: boolean; size?: number; className?: string; label: string;
  /** Asleep. A pack has no sleeping row, so the first idle frame is held, dimmed, with a "z". */
  rest?: boolean;
  /** Accessories on top (owner 2026-10-26: the wardrobe is for every character). */
  wear?: readonly ItemId[];
}) {
  const [frame, setFrame] = useState(0);
  const [anchors, setAnchors] = useState<SheetAnchors | null>(null);
  const dressed = wear.length > 0;
  // Looks at the picture (once per sheet) only when something is worn.
  useEffect(() => {
    if (!dressed) return;
    let live = true;
    void loadAnchors(sheet).then((a) => live && setAnchors(a));
    return () => { live = false; };
  }, [sheet, dressed]);
  useEffect(() => {
    setFrame(0);
    if (!animated || rest) return;
    const { frames, ms } = SPRITE_ROWS[row];
    // One timer per frame step, only while shown; no work between frames.
    let i = 0;
    const id = window.setInterval(() => { i = (i + 1) % frames; setFrame(i); }, ms / frames);
    return () => window.clearInterval(id);
  }, [row, animated, rest]);
  const { row: r } = SPRITE_ROWS[row];
  const scale = size / 208;
  const place = placement(anchors?.[r]?.[frame] ?? DEFAULT_ANCHOR);
  return (
    <div
      role="img"
      aria-label={label}
      data-row={row}
      data-frame={frame}
      data-rest={rest || undefined}
      className={cn("relative shrink-0", className)}
      style={{
        width: 192 * scale,
        height: size,
        backgroundImage: `url("${sheet}")`,
        backgroundSize: `${1536 * scale}px auto`,
        backgroundPosition: `${-frame * 192 * scale}px ${-r * 208 * scale}px`,
        backgroundRepeat: "no-repeat",
        imageRendering: "auto",
        filter: rest ? "saturate(0.6) brightness(0.85)" : undefined,
      }}
    >
      {rest && <span aria-hidden className="absolute right-1 top-1 font-mono text-[11px] font-bold text-foreground/70">z<span className="text-[9px]">z</span></span>}
      {dressed && (
        <svg viewBox="0 0 192 208" aria-hidden="true" className="pointer-events-none absolute inset-0 size-full overflow-visible">
          <g transform={place.head}>{headGear(wear)}</g>
          <g transform={place.neck}>{neckGear(wear)}</g>
        </svg>
      )}
    </div>
  );
}
