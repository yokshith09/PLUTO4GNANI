export const isMac = typeof navigator !== "undefined" && /Mac/i.test(navigator.platform);
/** The shortcut modifier as shown to the person: ⌘ on a Mac, Ctrl elsewhere. */
export const mod = isMac ? "⌘" : "Ctrl";
