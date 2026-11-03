import { describe, expect, it } from "vitest";
import {
  MAX_ITEMS, addItem, emptyRoutine, msUntil, parseRoutine, pending, pendingMessage, removeItem, renameItem, rollOver, shouldRemind, toggle, weekCount,
} from "./routine";

const DAY = "2026-10-23";
const base = () => {
  let s = emptyRoutine(DAY);
  s = addItem(s, "Drink water", "w");
  s = addItem(s, "Revise 30 min", "r");
  s = addItem(s, "Gym", "g");
  return s;
};

describe("daily routine model (FR-85)", () => {
  it("adds, renames, ticks and removes items", () => {
    let s = base();
    expect(s.items.map((i) => i.title)).toEqual(["Drink water", "Revise 30 min", "Gym"]);
    s = renameItem(s, "g", "  Gym 45 min ");
    expect(s.items[2]?.title).toBe("Gym 45 min");
    s = toggle(s, "w");
    expect(pending(s).map((i) => i.id)).toEqual(["r", "g"]);
    s = toggle(s, "w");
    expect(pending(s)).toHaveLength(3);
    s = toggle(removeItem(toggle(s, "r"), "r"), "g");
    expect(s.done).toEqual(["g"]);
  });

  it("ignores empty titles and caps the list", () => {
    let s = addItem(emptyRoutine(DAY), "   ", "x");
    expect(s.items).toHaveLength(0);
    for (let i = 0; i < MAX_ITEMS + 5; i++) s = addItem(s, `item ${i}`, `id${i}`);
    expect(s.items).toHaveLength(MAX_ITEMS);
  });

  it("starts fresh on a new day and keeps yesterday in history", () => {
    const s = rollOver(toggle(base(), "w"), "2026-10-24");
    expect(s.day).toBe("2026-10-24");
    expect(s.done).toEqual([]);
    expect(s.history[DAY]).toEqual(["w"]);
    expect(rollOver(s, "2026-10-24")).toBe(s);
  });

  it("counts days done in the last week, and a missed day never wipes it (FR-90)", () => {
    let s = toggle(base(), "w"); // 23rd done
    s = toggle(rollOver(s, "2026-10-24"), "w"); // 24th done
    s = rollOver(s, "2026-10-25"); // 25th missed
    s = rollOver(s, "2026-10-26"); // 26th
    expect(weekCount(s, "w", "2026-10-26")).toBe(2);
    expect(weekCount(toggle(s, "w"), "w", "2026-10-26")).toBe(3);
    expect(weekCount(s, "g", "2026-10-26")).toBe(0);
    // A week later the old days fall out of the window, gently.
    expect(weekCount(rollOver(s, "2026-11-05"), "w", "2026-11-05")).toBe(0);
  });

  it("says how many are pending in plain words", () => {
    expect(pendingMessage(base())).toBe("You have 3 daily routine tasks pending");
    expect(pendingMessage(toggle(toggle(base(), "w"), "r"))).toBe("You have 1 daily routine task pending");
    expect(pendingMessage(toggle(toggle(toggle(base(), "w"), "r"), "g"))).toBeNull();
  });

  it("reminds once a day, after the chosen time, only when something is pending", () => {
    const s = base();
    expect(shouldRemind(s, new Date(`${DAY}T17:59:00`), DAY)).toBe(false);
    expect(shouldRemind(s, new Date(`${DAY}T18:00:00`), DAY)).toBe(true);
    expect(shouldRemind({ ...s, remindedOn: DAY }, new Date(`${DAY}T19:00:00`), DAY)).toBe(false);
    expect(shouldRemind({ ...s, remind: { on: false, at: "18:00" } }, new Date(`${DAY}T19:00:00`), DAY)).toBe(false);
    const allDone = toggle(toggle(toggle(s, "w"), "r"), "g");
    expect(shouldRemind(allDone, new Date(`${DAY}T19:00:00`), DAY)).toBe(false);
    expect(msUntil("18:00", new Date(`${DAY}T17:00:00`))).toBe(3_600_000);
    expect(msUntil("18:00", new Date(`${DAY}T19:00:00`))).toBe(0);
  });

  it("reads broken stored data safely and rolls it to today", () => {
    expect(parseRoutine(null, DAY).items).toEqual([]);
    const s = parseRoutine({ items: [{ id: "a", title: "A" }, { nope: 1 }], day: "2026-10-22", done: ["a"], remind: { at: "bad" } }, DAY);
    expect(s.items).toHaveLength(1);
    expect(s.done).toEqual([]);
    expect(s.history["2026-10-22"]).toEqual(["a"]);
    expect(s.remind).toEqual({ on: true, at: "18:00" });
  });
});
