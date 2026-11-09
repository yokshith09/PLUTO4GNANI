import { describe, expect, it } from "vitest";
import type { PlutoEvent, TaskRow } from "@/ipc";
import { REACTION_INFO, REACTION_KEYS, fillText, momentOf, parseReactionSettings, reactionWithSettings } from "./reactionSettings";

const row = (id: string, title = id): TaskRow => ({ overdue: false, done_today: false, task: { id, title } } as unknown as TaskRow);
const ev = (type: string, fields: Record<string, unknown> = {}) => ({ type, at: 0, ...fields }) as PlutoEvent;
const two = { openToday: [row("a"), row("b")] };
const done = ev("TaskStatusChanged", { id: "a", from: "PLANNED", to: "COMPLETED" });

describe("changeable reactions (J3)", () => {
  it("with nothing changed, the defaults play", () => {
    expect(reactionWithSettings(done, two, {})).toMatchObject({ pose: "cheer", bubble: "Done! 1 left today" });
  });
  it("a reaction can be turned off", () => {
    expect(reactionWithSettings(done, two, { "task-done": { enabled: false, text: "", pose: "" } })).toBeNull();
  });
  it("another pose and other words replace the defaults, filling in the values", () => {
    const r = reactionWithSettings(done, two, { "task-done": { enabled: true, text: "Nice! {left} to go", pose: "party" } });
    expect(r).toMatchObject({ pose: "party", bubble: "Nice! 1 to go" });
  });
  it("keeps the reminder's Done button and its priority", () => {
    const r = reactionWithSettings(ev("ReminderDue", { reminder: { id: "r1", title: "Stand up" } }), two, { reminder: { enabled: true, text: "Hey: {title}", pose: "" } });
    expect(r).toMatchObject({ bubble: "Hey: “Stand up”", priority: 5, action: { id: "reminder-done:r1", label: "Done" } });
  });
  it("tells the last task of the day from the others", () => {
    expect(momentOf(done, { openToday: [row("a")] })?.key).toBe("all-done");
    expect(momentOf(done, two)?.key).toBe("task-done");
    expect(momentOf(ev("NoteUpdated"), two)).toBeNull();
  });
  it("every moment has a label, a default and only tokens it can fill", () => {
    for (const k of REACTION_KEYS) {
      expect(REACTION_INFO[k].label).toBeTruthy();
      const used = [...REACTION_INFO[k].text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]);
      for (const t of used) expect(REACTION_INFO[k].tokens).toContain(t);
    }
  });
});

describe("saved reaction settings", () => {
  it("keeps good values and drops bad ones", () => {
    const s = parseReactionSettings({ "task-done": { enabled: false, text: "  Yay  ", pose: "party" }, "task-added": { pose: "nope", text: 5 }, bogus: {} });
    expect(s["task-done"]).toEqual({ enabled: false, text: "Yay", pose: "party" });
    expect(s["task-added"]).toEqual({ enabled: true, text: "", pose: "" });
    expect(Object.keys(s)).toEqual(["task-added", "task-done"]);
    expect(parseReactionSettings(null)).toEqual({});
  });
  it("fills tokens and drops the ones with no value", () => {
    expect(fillText("Got it: {title}", { title: "x" })).toBe("Got it: x");
    expect(fillText("Hi {nobody} there", {})).toBe("Hi there");
  });
});

describe("a pet's own poses (pluto.json) sit between Pluto's defaults and the user's choice", () => {
  it("uses the pet's pose when the user has not picked one", () => {
    expect(reactionWithSettings(done, two, {}, { "task-done": "jump" })?.pose).toBe("jump");
  });
  it("lets the user's pose win over the pet's", () => {
    const settings = { "task-done": { enabled: true, text: "", pose: "party" as const } };
    expect(reactionWithSettings(done, two, settings, { "task-done": "jump" })?.pose).toBe("party");
  });
  it("ignores a pose Pluto does not know, and still honours 'off'", () => {
    expect(reactionWithSettings(done, two, {}, { "task-done": "moonwalk" })?.pose).toBe("cheer");
    expect(reactionWithSettings(done, two, { "task-done": { enabled: false, text: "", pose: "" } }, { "task-done": "jump" })).toBeNull();
  });
});
