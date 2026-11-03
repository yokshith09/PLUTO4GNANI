import { isMac } from "@/lib/platform";

/** The global shortcuts and their defaults (src-tauri/src/shortcuts.rs). `key` = the setting. */
export const GLOBAL_SHORTCUTS = [
  { key: "shortcuts.global_open", label: "Open or hide Pluto, from any app", fallback: "CmdOrCtrl+Shift+L" },
  { key: "shortcuts.global_new_note", label: "New note, from any app", fallback: "CmdOrCtrl+Shift+N" },
  // Off by default (FR-48): an empty fallback means "none".
  { key: "shortcuts.global_pet", label: "Show or hide the pet, from any app", fallback: "" },
] as const;

/** `CmdOrCtrl+Shift+L` as the person sees it: `Ctrl+Shift+L` or `⌘⇧L`. */
export function showCombo(combo: string): string {
  const parts = combo.split("+");
  if (!isMac) return parts.map((p) => (p === "CmdOrCtrl" ? "Ctrl" : p === "Cmd" ? "Win" : p)).join("+");
  const glyph: Record<string, string> = { CmdOrCtrl: "⌘", Cmd: "⌘", Ctrl: "⌃", Alt: "⌥", Shift: "⇧" };
  return parts.map((p) => glyph[p] ?? p).join("");
}

/**
 * A key press turned into a stored combination, or null if it can't be one: it needs Ctrl/⌘ or
 * Alt (Shift alone would steal ordinary typing) and a letter, digit or F-key. Matches the core's
 * `shortcut_is_valid`.
 */
export function comboFromEvent(e: Pick<KeyboardEvent, "key" | "code" | "ctrlKey" | "metaKey" | "altKey" | "shiftKey">, mac = isMac): string | null {
  const key = /^Key[A-Z]$/.test(e.code) ? e.code.slice(3)
    : /^Digit[0-9]$/.test(e.code) ? e.code.slice(5)
    : /^F([1-9]|1[0-9]|2[0-4])$/.test(e.code) ? e.code
    : /^F([1-9]|1[0-9]|2[0-4])$/.test(e.key) ? e.key // some keyboards report no code for F-keys
    : null;
  if (!key) return null;
  const primary = mac ? e.metaKey : e.ctrlKey;
  const other = mac ? e.ctrlKey : e.metaKey;
  if (!primary && !e.altKey && !other) return null;
  return [
    primary && "CmdOrCtrl", other && (mac ? "Ctrl" : "Cmd"), e.altKey && "Alt", e.shiftKey && "Shift", key,
  ].filter(Boolean).join("+");
}
