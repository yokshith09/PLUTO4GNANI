import { afterEach, describe, expect, it } from "vitest";
import { noteWindowId } from "./NoteWindow";

afterEach(() => {
  delete (window as unknown as { __PLUTO_NOTE__?: string }).__PLUTO_NOTE__;
  window.history.replaceState(null, "", "/");
});

describe("a note in its own window", () => {
  it("is the dashboard unless the shell or the address names a note", () => {
    expect(noteWindowId()).toBeNull();
    window.history.replaceState(null, "", "/main.html?note=n6");
    expect(noteWindowId()).toBe("n6");
    (window as unknown as { __PLUTO_NOTE__?: string }).__PLUTO_NOTE__ = "abc";
    expect(noteWindowId()).toBe("abc");
  });
});
