import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { ipc } from "@/ipc";
import { CompanionPanel } from "./CompanionPanel";
import { ReactionsPanel } from "./ReactionsPanel";

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

describe("Reactions (J3)", () => {
  it("lists every moment with its default words, and saves a change", async () => {
    const m = prefs({});
    const user = userEvent.setup();
    wrap(<ReactionsPanel />);
    const row = await screen.findByRole("group", { name: "A task is done" });
    expect(within(row).getByPlaceholderText("Done! {left} left today")).toBeTruthy();
    await user.selectOptions(within(row).getByLabelText("Pose for: A task is done"), "party");
    await waitFor(() => expect(m.get("ui.reactions")).toMatchObject({ "task-done": { enabled: true, pose: "party", text: "" } }));
    await user.type(within(row).getByLabelText("Words for: A task is done"), "Yes! {{left} to go");
    await user.tab();
    await waitFor(() => expect(m.get("ui.reactions")).toMatchObject({ "task-done": { text: "Yes! {left} to go", pose: "party" } }));
  });

  it("turns one off and puts it back to the default", async () => {
    const m = prefs({});
    const user = userEvent.setup();
    wrap(<ReactionsPanel />);
    const row = await screen.findByRole("group", { name: "A task is deleted" });
    await user.click(within(row).getByRole("switch"));
    await waitFor(() => expect(m.get("ui.reactions")).toMatchObject({ "task-deleted": { enabled: false } }));
    await user.click(within(row).getByRole("button", { name: "Reset A task is deleted" }));
    await waitFor(() => expect(m.get("ui.reactions")).toEqual({}));
  });
});

describe("Say hello now (J3)", () => {
  it("shows the hello as a text bubble above the pet, with its name and what is waiting", async () => {
    prefs({ "ui.companion": { born: "2026-01-01", petName: "Mochi", userName: "Asha", userBirthday: null, greet: true, greetedOn: null } });
    const said: { text: string }[] = [];
    const bridge = await import("@/ipc");
    vi.spyOn(bridge, "petSay").mockImplementation(async (m) => { said.push(m as never); });
    const user = userEvent.setup();
    wrap(<CompanionPanel />);
    await user.click(await screen.findByRole("button", { name: /Say hello now/ }));
    await waitFor(() => expect(said).toHaveLength(1));
    expect(said[0]?.text).toMatch(/I'm Mochi, your assistant\./);
  });
});
