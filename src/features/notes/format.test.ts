import { describe, expect, it } from "vitest";
import { continueList, lines, toggleCheck, withBoxes, wrap } from "./format";

describe("checklists", () => {
  it("the Checklist button works on an empty note and on a blank line", () => {
    const r = lines({ text: "", start: 0, end: 0 }, "check");
    expect(r.text).toBe("- [ ] ");
    expect(lines({ text: "a\n\nb", start: 0, end: 4 }, "check").text).toBe("- [ ] a\n\n- [ ] b");
    expect(lines({ text: "- [ ] ", start: 6, end: 6 }, "check").text).toBe(""); // pressed again: off
  });

  it("with nothing selected the caret ends up after the marker, so typing adds to it", () => {
    const r = lines({ text: "", start: 0, end: 0 }, "check");
    expect(r).toEqual({ text: "- [ ] ", start: 6, end: 6 });
    const b = lines({ text: "abc", start: 1, end: 1 }, "bullet");
    expect(b).toEqual({ text: "- abc", start: 5, end: 5 });
    const sel = lines({ text: "a\nb", start: 0, end: 3 }, "check");
    expect([sel.start, sel.end]).toEqual([0, sel.text.length]); // a range stays selected
  });

  it("ticks and unticks the item on a line, and leaves other lines alone", () => {
    const t = "intro\n- [ ] milk\n- [x] eggs";
    expect(toggleCheck(t, 2)).toBe("intro\n- [x] milk\n- [x] eggs");
    expect(toggleCheck(t, 3)).toBe("intro\n- [ ] milk\n- [ ] eggs");
    expect(toggleCheck(t, 1)).toBe(t);
    expect(toggleCheck(t, 99)).toBe(t);
  });

  it("Enter on a checklist line starts a new unticked box", () => {
    const r = continueList({ text: "- [x] milk", start: 10, end: 10 });
    expect(r?.text).toBe("- [x] milk\n- [ ] ");
    expect(r?.start).toBe(r?.text.length);
  });

  it("Enter continues bullets and counts numbers up", () => {
    expect(continueList({ text: "- a", start: 3, end: 3 })?.text).toBe("- a\n- ");
    expect(continueList({ text: "1. a", start: 4, end: 4 })?.text).toBe("1. a\n2. ");
  });

  it("Enter on an empty item ends the list", () => {
    expect(continueList({ text: "x\n- [ ] a\n- [ ] ", start: 16, end: 16 })?.text).toBe("x\n- [ ] a\n");
  });

  it("leaves Enter alone on ordinary text, with a selection, or mid-line", () => {
    expect(continueList({ text: "plain", start: 5, end: 5 })).toBeNull();
    expect(continueList({ text: "- a", start: 0, end: 3 })).toBeNull();
    expect(continueList({ text: "- [ ] ab", start: 7, end: 7 })).toBeNull();
  });

  it("cards show boxes instead of square brackets", () => {
    expect(withBoxes("- [ ] milk\n- [x] eggs")).toBe("☐ milk\n☑ eggs");
    expect(withBoxes("a [ ] b")).toBe("a [ ] b");
  });
});

describe("note formatting", () => {
  it("bolds the selection and un-bolds it again", () => {
    const b = wrap({ text: "make this bold", start: 5, end: 9 }, "**");
    expect(b.text).toBe("make **this** bold");
    expect(b.text.slice(b.start, b.end)).toBe("this");
    expect(wrap(b, "**").text).toBe("make this bold");
  });

  it("with nothing selected, puts the marks at the cursor", () => {
    const b = wrap({ text: "ab", start: 1, end: 1 }, "_");
    expect(b).toEqual({ text: "a__b", start: 2, end: 2 });
  });

  it("numbers every selected line, and a second press removes the numbers", () => {
    const text = "intro\nfirst\nsecond\noutro";
    const n = lines({ text, start: 7, end: 15 }, "number");
    expect(n.text).toBe("intro\n1. first\n2. second\noutro");
    expect(lines(n, "number").text).toBe(text);
  });

  it("switches a bullet list to a checklist", () => {
    const b = lines({ text: "a\nb", start: 0, end: 3 }, "bullet");
    expect(b.text).toBe("- a\n- b");
    expect(lines(b, "check").text).toBe("- [ ] a\n- [ ] b");
  });

  it("makes a heading of the current line", () => {
    expect(lines({ text: "Plan\nbody", start: 2, end: 2 }, "heading").text).toBe("## Plan\nbody");
  });
});
