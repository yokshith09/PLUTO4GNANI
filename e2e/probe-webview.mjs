// Windows-only CI diagnostic for R11 ("DevToolsActivePort file doesn't exist").
// Starts Pluto the way msedgedriver does (debug flag in WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS)
// and reports: does WebView2 start at all, does it get the flag, where is its user-data folder,
// and does the debug port answer. Never fails the job; it only prints.
import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";

const binary = resolve(process.argv[2] ?? join("target", "debug", "pluto.exe"));
const dataDir = mkdtempSync(join(tmpdir(), "pluto-probe-"));
const PORT = 9333;
const app = spawn(binary, [], {
  stdio: "inherit",
  env: { ...process.env, PLUTO_DATA_DIR: dataDir, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}` },
});

let answered = false;
for (let i = 0; i < 80 && !answered; i++) {
  try {
    const r = await fetch(`http://127.0.0.1:${PORT}/json/version`);
    if (r.ok) { console.log(`debug port answered after ~${i * 0.5}s:`, await r.text()); answered = true; }
  } catch { /* not yet */ }
  if (!answered) await sleep(500);
}
if (!answered) console.log(`debug port ${PORT} never answered within 40 s`);

const ps = spawnSync("powershell", ["-NoProfile", "-Command",
  "Get-CimInstance Win32_Process -Filter \"Name='msedgewebview2.exe' OR Name='pluto.exe'\" | ForEach-Object { $_.Name + ' :: ' + $_.CommandLine }"], { encoding: "utf8" });
const lines = (ps.stdout || "").split(/\r?\n/).filter(Boolean);
console.log(`processes (${lines.length}):`);
for (const l of lines) console.log("  " + (l.length > 400 ? l.slice(0, 400) + "…" : l));
console.log(lines.some((l) => l.includes("msedgewebview2")) ? "WebView2 browser process: STARTED" : "WebView2 browser process: NOT FOUND");
console.log(lines.some((l) => l.includes("remote-debugging-port")) ? "debug flag reached WebView2: YES" : "debug flag reached WebView2: NO");

const ebw = join(process.env.LOCALAPPDATA ?? "", "app.pluto.desktop", "EBWebView");
console.log(`${ebw}: ${existsSync(ebw) ? readdirSync(ebw).join(", ") : "(missing)"}`);

spawnSync("taskkill", ["/pid", String(app.pid), "/T", "/F"], { stdio: "ignore" });
await sleep(2000);
try { rmSync(dataDir, { recursive: true, force: true }); } catch { /* best effort */ }
