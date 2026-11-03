import { describe, expect, it } from "vitest";
import { detectTodos, parseDue, titleOf } from "./detect";

// Wednesday 28 October 2026.
const today = new Date(2026, 9, 28);

describe("reading a day out of words", () => {
  it.each([
    ["by today", "2026-10-28"],
    ["tomorrow", "2026-10-29"],
    ["by Friday", "2026-10-30"],
    ["on Monday", "2026-11-02"],
    ["next week", "2026-11-04"],
    ["by 23 Oct", "2027-10-23"],
    ["on 5th Nov", "2026-11-05"],
    ["before Dec 3", "2026-12-03"],
  ])("%s", (words, day) => expect(parseDue(`submit the report ${words}`, today)).toBe(day));
  it("a weekday that is today means next week's, and nonsense gives no day", () => {
    expect(parseDue("by Wednesday", today)).toBe("2026-11-04");
    expect(parseDue("submit the report", today)).toBeNull();
    expect(parseDue("on 31 Feb", today)).toBeNull();
  });
});

describe("finding things to do", () => {
  it("makes a clean title and reads the date", () => {
    expect(titleOf("I need to submit the lab report by Friday.")).toBe("Submit the lab report");
    expect(titleOf("- [ ] buy a charger")).toBe("Buy a charger");
    expect(titleOf("Remember to email Prof. Rao tomorrow")).toBe("Email Prof. Rao");
  });
  it("picks the lines that read like to-dos and leaves the rest", () => {
    const text = "Long day. The lecture was great.\nI need to submit the lab report by Friday.\nWent for a walk.\n- [ ] call home tomorrow\nHad to run for the bus (that is the past, not a to-do).";
    const todos = detectTodos(text, today);
    expect(todos.map((t) => [t.line, t.title, t.due])).toEqual([
      [1, "Submit the lab report", "2026-10-30"],
      [3, "Call home", "2026-10-29"],
    ]);
  });
  it("ignores empty or too-short matches", () => {
    expect(detectTodos("must\n\n- [ ]", today)).toEqual([]);
  });
});
