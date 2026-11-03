import { describe, expect, it } from "vitest";
import type { FocusSession } from "@/ipc";
import { clock, endLine, remainingMs, workedMs } from "./focus";

const MIN = 60_000;
const s = (over: Partial<FocusSession> = {}): FocusSession => ({
  id: "f", kind: "FOCUS", task_id: null, label: "", planned_ms: 25 * MIN, started_at: 0, paused_at: null, paused_ms: 0, ended_at: null, outcome: null, local_date: "2026-10-23", ...over,
});

describe("focus time (FR-05), same answers as the core", () => {
  it("leaves out pauses and stops at zero", () => {
    expect(remainingMs(s(), 10 * MIN)).toBe(15 * MIN);
    expect(remainingMs(s({ paused_at: 10 * MIN }), 50 * MIN)).toBe(15 * MIN);
    expect(remainingMs(s({ paused_ms: 40 * MIN }), 55 * MIN)).toBe(10 * MIN);
    expect(remainingMs(s(), 999 * MIN)).toBe(0);
    expect(workedMs(s({ ended_at: 12 * MIN, outcome: "STOPPED" }), 99 * MIN)).toBe(12 * MIN);
  });

  it("reads like a clock", () => {
    expect(clock(13 * MIN + 12_000)).toBe("13:12");
    expect(clock(500)).toBe("0:01");
    expect(clock(0)).toBe("0:00");
    expect(clock(65 * MIN)).toBe("1:05:00");
  });

  it("cheers and offers a break after focus, and stays quiet when stopped", () => {
    expect(endLine(s({ outcome: "DONE", label: "Physics" }))).toEqual({ text: "Nice: 25 min on Physics. Break 5 min?", pose: "cheer", offerBreak: true });
    expect(endLine(s({ outcome: "DONE", kind: "BREAK", planned_ms: 5 * MIN }))?.offerBreak).toBe(false);
    expect(endLine(s({ outcome: "STOPPED" }))).toBeNull();
    expect(endLine(s())).toBeNull();
  });
});
