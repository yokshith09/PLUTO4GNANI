import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PetSay } from "@/ipc";
import { ERROR_MS, TOAST_MS, bubbleText, completed, notify, setPetShown } from "./notify";

// Every notification goes above the pet as a bubble; nothing shows inside the dashboard.
let said: PetSay[] = [];
const onSay = (e: Event) => said.push((e as CustomEvent<PetSay>).detail);
beforeEach(() => {
  said = [];
  window.addEventListener("pluto-pet-say", onSay);
});
afterEach(() => {
  window.removeEventListener("pluto-pet-say", onSay);
  vi.useRealTimers();
});

describe("notify: bubbles above the pet only", () => {
  it("sends success and info to the pet for 5 seconds, with a fitting pose", () => {
    notify.success("Task added", { description: "Read chapter 3" });
    notify.info("Moved to Bin");
    expect(said).toEqual([
      { text: "Task added — Read chapter 3", pose: "cheer", ms: TOAST_MS },
      { text: "Moved to Bin", pose: "wave", ms: TOAST_MS },
    ]);
    expect(TOAST_MS).toBe(5000);
  });

  it("keeps errors longer and looks worried", () => {
    notify.error("Could not save");
    expect(said[0]).toEqual({ text: "Could not save", pose: "worried", ms: ERROR_MS });
    expect(ERROR_MS).toBeGreaterThan(TOAST_MS);
  });

  it("drops a long detail so the bubble stays short", () => {
    expect(bubbleText("Saved", "x".repeat(100))).toBe("Saved");
    expect(bubbleText("Saved", "fine")).toBe("Saved — fine");
  });

  it("offers an action such as Undo; clicking the pet runs it once", () => {
    const undo = vi.fn();
    notify.info("Moved to Bin", { action: { label: "Undo", onClick: undo } });
    const say = said[0] as PetSay;
    expect(say.actionLabel).toBe("Undo");
    expect(say.actionId).toBeTruthy();
    expect(say.ms).toBeGreaterThan(TOAST_MS);

    window.dispatchEvent(new CustomEvent("pluto-pet-action", { detail: say.actionId ?? "" }));
    expect(undo).toHaveBeenCalledTimes(1);
    window.dispatchEvent(new CustomEvent("pluto-pet-action", { detail: say.actionId ?? "" }));
    expect(undo).toHaveBeenCalledTimes(1);
  });

  it("forgets the action once the bubble is gone", () => {
    vi.useFakeTimers();
    const undo = vi.fn();
    notify.info("Moved to Bin", { action: { label: "Undo", onClick: undo } });
    vi.advanceTimersByTime(TOAST_MS + 5000);
    window.dispatchEvent(new CustomEvent("pluto-pet-action", { detail: said[0]?.actionId ?? "" }));
    expect(undo).not.toHaveBeenCalled();
  });

  it("ignores an unknown action id", () => {
    expect(() => window.dispatchEvent(new CustomEvent("pluto-pet-action", { detail: "nope" }))).not.toThrow();
  });

  it("a completion adds no bubble while the pet is on screen (its own reaction says it), and one when it is hidden", () => {
    setPetShown(true);
    completed("Water plants", vi.fn());
    expect(said).toEqual([]);
    setPetShown(false);
    completed("Water plants", vi.fn());
    setPetShown(true);
    expect(said).toEqual([]); // hidden pet: shown in the dashboard instead, never as a bubble
  });
});
