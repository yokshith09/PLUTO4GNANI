/**
 * Sprite-pack layout (FR-50). One atlas per character: 8 columns x 9 rows of 192x208 cells,
 * plus a `pet.json`. The row names and frame counts follow the OpenPets / Codex "hatch-pet"
 * layout as we understand it (unconfirmed: the OpenPets site could not be fetched, D-SIZE).
 *
 * Our art does not cover every row. Each row says where its frames come from:
 *  - `drawn`   : a pose we drew (possibly with a small bob/shift),
 *  - `mirrored`: the drawn right-facing art flipped, with the gradients re-lit so the key light
 *                stays top left (standard sprite practice for a left-facing run),
 *  - `derived` : a transform of drawn art (hop, shake) with no new drawing. No row uses it now;
 *                the Studio shows such a row as "needs art".
 */
export const CELL_W = 192;
export const CELL_H = 208;
export const COLS = 8;

export type RowSource = "drawn" | "mirrored" | "derived";
export interface RowSpec {
  name: string;
  frames: number;
  pose: string;
  source: RowSource;
  /** Per-frame transform in cell units, t runs 0..1 across the row. */
  motion: (t: number) => { dx: number; dy: number; rot: number; flip?: boolean; sy?: number };
}

/**
 * Breathing: a slight stretch about the feet. The feet never leave the baseline (a bob made the
 * pet look like it was jittering up and down, owner 2026-10-28); `amp` is the stretch in percent.
 */
const breathe = (amp: number) => (t: number) => ({ dx: 0, dy: 0, rot: 0, sy: 1 + (amp / 100) * Math.sin(t * Math.PI * 2) });
/** Walking in place: the body rocks about its feet, the feet stay put. */
const waddle = (deg: number) => (t: number) => ({ dx: 0, dy: 0, rot: Math.sin(t * Math.PI * 2) * deg });
const still = () => ({ dx: 0, dy: 0, rot: 0 });

export const ROWS: readonly RowSpec[] = [
  { name: "idle", frames: 6, pose: "idle", source: "drawn", motion: breathe(3) },
  { name: "running-right", frames: 8, pose: "turn", source: "drawn", motion: waddle(4) },
  { name: "running-left", frames: 8, pose: "turn", source: "mirrored", motion: (t) => ({ ...waddle(4)(t), flip: true }) },
  { name: "waving", frames: 4, pose: "cheer", source: "drawn", motion: still },
  // Crouch on the first and last frame (squash), stretch while rising.
  { name: "jumping", frames: 5, pose: "jump", source: "drawn", motion: (t) => ({ dx: 0, dy: -Math.sin(t * Math.PI) * 28, rot: 0, sy: t === 0 || t === 1 ? 0.9 : t === 0.25 ? 1.06 : 1 }) },
  // Slumps a little and sways side to side.
  { name: "failed", frames: 8, pose: "sad", source: "drawn", motion: (t) => ({ dx: Math.sin(t * Math.PI * 2) * 2, dy: Math.min(t * 8, 3), rot: Math.sin(t * Math.PI * 2) * 3 }) },
  { name: "waiting", frames: 6, pose: "wait", source: "drawn", motion: breathe(2) },
  { name: "running", frames: 6, pose: "type", source: "drawn", motion: breathe(2) },
  { name: "review", frames: 6, pose: "review", source: "drawn", motion: (t) => ({ dx: 0, dy: 0, rot: Math.sin(t * Math.PI * 2) * 2 }) },
] as const;

export const ROW_COUNT = ROWS.length;
export const ATLAS_W = COLS * CELL_W;
export const ATLAS_H = ROW_COUNT * CELL_H;

export function frameTransform(row: number, frame: number, at: { row: number; x: number } = { row, x: col(frame) * CELL_W }): string {
  const spec = ROWS[row];
  if (!spec) throw new RangeError(`no such sprite row: ${row}`);
  const m = spec.motion(spec.frames > 1 ? frame / (spec.frames - 1) : 0);
  const s = CELL_W / 200; // art is 200x200
  const cx = CELL_W / 2;
  const x = at.x + m.dx;
  const y = at.row * CELL_H + 8 + m.dy;
  const flip = m.flip ? ` translate(${cx},0) scale(-1,1) translate(${-cx},0)` : "";
  const feet = 188 * s;
  const squash = m.sy && m.sy !== 1 ? ` translate(0,${feet}) scale(${2 - m.sy},${m.sy}) translate(0,${-feet})` : "";
  return `translate(${x},${y}) ${flip}${squash} rotate(${m.rot},${cx},${100 * s}) scale(${s})`;
}

/**
 * Re-light art that will be drawn mirrored: flip every gradient's horizontal position so that,
 * after the mirror, the key light is back at the top left.
 */
export function relightForMirror(svg: string): string {
  const flipPct = (v: string) => `${100 - Number(v)}%`;
  return svg
    .replace(/<radialGradient[^>]*>/g, (tag) => tag.replace(/\b(cx|fx)="(\d+(?:\.\d+)?)%"/g, (_m, k: string, v: string) => `${k}="${flipPct(v)}"`))
    .replace(/<linearGradient[^>]*>/g, (tag) => tag.replace(/\bx1="([^"]+)"([^>]*)\bx2="([^"]+)"/, (_m, a: string, mid: string, b: string) => `x1="${b}"${mid}x2="${a}"`));
}

const col = (frame: number) => frame;

/** `pet.json`: only the fields OpenPets/Codex pets define (docs/pets.md in OpenPetsHQ/openpets). */
export interface PetJson { id: string; displayName: string; description: string; spritesheetPath: string }

/** `pluto.json`: our own details, kept out of pet.json so strict importers never see unknown keys. */
export interface PlutoPackInfo {
  cell: { width: number; height: number };
  columns: number;
  rows: { name: string; frames: number; pose: string; source: RowSource }[];
  /** This character's own default pose for some moments (the user's choice still wins). */
  reactions?: Record<string, string>;
  /** The one signature move: a pose, a line and what sets it off. */
  special?: { name: string; pose: string; line: string; trigger: "click" | "tasks" | "daily"; every: number };
  license: string;
  status: "Unverified";
}

export type PackManifest = PetJson & PlutoPackInfo;

export function buildPetJson(id: string, displayName: string, description: string): PetJson {
  return { id, displayName, description, spritesheetPath: "spritesheet.webp" };
}

/** Each built-in character's own reactions and special (owner 2026-10-28: one special each). */
export const PACK_EXTRAS: Record<string, Pick<PlutoPackInfo, "reactions" | "special">> = {
  bear: { reactions: { "all-done": "party" }, special: { name: "Pluto mode", pose: "sleep", line: "Five done. Pluto mode: on.", trigger: "tasks", every: 5 } },
  mochi: { reactions: { "task-done": "jump", "task-deleted": "toss" }, special: { name: "Zoomies", pose: "jump", line: "Zoomies!", trigger: "click", every: 5 } },
  pip: { reactions: { "task-added": "wave", "all-done": "party" }, special: { name: "Belly slide", pose: "toss", line: "Three done. Belly slide!", trigger: "tasks", every: 3 } },
  bolt: { reactions: { "task-started": "type", reminder: "ring" }, special: { name: "Systems online", pose: "type", line: "Systems online. Ready when you are.", trigger: "daily", every: 2 } },
  mira: { reactions: { "task-done": "cheer", "all-done": "party" }, special: { name: "Petal spin", pose: "cheer", line: "Five done! Petal spin!", trigger: "tasks", every: 5 } },
  raja: { reactions: { "task-done": "cheer", "task-started": "type" }, special: { name: "Shades on", pose: "cheer", line: "Two done. Shades on.", trigger: "tasks", every: 2 } },
  ghosty: { reactions: { "task-deleted": "worried", "focus-done": "cheer" }, special: { name: "Boo", pose: "worried", line: "Boo! …did I scare you?", trigger: "click", every: 7 } },
};

export function buildPlutoInfo(key?: string): PlutoPackInfo {
  return {
    ...(key ? PACK_EXTRAS[key] : {}),
    cell: { width: CELL_W, height: CELL_H },
    columns: COLS,
    rows: ROWS.map((r) => ({ name: r.name, frames: r.frames, pose: r.pose, source: r.source })),
    license: "CC BY 4.0, credit QUORBIT TECHNOLOGIES (auto-chosen 2026-10-22, owner may change)",
    status: "Unverified",
  };
}

export function buildManifest(id: string, displayName: string, description: string): PackManifest {
  return { ...buildPetJson(id, displayName, description), ...buildPlutoInfo() };
}

/**
 * Export-only: the same frame, but on a roomy cell (BIG_W x BIG_H, art at the bottom with space
 * above for raised arms and a jump). The skill's `pack.py build` scales and aligns these exactly
 * as it does a user's strips, so our own characters hold to the same rules (baseline, centre, size).
 */
export const BIG_W = 400; // wide: wings and tails that overflow the art box must not reach the next frame
export const BIG_H = 420;
export function bigFrameTransform(row: number, frame: number): string {
  const k = 300 / CELL_W; // the art is drawn 300 px wide, centred in the roomy cell
  const left = (BIG_W - CELL_W * k) / 2;
  // The roomy cell is the normal cell scaled up and moved down by the extra height above.
  return `translate(${frame * BIG_W + left},${row * BIG_H + BIG_H - CELL_H * k}) scale(${k}) ${frameTransform(row, frame, { row: 0, x: 0 })}`;
}
