import { describe, expect, it } from "vitest";
import type { PlutoEvent, TaskRow } from "@/ipc";
import { SLEEP_AFTER_MS, enqueue, isAsleep, reactionFor, reactionForSay, type Reaction } from "./reactions";

const row = (id: string, title = id, overdue = false): TaskRow => ({
  overdue, done_today: false,
  task: { id, title, description: "", status: "PLANNED", priority: null, project: null, planned_date: null, due_date: null, note_id: null, source_action_item_id: null, created_at: 0, updated_at: 0, started_at: null, completed_at: null, cancelled_at: null, starred: false, repeat: null, repeat_until: null, days: [], links: [] },
});
const ev = (type: string, fields: Record<string, unknown> = {}) => ({ type, at: 0, ...fields }) as PlutoEvent;

describe("pet reactions", () => {
  it("cheers when a task is done, and throws a party for the last one", () => {
    const two = { openToday: [row("a"), row("b")] };
    expect(reactionFor(ev("TaskStatusChanged", { id: "a", from: "PLANNED", to: "COMPLETED" }), two)).toMatchObject({ pose: "cheer", bubble: "Done! 1 left today", action: { id: "task-undo:a", label: "Undo" } });
    const one = { openToday: [row("a")] };
    expect(reactionFor(ev("TaskStatusChanged", { id: "a", from: "PLANNED", to: "COMPLETED" }), one)).toMatchObject({ pose: "party" });
  });

  it("catches new tasks, works on started ones, tosses deleted ones and rings for reminders", () => {
    const ctx = { openToday: [row("a", "Write the report")] };
    expect(reactionFor(ev("TaskCreated", { task: row("n", "Call the bank").task }), ctx)).toMatchObject({ pose: "catch", bubble: "Got it: “Call the bank”" });
    expect(reactionFor(ev("TaskStatusChanged", { id: "a", from: "PLANNED", to: "IN_PROGRESS" }), ctx)).toMatchObject({ pose: "work", bubble: "On it: “Write the report”" });
    expect(reactionFor(ev("TaskDeleted", { id: "a" }), ctx)).toMatchObject({ pose: "toss" });
    expect(reactionFor(ev("ReminderDue", { reminder: { id: "r1", title: "Stand up" } }), ctx)).toMatchObject({ pose: "ring", priority: 5, action: { id: "reminder-done:r1", label: "Done" } });
  });

  it("stays calm for everything else, and shortens long titles", () => {
    expect(reactionFor(ev("NoteUpdated"), { openToday: [] })).toBeNull();
    expect(reactionFor(ev("TaskStatusChanged", { id: "a", from: "IN_PROGRESS", to: "PENDING" }), { openToday: [] })).toBeNull();
    const long = reactionFor(ev("TaskCreated", { task: row("n", "A very long task title that goes on and on").task }), { openToday: [] });
    expect(long?.bubble).toBe("Got it: “A very long task title th…”");
  });

  it("sleeps only after a quiet spell, whatever the time of day", () => {
    expect(isAsleep(0, SLEEP_AFTER_MS - 1)).toBe(false);
    expect(isAsleep(0, SLEEP_AFTER_MS)).toBe(true);
  });

  it("keeps at most three waiting reactions, most important first", () => {
    const r = (priority: number): Reaction => ({ pose: "idle", bubble: String(priority), ms: 1, priority });
    let q: Reaction[] = [];
    for (const p of [1, 5, 2, 4]) q = enqueue(q, r(p));
    expect(q.map((x) => x.priority)).toEqual([5, 4, 2]);
  });
});

describe("focus sessions (FR-110)", () => {
  const session = (over: Record<string, unknown>) => ({ id: "f", kind: "FOCUS", task_id: null, label: "Physics", planned_ms: 25 * 60_000, started_at: 0, paused_at: null, paused_ms: 0, ended_at: 1, outcome: "DONE", local_date: "2026-10-23", ...over });
  it("cheers when focus is done and offers a 5-minute break on a click", () => {
    expect(reactionFor(ev("FocusEnded", { session: session({}) }), { openToday: [] })).toEqual({
      pose: "cheer", bubble: "Nice: 25 min on Physics. Break 5 min?", ms: 12_000, priority: 4, action: { id: "focus-break:5", label: "Break" },
    });
  });
  it("says the break is over without offering another, and stays quiet when stopped", () => {
    const br = reactionFor(ev("FocusEnded", { session: session({ kind: "BREAK" }) }), { openToday: [] });
    expect(br?.bubble).toBe("Break's over. Ready when you are.");
    expect(br?.action).toBeUndefined();
    expect(reactionFor(ev("FocusEnded", { session: session({ outcome: "STOPPED" }) }), { openToday: [] })).toBeNull();
  });
});

describe("messages from the dashboard (FR-43)", () => {
  const poses = ["idle", "wave", "wait"] as const;
  it("shows the text with the asked pose, or a wave", () => {
    expect(reactionForSay({ text: "You have 3 daily routine tasks pending", pose: "wait", ms: 10_000 }, poses))
      .toEqual({ pose: "wait", bubble: "You have 3 daily routine tasks pending", ms: 10_000, priority: 4 });
    expect(reactionForSay({ text: "Hi", pose: "nope" }, poses)?.pose).toBe("wave");
  });
  it("ignores empty text, shortens long text and bounds the time", () => {
    expect(reactionForSay({ text: "   " }, poses)).toBeNull();
    expect(reactionForSay({ text: "x".repeat(200) }, poses)?.bubble).toHaveLength(160);
    expect(reactionForSay({ text: "a", ms: 1 }, poses)?.ms).toBe(2000);
    expect(reactionForSay({ text: "a", ms: 999_999 }, poses)?.ms).toBe(20_000);
  });
  it("carries an action (like Undo) only when it has both an id and a label", () => {
    expect(reactionForSay({ text: "Moved to Bin", actionId: "n1", actionLabel: "Undo" }, poses)?.action).toEqual({ id: "n1", label: "Undo" });
    expect(reactionForSay({ text: "Moved to Bin", actionId: "n1" }, poses)?.action).toBeUndefined();
  });
});
