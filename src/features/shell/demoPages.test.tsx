import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { DEMO_HIDDEN, isPageVisible } from "./pages";

beforeAll(() => {
  window.matchMedia ??= ((q: string) => ({ matches: false, media: q, addEventListener() {}, removeEventListener() {} })) as never;
  window.ResizeObserver ??= class { observe() {} unobserve() {} disconnect() {} } as never;
  Element.prototype.hasPointerCapture ??= () => false;
  Element.prototype.setPointerCapture ??= () => {};
  Element.prototype.releasePointerCapture ??= () => {};
  Element.prototype.scrollIntoView ??= () => {};
});
afterEach(cleanup);

let App: typeof import("../../App").App;
let ipc: typeof import("@/ipc").ipc;
beforeEach(async () => {
  vi.resetModules(); // a fresh in-memory shell for every test
  ({ App } = await import("../../App"));
  ({ ipc } = await import("@/ipc"));
});

const side = async () => within(await screen.findByRole("complementary"));
const names = async () => (await side()).getAllByRole("button").map((b) => b.getAttribute("title") ?? b.textContent ?? "");

describe("pages.ts", () => {
  it("hides only the listed pages, and only in the demo view", () => {
    for (const p of DEMO_HIDDEN) {
      expect(isPageVisible(p, true)).toBe(false);
      expect(isPageVisible(p, false)).toBe(true);
    }
    for (const p of ["today", "tasks", "notes", "projects", "meetings", "voice", "settings", "bin"] as const) expect(isPageVisible(p, true)).toBe(true);
  });
});

describe("Demo view (T-402), against the in-memory shell", () => {
  it("shows every page by default", async () => {
    render(<App />);
    const s = await side();
    for (const n of ["Time", "Character", "MCP", "Archive", "Voice", "Meetings"]) expect(await s.findByRole("button", { name: new RegExp(`^${n}`) })).toBeTruthy();
  });

  it("hides Time, Character, MCP and Archive when the setting is on, and keeps the product pages", async () => {
    await ipc.settingSet("advanced.demo_pages_only", true);
    render(<App />);
    const s = await side();
    await s.findByRole("button", { name: /^Voice/ });
    for (const n of ["Time", "Character", "MCP", "Archive"]) expect(s.queryByRole("button", { name: new RegExp(`^${n}`) })).toBeNull();
    for (const n of ["Today overview", "Tasks", "Notes", "Projects", "Meetings", "Voice", "Bin", "Settings"]) expect(s.getByRole("button", { name: new RegExp(`^${n}`) })).toBeTruthy();
    expect((await names()).length).toBeGreaterThan(6);
  });

  it("falls back to Today when the page that was open gets hidden", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click((await side()).getByRole("button", { name: /^Time/ }));
    await waitFor(async () => expect((await side()).getByRole("button", { name: /^Time/ }).getAttribute("aria-current")).toBe("page"));
    await ipc.settingSet("advanced.demo_pages_only", true);
    await waitFor(async () => expect((await side()).queryByRole("button", { name: /^Time/ })).toBeNull());
    await waitFor(async () => expect((await side()).getByRole("button", { name: /^Today overview/ }).getAttribute("aria-current")).toBe("page"));
  });

  it("the Ctrl+4 shortcut does nothing while Time is hidden", async () => {
    const user = userEvent.setup();
    await ipc.settingSet("advanced.demo_pages_only", true);
    render(<App />);
    await (await side()).findByRole("button", { name: /^Voice/ });
    await user.keyboard("{Control>}4{/Control}");
    expect((await side()).getByRole("button", { name: /^Today overview/ }).getAttribute("aria-current")).toBe("page");
  });

  it("is a switch in Settings", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click((await side()).getByRole("button", { name: /^Settings/ }));
    const toggle = await screen.findByRole("switch", { name: "Show only the main pages" });
    await user.click(toggle);
    await waitFor(async () => expect((await side()).queryByRole("button", { name: /^Archive/ })).toBeNull());
    await user.click(screen.getByRole("switch", { name: "Show only the main pages" }));
    await waitFor(async () => expect((await side()).getByRole("button", { name: /^Archive/ })).toBeTruthy());
  });
});
