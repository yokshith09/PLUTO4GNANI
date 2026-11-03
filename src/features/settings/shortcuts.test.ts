import { describe, expect, it } from "vitest";
import { comboFromEvent, showCombo } from "./shortcuts";

const ev = (code: string, mods: Partial<Record<"ctrlKey" | "metaKey" | "altKey" | "shiftKey", boolean>> = {}) =>
  ({ key: "", code, ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, ...mods });

describe("recording a global shortcut", () => {
  it("turns Ctrl/⌘ + Shift + a letter into the stored form", () => {
    expect(comboFromEvent(ev("KeyK", { ctrlKey: true, shiftKey: true }), false)).toBe("CmdOrCtrl+Shift+K");
    expect(comboFromEvent(ev("KeyK", { metaKey: true, shiftKey: true }), true)).toBe("CmdOrCtrl+Shift+K");
    expect(comboFromEvent(ev("F9", { altKey: true }), false)).toBe("Alt+F9");
    expect(comboFromEvent({ ...ev("Unknown", { altKey: true }), key: "F9" }, false)).toBe("Alt+F9");
    expect(comboFromEvent(ev("Digit3", { ctrlKey: true, altKey: true }), false)).toBe("CmdOrCtrl+Alt+3");
  });
  it("refuses a bare key, Shift alone, or keys that aren't letters, digits or F-keys", () => {
    expect(comboFromEvent(ev("KeyK"), false)).toBeNull();
    expect(comboFromEvent(ev("KeyK", { shiftKey: true }), false)).toBeNull();
    expect(comboFromEvent(ev("Space", { ctrlKey: true }), false)).toBeNull();
    expect(comboFromEvent(ev("ControlLeft", { ctrlKey: true }), false)).toBeNull();
  });
  it("shows Ctrl on Windows", () => {
    expect(showCombo("CmdOrCtrl+Shift+L")).toMatch(/^(Ctrl\+Shift\+L|⌘⇧L)$/);
  });
});
