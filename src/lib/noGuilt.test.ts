import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, resolve } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * No-guilt rule (FR-92): words Pluto must never show the user. Shaming copy is the top reason
 * people quit habit and pet apps (social research 2026-10-23). Code comments are allowed to
 * mention these words; only string literals in the UI are checked.
 */
const BANNED = [
  /you missed/i, /you failed/i, /don'?t break (your|the) streak/i, /streak (lost|broken)/i,
  /your pet is (sad|hungry|sick|dying)/i, /disappointed/i, /lazy/i, /you forgot/i,
];

function files(dir: string): string[] {
  return readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    if (statSync(p).isDirectory()) return f === "generated" ? [] : files(p);
    return /\.tsx?$/.test(f) && !/\.test\.tsx?$/.test(f) ? [p] : [];
  });
}

/** String literals only: "…", '…' and `…` (comments are stripped first). */
function strings(src: string): string[] {
  const code = src.replace(/\/\*[\s\S]*?\*\//g, "").replace(/(^|[^:"'`])\/\/.*$/gm, "$1");
  return [...code.matchAll(/"([^"\\\n]|\\.)*"|'([^'\\\n]|\\.)*'|`([^`\\]|\\.)*`/g)].map((m) => m[0]);
}

describe("no-guilt rule (FR-92)", () => {
  it("the checker itself catches shaming copy and ignores comments", () => {
    const found = strings(`// you missed it\nconst a = "You missed a day"; const b = 'Ready when you are';`);
    expect(found.filter((s) => BANNED.some((b) => b.test(s)))).toEqual(['"You missed a day"']);
  });

  it("no shaming words appear in any text the app shows", () => {
    const root = resolve(import.meta.dirname, "..");
    const hits = files(root).flatMap((f) =>
      strings(readFileSync(f, "utf8")).filter((s) => BANNED.some((b) => b.test(s))).map((s) => `${f}: ${s}`),
    );
    expect(hits).toEqual([]);
  });
});
