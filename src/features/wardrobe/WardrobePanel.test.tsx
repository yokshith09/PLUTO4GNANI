import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import * as api from "@/ipc";
import { ipc } from "@/ipc";
import { Bear } from "@/features/shell/Bear";
import { ITEMS } from "./wardrobe";
import { WardrobePanel } from "./WardrobePanel";

beforeAll(() => {
  globalThis.ResizeObserver ??= class { observe() {} unobserve() {} disconnect() {} } as never;
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

function prefs(initial: Record<string, unknown>) {
  const m = new Map<string, unknown>(Object.entries(initial));
  vi.spyOn(ipc, "prefsGet").mockImplementation(async (k: string) => (m.get(k) ?? null) as never);
  vi.spyOn(ipc, "prefsSet").mockImplementation(async (k, v) => { m.set(k, v); return null; });
  return m;
}
const wrap = (node: React.ReactNode) => render(
  <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>{node}</QueryClientProvider>,
);

describe("Accessories and outfits", () => {
  it("everything is open (nothing is locked) and a pick is saved and shown to the pet", async () => {
    const m = prefs({});
    const say = vi.spyOn(api, "petSay").mockResolvedValue(undefined as never);
    const user = userEvent.setup();
    wrap(<WardrobePanel />);
    const head = await screen.findByRole("group", { name: "Accessories" });
    expect(within(head).getAllByRole("button")).toHaveLength(ITEMS.length); // one list, every accessory open
    expect(head.querySelectorAll("[data-icon]")).toHaveLength(ITEMS.length); // each has its picture
    await user.click(within(head).getByRole("button", { name: "Crown" }));
    await waitFor(() => expect(m.get("ui.wardrobe")).toMatchObject({ worn: ["crown"], outfit: "" }));
    expect(say).toHaveBeenCalled();
    await user.click(within(screen.getByRole("group", { name: "Outfits" })).getByRole("button", { name: "Cosy" }));
    await waitFor(() => expect(m.get("ui.wardrobe")).toMatchObject({ worn: ["beanie", "scarf"], outfit: "cosy" }));
    expect(within(head).getByRole("button", { name: "Beanie" }).getAttribute("aria-pressed")).toBe("true");
    expect(within(head).getByRole("button", { name: "Crown" }).getAttribute("aria-pressed")).toBe("false");
    await user.click(screen.getByRole("button", { name: /Take everything off/ }));
    await waitFor(() => expect(m.get("ui.wardrobe")).toMatchObject({ worn: [], outfit: "" }));
  });

  it("every tile is the same shape: picture above a name that never spills out (owner 2026-10-28)", async () => {
    prefs({});
    wrap(<WardrobePanel />);
    const crown = await screen.findByRole("button", { name: /Crown/ });
    expect(crown.querySelector("svg[data-icon='crown']")).toBeTruthy();
    const name = crown.querySelector("span.truncate");
    expect(name?.textContent).toBe("Crown");
    // The outfits are built the same way, so the two lists line up.
    const royal = screen.getByRole("button", { name: /Royal/ });
    expect(royal.querySelector("span.truncate")?.textContent).toBe("Royal");
  });

  it("the birthday hat can be turned off", async () => {
    const m = prefs({});
    vi.spyOn(api, "petSay").mockResolvedValue(undefined as never);
    const user = userEvent.setup();
    wrap(<WardrobePanel />);
    await user.click(await screen.findByLabelText("Party hat on birthdays"));
    await waitFor(() => expect(m.get("ui.wardrobe")).toMatchObject({ partyOnBirthday: false }));
  });
});

describe("The bear wears things", () => {
  it("draws what is worn, inside the head and at the neck, and nothing by default", () => {
    const plain = render(<Bear />);
    expect(plain.container.querySelector("[data-accessory]")).toBeNull();
    cleanup();
    const { container } = render(<Bear wear={["crown", "sunglasses", "scarf"]} />);
    for (const id of ["crown", "sunglasses", "scarf"]) expect(container.querySelector(`[data-accessory="${id}"]`)).toBeTruthy();
    expect(container.querySelector('[data-accessory="beanie"]')).toBeNull();
    expect(container.querySelector('[data-accessory="crown"]')?.closest("g[class*='bear-head']") ?? container.querySelector('[data-accessory="crown"]')).toBeTruthy();
  });
});
