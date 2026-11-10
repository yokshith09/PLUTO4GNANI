import { describe, expect, it } from "vitest";
import { extractSummary, upsertSummary } from "./summaryNote";

describe("the summary inside a meeting's notes", () => {
  it("is found under a Summary heading, with or without the #", () => {
    expect(extractSummary("## Summary\nWe ship Friday.\n\nMy notes")).toBe("We ship Friday.");
    expect(extractSummary("My notes\n\nSummary\nWe ship Friday.")).toBe("We ship Friday.");
    expect(extractSummary("just my notes")).toBeNull();
    expect(extractSummary("## Summary\n\nafter a blank")).toBeNull();
  });

  it("is added at the top of notes that have none, leaving the notes alone", () => {
    expect(upsertSummary("", "Done.")).toBe("## Summary\nDone.");
    expect(upsertSummary("My notes", "Done.")).toBe("## Summary\nDone.\n\nMy notes");
  });

  it("is replaced in place when there already is one, and nothing around it changes", () => {
    const before = "Intro line\n\n## Summary\nOld text.\n\n## Actions\nCall Ravi";
    expect(upsertSummary(before, "New text.")).toBe("Intro line\n\n## Summary\nNew text.\n\n## Actions\nCall Ravi");
    expect(extractSummary(upsertSummary(before, "New text."))).toBe("New text.");
  });

  it("an old bare 'Summary' block (auto mode before) is rewritten as a heading block", () => {
    expect(upsertSummary("Mine\n\nSummary\nOld.", "New.")).toBe("Mine\n\n## Summary\nNew.");
  });

  it("a recap with blank lines becomes one block, so the next regenerate replaces all of it", () => {
    const once = upsertSummary("Mine", "One.\n\nTwo.");
    expect(once).toBe("## Summary\nOne.\nTwo.\n\nMine");
    expect(upsertSummary(once, "Fresh.")).toBe("## Summary\nFresh.\n\nMine");
  });
});
