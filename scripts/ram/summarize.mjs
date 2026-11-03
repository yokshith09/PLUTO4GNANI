// Pure logic for the preliminary V-2 RAM measurement (F0-02, ADR-018).
// No I/O here, so every rule below is unit-tested against fixtures. The collector
// (measure.mjs) only gathers process listings and feeds them in.

const MB = 1024 * 1024;
export const BUDGET_MB = 100;

/** Every pid that is `rootPid` or a descendant of it. */
export function descendants(procs, rootPid) {
  const kids = new Map();
  for (const p of procs) {
    if (!kids.has(p.ppid)) kids.set(p.ppid, []);
    kids.get(p.ppid).push(p.pid);
  }
  const out = new Set([rootPid]);
  const stack = [rootPid];
  while (stack.length) {
    for (const k of kids.get(stack.pop()) ?? []) {
      if (!out.has(k)) {
        out.add(k);
        stack.push(k);
      }
    }
  }
  return out;
}

const asArray = (x) => (Array.isArray(x) ? x : x ? [x] : []);

/**
 * Windows: Get-Process (private bytes = committed private memory, the metric 09 specifies)
 * joined with Win32_Process (parent ids) and, optionally, the perf counter for private
 * working set (resident private memory, closer to Task Manager). The working set is a
 * secondary figure for context only; the gate stays on private bytes.
 */
export function parseWindows(processJson, cimJson, perfJson) {
  const parent = new Map(asArray(cimJson).map((c) => [c.ProcessId, c.ParentProcessId]));
  const ws = new Map(asArray(perfJson).map((c) => [c.IDProcess, c.WorkingSetPrivate]));
  return asArray(processJson).map((p) => {
    const proc = { pid: p.Id, ppid: parent.get(p.Id) ?? 0, name: p.ProcessName, bytes: p.PrivateMemorySize64 };
    if (ws.has(p.Id)) proc.residentBytes = ws.get(p.Id);
    return proc;
  });
}

/** macOS: `ps -axo pid=,ppid=,rss=,comm=` (rss in KB; comm may contain spaces). */
export function parsePs(text) {
  return text
    .split("\n")
    .map((l) => l.trim().match(/^(\d+)\s+(\d+)\s+(\d+)\s+(.*)$/))
    .filter(Boolean)
    .map((m) => ({ pid: +m[1], ppid: +m[2], name: m[4], bytes: +m[3] * 1024 }));
}

/** First line of `footprint -p PID`, e.g. "pluto [123]: 64-bit  Footprint: 33 MB (...)". */
export function parseFootprint(text) {
  const m = text.match(/Footprint:\s*([\d.]+)\s*(B|KB|MB|GB)\b/i);
  if (!m) return null;
  const unit = { b: 1, kb: 1024, mb: MB, gb: 1024 * MB }[m[2].toLowerCase()];
  return Math.round(parseFloat(m[1]) * unit);
}

const isWebview = (platform, name) =>
  platform === "win32" ? /msedgewebview2/i.test(name) : /com\.apple\.WebKit\./.test(name);

/**
 * The processes that belong to Pluto. Windows: the app and all descendants (WebView2
 * children are real children). macOS: the app and descendants, plus WebKit helper
 * processes that appeared after launch — WKWebView helpers are started over XPC and
 * are NOT children of the app, so they are matched by name and absence from `baseline`.
 */
export function selectMembers({ platform, procs, rootPid, baseline = new Set() }) {
  const tree = descendants(procs, rootPid);
  let members = procs.filter((p) => tree.has(p.pid));
  if (platform === "darwin") {
    members = members.concat(
      procs.filter((p) => isWebview(platform, p.name) && !baseline.has(p.pid) && !tree.has(p.pid)),
    );
  }
  const webviewCount = members.filter((p) => isWebview(platform, p.name)).length;
  return { members, webviewCount };
}

export const totalBytes = (members) => members.reduce((a, p) => a + p.bytes, 0);

/** Sum of the secondary resident figure, or null if any member lacks it (never a partial sum). */
export const totalResidentBytes = (members) =>
  members.length > 0 && members.every((p) => typeof p.residentBytes === "number")
    ? members.reduce((a, p) => a + p.residentBytes, 0)
    : null;

/** Bands from plan §3 CP1 decision point 1 and ADR-018. */
export function verdict(maxBytes) {
  const mb = maxBytes / MB;
  if (mb < 80) return { level: "PASS", text: "under 80 MB — comfortably within the 100 MB budget" };
  if (mb <= BUDGET_MB)
    return { level: "MARGINAL", text: "80–100 MB — within budget, but runner noise could hide a real failure; confirm on real hardware" };
  if (mb <= 120)
    return { level: "OVER", text: "100–120 MB — over budget, but within runner noise; confirm on real hardware before writing an ADR" };
  return { level: "FAIL", text: "over 120 MB — stop before F0-13 and write the V-2 ADR" };
}

const mb1 = (b) => (b / MB).toFixed(1);

export function summarizeSamples(samples) {
  const totals = samples.map((s) => s.totalBytes);
  const maxBytes = Math.max(...totals);
  const meanBytes = totals.reduce((a, b) => a + b, 0) / totals.length;
  const peak = samples[totals.indexOf(maxBytes)];
  const resident = samples.map((x) => x.residentTotalBytes).filter((x) => typeof x === "number");
  const maxResidentBytes = resident.length === samples.length ? Math.max(...resident) : null;
  return { maxBytes, meanBytes, peak, maxResidentBytes, verdict: verdict(maxBytes) };
}

/** `{ os, webview, commit }` → "Windows 10.0.26100, WebView2 142.0.3595.65, commit 7b19c2d"; missing parts are skipped. */
export function describeEnvironment({ os, webview, commit } = {}) {
  return [os, webview ? `webview ${webview}` : null, commit ? `commit ${commit}` : null].filter(Boolean).join(", ");
}

export function renderMarkdown(report) {
  const { platform, metric, settleSeconds, intervalSeconds, samples, environment } = report;
  const s = summarizeSamples(samples);
  const hasResident = s.maxResidentBytes !== null;
  const rows = s.peak.procs
    .slice()
    .sort((a, b) => b.bytes - a.bytes)
    .map((p) => `| ${p.pid} | ${p.name} | ${mb1(p.bytes)} |${hasResident ? ` ${mb1(p.residentBytes ?? 0)} |` : ""}`);
  return [
    `### V-2 preliminary RAM — ${platform} (CI runner, hello-world bundle)`,
    "",
    `**${s.verdict.level}** — ${s.verdict.text}.`,
    "",
    `Sum across all Pluto processes: **max ${mb1(s.maxBytes)} MB**, mean ${mb1(s.meanBytes)} MB over ${samples.length} samples ` +
      `(${settleSeconds}s settle, every ${intervalSeconds}s). Metric: ${metric}.`,
    environment ? `Environment: ${describeEnvironment(environment)}.` : "",
    "",
    hasResident
      ? `Secondary, for context only (the gate stays on the figure above): **private working set max ${mb1(s.maxResidentBytes)} MB** — resident private memory, closer to what Task Manager shows. Private bytes counts committed pages that may never have been touched.`
      : "",
    "",
    hasResident ? "| Sample (s) | Private bytes (MB) | Private working set (MB) |" : "| Sample (s) | Total (MB) |",
    hasResident ? "|-----------:|-----------:|-----------:|" : "|-----------:|-----------:|",
    ...samples.map((x) => `| ${x.t} | ${mb1(x.totalBytes)} |${hasResident ? ` ${mb1(x.residentTotalBytes)} |` : ""}`),
    "",
    "Processes at the peak sample:",
    "",
    hasResident ? "| PID | Process | Private bytes (MB) | Private working set (MB) |" : "| PID | Process | MB |",
    hasResident ? "|----:|---------|---:|---:|" : "|----:|---------|---:|",
    ...rows,
    "",
    "> Preliminary. A shared CI VM is not a user's machine: different webview build, no GPU, other load. " +
      "Windows reports private bytes; macOS reports physical footprint where `footprint` parses, otherwise RSS " +
      "(an upper bound that counts shared pages). This number gates work (plan §3); it does not replace the final F0-12 measurement.",
    "",
  ].join("\n");
}
