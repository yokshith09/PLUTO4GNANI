#!/usr/bin/env node
// Collector for the preliminary V-2 measurement (F0-02, ADR-018).
// Launches the built app, lets it settle, samples all Pluto processes, writes a report.
// Polling here is tooling, not product code; Principle 1 governs the app, not its test rig.
import { execFileSync, spawn } from "node:child_process";
import { appendFileSync, writeFileSync } from "node:fs";
import os from "node:os";
import { setTimeout as sleep } from "node:timers/promises";
import {
  parseFootprint,
  parsePs,
  parseWindows,
  renderMarkdown,
  selectMembers,
  summarizeSamples,
  totalBytes,
  totalResidentBytes,
} from "./summarize.mjs";

const arg = (name, fallback) => {
  const i = process.argv.indexOf(`--${name}`);
  return i > -1 ? process.argv[i + 1] : fallback;
};
const exe = arg("exe");
const settle = Number(arg("settle", 120));
const duration = Number(arg("duration", 300));
const interval = Number(arg("interval", 30));
const outBase = arg("out", "ram-report");
if (!exe) {
  console.error("usage: measure.mjs --exe <path> [--settle 120] [--duration 300] [--interval 30] [--out ram-report]");
  process.exit(2);
}
const platform = process.platform;
if (platform !== "win32" && platform !== "darwin") {
  console.error(`unsupported platform ${platform}: V-2 is measured on Windows and macOS`);
  process.exit(2);
}

const run = (cmd, args) => execFileSync(cmd, args, { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
const ps = (script) => run("powershell", ["-NoProfile", "-Command", script]);

function listProcesses() {
  if (platform === "win32") {
    const procs = JSON.parse(ps("Get-Process | Select-Object Id,ProcessName,PrivateMemorySize64 | ConvertTo-Json -Compress"));
    const cim = JSON.parse(ps("Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId | ConvertTo-Json -Compress"));
    let perf = [];
    try {
      perf = JSON.parse(
        ps("Get-CimInstance Win32_PerfFormattedData_PerfProc_Process | Select-Object IDProcess,WorkingSetPrivate | ConvertTo-Json -Compress"),
      );
    } catch {
      /* secondary figure only: its absence must never break the gated measurement */
    }
    return parseWindows(procs, cim, perf);
  }
  return parsePs(run("ps", ["-axo", "pid=,ppid=,rss=,comm="]));
}

/** macOS: prefer physical footprint (closest to Activity Monitor); fall back to RSS. */
function applyFootprint(members) {
  let allParsed = true;
  const out = members.map((p) => {
    try {
      const fp = parseFootprint(run("footprint", ["-p", String(p.pid)]));
      if (fp != null) return { ...p, bytes: fp };
    } catch {
      /* process may have exited, or footprint is unavailable */
    }
    allParsed = false;
    return p;
  });
  return { members: out, metric: allParsed ? "physical footprint" : "RSS (footprint unavailable for some processes)" };
}

/** Best-effort facts that explain a number later; a failure here must never stop the measurement. */
function environment() {
  const attempt = (f) => {
    try {
      return f().trim() || null;
    } catch {
      return null;
    }
  };
  const webview =
    platform === "win32"
      ? attempt(() =>
          ps("(Get-ItemProperty 'HKLM:\\SOFTWARE\\WOW6432Node\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}' -ErrorAction SilentlyContinue).pv"),
        )
      : attempt(() => `WebKit (macOS ${run("sw_vers", ["-productVersion"])})`);
  return {
    os: `${platform === "win32" ? "Windows" : "macOS"} ${os.release()}`,
    webview: webview && platform === "win32" ? `WebView2 ${webview}` : webview,
    commit: process.env.GITHUB_SHA ? process.env.GITHUB_SHA.slice(0, 7) : null,
  };
}

const baseline = new Set(listProcesses().map((p) => p.pid));
const child = spawn(exe, [], { stdio: "ignore", detached: false });
child.on("error", (e) => {
  console.error(`failed to launch ${exe}: ${e.message}`);
  process.exit(1);
});
console.log(`launched ${exe} (pid ${child.pid}); settling ${settle}s`);
await sleep(settle * 1000);

const samples = [];
let metric = platform === "win32" ? "private bytes" : "physical footprint";
let error = null;
for (let t = 0; t <= duration; t += interval) {
  if (child.exitCode !== null) {
    error = `app exited early with code ${child.exitCode}`;
    break;
  }
  const { members: raw, webviewCount } = selectMembers({
    platform,
    procs: listProcesses(),
    rootPid: child.pid,
    baseline,
  });
  if (webviewCount === 0) {
    // Fail loudly: reporting only the shell's few MB would look like a pass.
    error = `no webview process found at t=${t}s — the number would be meaningless`;
    break;
  }
  let members = raw;
  if (platform === "darwin") ({ members, metric } = applyFootprint(raw));
  samples.push({
    t,
    totalBytes: totalBytes(members),
    residentTotalBytes: totalResidentBytes(members),
    procs: members.map((p) => ({
      pid: p.pid,
      name: p.name.split(/[\\/]/).pop(),
      bytes: p.bytes,
      ...(typeof p.residentBytes === "number" ? { residentBytes: p.residentBytes } : {}),
    })),
  });
  console.log(`t=${t}s total=${(totalBytes(members) / 1048576).toFixed(1)} MB across ${members.length} processes`);
  if (t + interval <= duration) await sleep(interval * 1000);
}

try {
  if (platform === "win32") run("taskkill", ["/PID", String(child.pid), "/T", "/F"]);
  else child.kill();
} catch {
  /* already gone */
}

if (error) {
  console.error(`V-2 measurement failed: ${error}`);
  writeFileSync(`${outBase}.md`, `### V-2 preliminary RAM — ${platform}\n\n**ERROR** — ${error}\n`);
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, `### V-2 — ${platform}\n\n**ERROR** — ${error}\n`);
  process.exit(1);
}

const report = { platform, metric, settleSeconds: settle, intervalSeconds: interval, environment: environment(), samples, summary: summarizeSamples(samples) };
const md = renderMarkdown(report);
writeFileSync(`${outBase}.json`, JSON.stringify(report, null, 2));
writeFileSync(`${outBase}.md`, md);
if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, md);
console.log(md);
// Exit 0 whatever the verdict: this job informs a human decision (plan §3); only an unmeasurable run fails.
