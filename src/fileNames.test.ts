import { readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

/** Windows and macOS ignore case in file names; Linux does not. `./Tour` must never be able to mean `tour.ts` (0.5.2's CI failed on exactly that). */
function clashes(dir: string, found: string[] = []): string[] {
  const seen = new Map<string, string>();
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) { clashes(path, found); continue; }
    const key = name.toLowerCase().replace(/\.(tsx?|jsx?)$/, "");
    const other = seen.get(key);
    if (other !== undefined) found.push(`${join(dir, other)} and ${path}`);
    seen.set(key, name);
  }
  return found;
}

describe("source file names", () => {
  it("no two files in a folder differ only by case or by .ts versus .tsx", () => {
    expect(clashes("src")).toEqual([]);
  });
});
