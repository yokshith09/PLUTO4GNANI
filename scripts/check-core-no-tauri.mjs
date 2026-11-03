#!/usr/bin/env node
// ADR-002 / F0-01: `pluto-core` must never depend on tauri, directly or transitively,
// so it compiles and its tests run on any machine. Fails CI if that ever changes.
import { execFileSync } from "node:child_process";

const meta = JSON.parse(
  execFileSync("cargo", ["metadata", "--format-version", "1", "--locked"], {
    encoding: "utf8",
    maxBuffer: 256 * 1024 * 1024,
  }),
);

const byId = new Map(meta.packages.map((p) => [p.id, p]));
const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
const root = meta.packages.find((p) => p.name === "pluto-core");
if (!root) {
  console.error("pluto-core not found in the workspace");
  process.exit(2);
}

const seen = new Set();
const stack = [root.id];
const offenders = [];
while (stack.length) {
  const id = stack.pop();
  if (seen.has(id)) continue;
  seen.add(id);
  const name = byId.get(id)?.name ?? id;
  if (id !== root.id && /^(tauri($|-))/.test(name)) offenders.push(name);
  for (const dep of nodes.get(id)?.dependencies ?? []) stack.push(dep);
}

if (offenders.length) {
  console.error(`pluto-core depends on tauri crates: ${[...new Set(offenders)].join(", ")}`);
  process.exit(1);
}
console.log(`ok: pluto-core's ${seen.size - 1} transitive dependencies contain no tauri crates`);
