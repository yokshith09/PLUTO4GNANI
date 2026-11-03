import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  descendants,
  describeEnvironment,
  parseFootprint,
  parsePs,
  parseWindows,
  renderMarkdown,
  selectMembers,
  summarizeSamples,
  totalBytes,
  totalResidentBytes,
  verdict,
} from "./summarize.mjs";

const MB = 1024 * 1024;

// Windows-shaped fixture: pluto.exe (100) -> msedgewebview2 browser (101) -> gpu (102), renderer (103);
// plus unrelated processes, including another msedgewebview2 tree that belongs to some other app.
const winProcs = [
  { Id: 100, ProcessName: "pluto", PrivateMemorySize64: 10 * MB },
  { Id: 101, ProcessName: "msedgewebview2", PrivateMemorySize64: 30 * MB },
  { Id: 102, ProcessName: "msedgewebview2", PrivateMemorySize64: 20 * MB },
  { Id: 103, ProcessName: "msedgewebview2", PrivateMemorySize64: 25 * MB },
  { Id: 200, ProcessName: "chrome", PrivateMemorySize64: 500 * MB },
  { Id: 300, ProcessName: "msedgewebview2", PrivateMemorySize64: 70 * MB },
];
const winCim = [
  { ProcessId: 100, ParentProcessId: 1 },
  { ProcessId: 101, ParentProcessId: 100 },
  { ProcessId: 102, ParentProcessId: 101 },
  { ProcessId: 103, ParentProcessId: 101 },
  { ProcessId: 200, ParentProcessId: 1 },
  { ProcessId: 300, ParentProcessId: 250 },
];

// macOS-shaped fixture. WebKit helpers have ppid 1 (XPC/launchd), not the app.
const macPs = `
  500     1  20480 /Applications/Pluto.app/Contents/MacOS/pluto
  600     1  30720 /System/Library/Frameworks/WebKit.framework/Versions/A/XPCServices/com.apple.WebKit.WebContent.xpc/Contents/MacOS/com.apple.WebKit.WebContent
  601     1  10240 /System/Library/Frameworks/WebKit.framework/Versions/A/XPCServices/com.apple.WebKit.Networking.xpc/Contents/MacOS/com.apple.WebKit.Networking
  700     1  99999 /Applications/Safari.app/Contents/MacOS/Safari
  701     1  88888 /System/Library/Frameworks/WebKit.framework/Versions/A/XPCServices/com.apple.WebKit.WebContent.xpc/Contents/MacOS/com.apple.WebKit.WebContent
`;

describe("descendants", () => {
  it("includes the root and every transitive child, nothing else", () => {
    const procs = parseWindows(winProcs, winCim);
    assert.deepEqual([...descendants(procs, 100)].sort(), [100, 101, 102, 103]);
  });
});

describe("windows", () => {
  it("sums private bytes of the app and its webview descendants, excluding unrelated processes", () => {
    const procs = parseWindows(winProcs, winCim);
    const { members, webviewCount } = selectMembers({ platform: "win32", procs, rootPid: 100 });
    assert.equal(webviewCount, 3);
    assert.equal(totalBytes(members), (10 + 30 + 20 + 25) * MB);
  });

  it("copes with PowerShell emitting a single object instead of an array", () => {
    const procs = parseWindows(winProcs[0], winCim[0]);
    assert.equal(procs.length, 1);
    assert.equal(procs[0].bytes, 10 * MB);
  });

  it("reports zero webview processes when the webview never started (must not look like a pass)", () => {
    const procs = parseWindows([winProcs[0], winProcs[4]], winCim);
    const { members, webviewCount } = selectMembers({ platform: "win32", procs, rootPid: 100 });
    assert.equal(webviewCount, 0);
    assert.equal(totalBytes(members), 10 * MB);
  });
});

describe("macOS", () => {
  const procs = parsePs(macPs);

  it("parses ps output including full executable paths", () => {
    assert.equal(procs.length, 5);
    assert.equal(procs[0].bytes, 20480 * 1024);
  });

  it("includes WebKit helpers that appeared after launch, though they are not children of the app", () => {
    const baseline = new Set([700, 701]); // Safari and its helper were already running
    const { members, webviewCount } = selectMembers({ platform: "darwin", procs, rootPid: 500, baseline });
    assert.deepEqual(members.map((p) => p.pid).sort(), [500, 600, 601]);
    assert.equal(webviewCount, 2);
    assert.equal(totalBytes(members), (20480 + 30720 + 10240) * 1024);
  });

  it("does not count pre-existing WebKit helpers belonging to other apps", () => {
    const baseline = new Set([700, 701]);
    const { members } = selectMembers({ platform: "darwin", procs, rootPid: 500, baseline });
    assert.ok(!members.some((p) => p.pid === 701));
  });

  it("reports zero webview processes when none appeared", () => {
    const only = parsePs("  500     1  20480 /Applications/Pluto.app/Contents/MacOS/pluto\n");
    const { webviewCount } = selectMembers({ platform: "darwin", procs: only, rootPid: 500 });
    assert.equal(webviewCount, 0);
  });
});

describe("parseFootprint", () => {
  it("reads MB, KB and GB", () => {
    assert.equal(parseFootprint("pluto [1]: 64-bit    Footprint: 33 MB (16384 bytes per page)"), 33 * MB);
    assert.equal(parseFootprint("x [2]: 64-bit    Footprint: 512 KB (16384 bytes per page)"), 512 * 1024);
    assert.equal(parseFootprint("x [3]: 64-bit    Footprint: 1.5 GB (16384 bytes per page)"), 1.5 * 1024 * MB);
  });
  it("returns null rather than guessing when the line is absent", () => {
    assert.equal(parseFootprint("nothing useful here"), null);
  });
});

describe("verdict bands (plan §3 decision point 1)", () => {
  const at = (mb) => verdict(mb * MB).level;
  it("maps sizes to the documented levels at the boundaries", () => {
    assert.equal(at(79.9), "PASS");
    assert.equal(at(80), "MARGINAL");
    assert.equal(at(100), "MARGINAL");
    assert.equal(at(100.1), "OVER");
    assert.equal(at(120), "OVER");
    assert.equal(at(120.1), "FAIL");
  });
});

describe("report", () => {
  const samples = [
    { t: 0, totalBytes: 60 * MB, procs: [{ pid: 1, name: "a", bytes: 60 * MB }] },
    { t: 30, totalBytes: 90 * MB, procs: [{ pid: 1, name: "a", bytes: 20 * MB }, { pid: 2, name: "b", bytes: 70 * MB }] },
  ];
  it("takes max and mean over samples and lists processes at the peak", () => {
    const s = summarizeSamples(samples);
    assert.equal(s.maxBytes, 90 * MB);
    assert.equal(s.meanBytes, 75 * MB);
    assert.equal(s.verdict.level, "MARGINAL");
  });
  it("renders the verdict, the sample table and the caveat", () => {
    const md = renderMarkdown({ platform: "win32", metric: "private bytes", settleSeconds: 120, intervalSeconds: 30, samples });
    assert.match(md, /\*\*MARGINAL\*\*/);
    assert.match(md, /max 90\.0 MB/);
    assert.match(md, /\| 2 \| b \| 70\.0 \|/);
    assert.match(md, /Preliminary/);
  });
});

describe("secondary resident metric (Windows private working set)", () => {
  const perf = [
    { IDProcess: 100, WorkingSetPrivate: 6 * MB },
    { IDProcess: 101, WorkingSetPrivate: 12 * MB },
    { IDProcess: 102, WorkingSetPrivate: 8 * MB },
    { IDProcess: 103, WorkingSetPrivate: 9 * MB },
  ];

  it("attaches the resident figure per process without changing the gated private-bytes figure", () => {
    const procs = parseWindows(winProcs, winCim, perf);
    const { members } = selectMembers({ platform: "win32", procs, rootPid: 100 });
    assert.equal(totalBytes(members), (10 + 30 + 20 + 25) * MB);
    assert.equal(totalResidentBytes(members), (6 + 12 + 8 + 9) * MB);
  });

  it("returns null, never a partial sum, when any member lacks the figure", () => {
    const procs = parseWindows(winProcs, winCim, perf.slice(0, 2));
    const { members } = selectMembers({ platform: "win32", procs, rootPid: 100 });
    assert.equal(totalResidentBytes(members), null);
  });

  it("works with no perf data at all (macOS, or the counter being unavailable)", () => {
    const procs = parseWindows(winProcs, winCim);
    const { members } = selectMembers({ platform: "win32", procs, rootPid: 100 });
    assert.equal(totalResidentBytes(members), null);
  });

  const withResident = [
    { t: 0, totalBytes: 105 * MB, residentTotalBytes: 40 * MB, procs: [{ pid: 1, name: "a", bytes: 105 * MB, residentBytes: 40 * MB }] },
  ];

  it("keeps the verdict on private bytes and reports the resident figure as context", () => {
    const s = summarizeSamples(withResident);
    assert.equal(s.verdict.level, "OVER");
    assert.equal(s.maxResidentBytes, 40 * MB);
  });

  it("renders both figures when present and omits the secondary section when absent", () => {
    const base = { platform: "win32", metric: "private bytes", settleSeconds: 120, intervalSeconds: 30 };
    const both = renderMarkdown({ ...base, samples: withResident });
    assert.match(both, /private working set max 40\.0 MB/);
    assert.match(both, /Private working set \(MB\)/);
    const only = renderMarkdown({
      ...base,
      samples: [{ t: 0, totalBytes: 50 * MB, residentTotalBytes: null, procs: [{ pid: 1, name: "a", bytes: 50 * MB }] }],
    });
    assert.doesNotMatch(only, /working set/i);
  });
});

describe("environment line", () => {
  it("describes the runner so a number can be interpreted later", () => {
    assert.equal(
      describeEnvironment({ os: "Windows 10.0.26100", webview: "WebView2 142.0.3595.65", commit: "7b19c2d" }),
      "Windows 10.0.26100, webview WebView2 142.0.3595.65, commit 7b19c2d",
    );
  });
  it("skips whatever could not be determined instead of printing null", () => {
    assert.equal(describeEnvironment({ os: "macOS 25.0.0", webview: null, commit: null }), "macOS 25.0.0");
    assert.equal(describeEnvironment(), "");
  });
  it("appears in the report when known and is omitted when not", () => {
    const base = { platform: "win32", metric: "private bytes", settleSeconds: 120, intervalSeconds: 30, samples: [{ t: 0, totalBytes: 50 * 1048576, procs: [{ pid: 1, name: "a", bytes: 50 * 1048576 }] }] };
    assert.match(renderMarkdown({ ...base, environment: { os: "Windows 10.0.1", commit: "abc1234" } }), /Environment: Windows 10\.0\.1, commit abc1234\./);
    assert.doesNotMatch(renderMarkdown(base), /Environment:/);
  });
});
