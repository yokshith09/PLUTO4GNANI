// Opt-in generator, not a real test: `EXPORT_PACKS=1 pnpm exec vitest run spritePackExport`.
// Writes packs/<id>/{atlas.svg,pet.json}. `pnpm packs:png` (scripts/svg-to-png.mjs) rasterises it.
import { mkdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, it } from "vitest";
import { Bear } from "@/features/shell/Bear";
import { TURNED_VIEWS } from "./CharacterViews";
import { CHARACTERS } from "./Characters";
import { ATLAS_H, ATLAS_W, BIG_H, BIG_W, COLS, ROWS, bigFrameTransform, buildPlutoInfo, buildPetJson, frameTransform, relightForMirror } from "./spritePack";

const TURNED = new Set(["turn", "back", "back_look"]);
const all = [
  { key: "bear", name: "Pluto", what: "the bear" },
  ...CHARACTERS.map((c) => ({ key: c.key, name: c.name, what: c.what })),
];

function art(key: string, pose: string, uid: string): string {
  const el =
    key === "bear" ? <Bear pose={pose as never} /> :
    TURNED.has(pose) ? (() => { const V = TURNED_VIEWS[key as keyof typeof TURNED_VIEWS]; return <V pose={pose as never} />; })() :
    (() => {
      const found = CHARACTERS.find((c) => c.key === key);
      if (!found) throw new Error(`unknown character ${key}`);
      const C = found.Component;
      return <C pose={pose as never} />;
    })();
  // Unique ids per frame so gradients do not collide inside one SVG.
  return renderToStaticMarkup(el).replace(/id="([^"]+)"/g, `id="$1-${uid}"`).replace(/url\(#([^)]+)\)/g, `url(#$1-${uid})`).replace(/href="#([^"]+)"/g, `href="#$1-${uid}"`);
}

describe.skipIf(!process.env.EXPORT_PACKS)("export sprite packs", () => {
  for (const c of all) {
    it(`writes ${c.key}`, () => {
      const dir = resolve(import.meta.dirname, "../../../packs", c.key);
      mkdirSync(dir, { recursive: true });
      const cells: string[] = [];
      const big: string[] = [];
      ROWS.forEach((row, r) => {
        for (let f = 0; f < row.frames; f++) {
          const drawn = art(c.key, row.pose, `${r}-${f}`).replace(/^<svg[^>]*>/, "").replace(/<\/svg>$/, "");
          const inner = row.source === "mirrored" ? relightForMirror(drawn) : drawn;
          cells.push(`<g transform="${frameTransform(r, f)}">${inner}</g>`);
          big.push(`<g transform="${bigFrameTransform(r, f)}">${inner}</g>`);
        }
      });
      // Roomy strips: what `scripts/svg-to-png.mjs` cuts into row strips for the skill's pack.py build.
      writeFileSync(resolve(dir, "strips.svg"), `<svg xmlns="http://www.w3.org/2000/svg" width="${COLS * BIG_W}" height="${ROWS.length * BIG_H}" viewBox="0 0 ${COLS * BIG_W} ${ROWS.length * BIG_H}">${big.join("")}</svg>`);
      writeFileSync(resolve(dir, "atlas.svg"), `<svg xmlns="http://www.w3.org/2000/svg" width="${ATLAS_W}" height="${ATLAS_H}" viewBox="0 0 ${ATLAS_W} ${ATLAS_H}">${cells.join("")}</svg>`);
      writeFileSync(resolve(dir, "pet.json"), JSON.stringify(buildPetJson(c.key, c.name, `${c.name}, ${c.what}`), null, 2) + "\n");
      writeFileSync(resolve(dir, "pluto.json"), JSON.stringify(buildPlutoInfo(c.key), null, 2) + "\n");
    });
  }
});
