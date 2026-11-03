#!/usr/bin/env node
// ADR-002 / F0-01 / T-CT-04: only the shell (`src-tauri`, package `pluto`) may depend on tauri.
// `pluto-core` and every crate under `crates/` must compile and test without it, directly or
// transitively. Fails CI if that ever changes.
import { execFileSync } from "node:child_process";

const SHELL = "pluto";

const meta = JSON.parse(
  execFileSync("cargo", ["metadata", "--format-version", "1", "--locked"], {
    encoding: "utf8",
    maxBuffer: 256 * 1024 * 1024,
  }),
);

const byId = new Map(meta.packages.map((p) => [p.id, p]));
const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
const members = meta.workspace_members.map((id) => byId.get(id)).filter(Boolean);
const checked = members.filter((p) => p.name !== SHELL);
if (!checked.some((p) => p.name === "pluto-core")) {
  console.error("pluto-core not found in the workspace");
  process.exit(2);
}

let failed = false;
for (const root of checked) {
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
    console.error(`${root.name} depends on tauri crates: ${[...new Set(offenders)].join(", ")}`);
    failed = true;
  } else {
    console.log(`ok: ${root.name}'s ${seen.size - 1} transitive dependencies contain no tauri crates`);
  }
}
process.exit(failed ? 1 : 0);
