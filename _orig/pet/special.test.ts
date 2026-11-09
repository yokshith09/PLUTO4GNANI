import { describe, expect, it } from "vitest";
import type { PetSpecial } from "@/ipc";
import { NO_COUNTS, advance, reactionForSpecial } from "./special";

const sp = (over: Partial<PetSpecial>): PetSpecial => ({ name: "Zoomies", pose: "jump", line: "Zoomies!", trigger: "click", every: 3, ...over });

describe("a character's special", () => {
  it("plays on every Nth click and counts again from zero", () => {
    let c = NO_COUNTS;
    const fired: boolean[] = [];
    for (let i = 0; i < 7; i++) { const r = advance(c, "click", sp({})); c = r.counts; fired.push(r.fire); }
    expect(fired).toEqual([false, false, true, false, false, true, false]);
  });
  it("counts finished tasks, not clicks, for a task special", () => {
    const s = sp({ trigger: "tasks", every: 2 });
    expect(advance(NO_COUNTS, "click", s).fire).toBe(false);
    const first = advance(NO_COUNTS, "task-done", s);
    expect(first.fire).toBe(false);
    expect(advance(first.counts, "task-done", s).fire).toBe(true);
  });
  it("a daily special plays only on the first sight of the day", () => {
    const s = sp({ trigger: "daily" });
    expect(advance(NO_COUNTS, "click", s).fire).toBe(false);
    expect(advance(NO_COUNTS, "day-start", s).fire).toBe(true);
  });
  it("a pet with no special never plays one", () => {
    expect(advance(NO_COUNTS, "click", undefined).fire).toBe(false);
  });
  it("turns a special into a reaction, and an unknown pose into a wave", () => {
    expect(reactionForSpecial(sp({}))).toMatchObject({ pose: "jump", bubble: "Zoomies!" });
    expect(reactionForSpecial(sp({ pose: "moonwalk" })).pose).toBe("wave");
  });
});
