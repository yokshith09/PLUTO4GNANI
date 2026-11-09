import type { Page } from "./Sidebar";

/** Pages the demo view hides (T-402). Their code and tests stay; the Bin stays so a deleted note can be restored. */
export const DEMO_HIDDEN: readonly Page[] = ["time", "character", "archive"];

export function isPageVisible(page: Page, demoOnly: boolean): boolean {
  return !(demoOnly && DEMO_HIDDEN.includes(page));
}
