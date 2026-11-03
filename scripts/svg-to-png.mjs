// Rasterise packs/<id>/atlas.svg to spritesheet.webp (the Codex/OpenPets standard) with the installed Chromium (Playwright); spritesheet.png is an untracked local preview.
// Packs with a strips.svg (the exporter writes one) are cut into roomy row strips and run through the skill's
// `pack.py build`, so our own characters are aligned by the same rules as a user's (baseline, centre, one size).
// Usage: NODE_PATH=<dir with playwright-core> node scripts/svg-to-png.mjs [packsDir]. playwright-core is NOT a project dependency (kept out on purpose); CHROMIUM_PATH points at a Chromium.
/* global document */
import { readdirSync, existsSync, readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { Buffer } from "node:buffer";
import { createRequire } from "node:module";
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, rmSync } from "node:fs";
const { chromium } = createRequire(import.meta.url)("playwright-core");

const root = resolve(process.argv[2] ?? "packs");
const exe = process.env.CHROMIUM_PATH ?? "/opt/pw-browsers/chromium-1194/chrome-linux/chrome";
const browser = await chromium.launch({ executablePath: exe });
const page = await browser.newPage({ viewport: { width: 1536, height: 1872 } });
const ROWS = [["idle", 6], ["running-right", 8], ["running-left", 8], ["waving", 4], ["jumping", 5], ["failed", 8], ["waiting", 6], ["running", 6], ["review", 6]];
const BIG_W = 400, BIG_H = 420; // keep in step with spritePack.ts
const packPy = resolve(import.meta.dirname, "../skills/pluto-character/scripts/pack.py");
for (const id of readdirSync(root)) {
  const strips = resolve(root, id, "strips.svg");
  if (existsSync(strips)) {
    const dir = resolve(root, id, "strips");
    const out = resolve(root, id, "build");
    rmSync(dir, { recursive: true, force: true });
    rmSync(out, { recursive: true, force: true });
    mkdirSync(dir, { recursive: true });
    const tall = await browser.newPage({ viewport: { width: 8 * BIG_W, height: ROWS.length * BIG_H } });
    await tall.goto("file://" + strips);
    for (const [r, [name, frames]] of ROWS.entries()) {
      await tall.screenshot({ path: resolve(dir, `${name}.png`), omitBackground: true, clip: { x: 0, y: r * BIG_H, width: frames * BIG_W, height: BIG_H } });
    }
    await tall.close();
    const pet = JSON.parse(readFileSync(resolve(root, id, "pet.json"), "utf8"));
    execFileSync("python3", [packPy, "build", dir, out, "--id", pet.id, "--name", pet.displayName, "--description", pet.description ?? ""], { stdio: "inherit" });
    copyFileSync(resolve(out, pet.id, "spritesheet.webp"), resolve(root, id, "spritesheet.webp"));
    rmSync(dir, { recursive: true, force: true });
    rmSync(out, { recursive: true, force: true });
    console.log("built", id);
    continue;
  }
  const svg = resolve(root, id, "atlas.svg");
  if (!existsSync(svg)) continue;
  await page.goto("file://" + svg);
  await page.screenshot({ path: resolve(root, id, "spritesheet.png"), omitBackground: true, clip: { x: 0, y: 0, width: 1536, height: 1872 } });
  // WebP via the browser's own encoder: draw the PNG on a canvas and export it at top quality (Chrome may still encode lossy).
  const png = readFileSync(resolve(root, id, "spritesheet.png")).toString("base64");
  const webp = await page.evaluate(async (b64) => {
    const img = document.createElementNS("http://www.w3.org/1999/xhtml", "img");
    img.src = "data:image/png;base64," + b64;
    await img.decode();
    const c = document.createElementNS("http://www.w3.org/1999/xhtml", "canvas"); // the page is an SVG document
    c.width = img.width; c.height = img.height;
    c.getContext("2d").drawImage(img, 0, 0);
    return c.toDataURL("image/webp", 1).split(",")[1];
  }, png);
  writeFileSync(resolve(root, id, "spritesheet.webp"), Buffer.from(webp, "base64"));
  console.log("wrote", id);
}
await browser.close();
