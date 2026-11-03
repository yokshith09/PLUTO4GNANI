import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ipc } from "@/ipc";
import { ymd } from "@/lib/dates";
import { RoutineCard } from "./RoutineCard";
import { useRoutineReminder } from "./useRoutine";

const notify = vi.hoisted(() => ({ info: vi.fn(), success: vi.fn(), warning: vi.fn(), error: vi.fn() }));
vi.mock("@/lib/notify", () => ({ notify }));

afterEach(() => { cleanup(); vi.restoreAllMocks(); notify.info.mockReset(); });

/** Record what the pet is asked to say until `stop()`. */
function petSaid() {
  const list: unknown[] = [];
  const listen = (e: Event) => list.push((e as CustomEvent).detail);
  window.addEventListener("pluto-pet-say", listen);
  return { list, stop: () => window.removeEventListener("pluto-pet-say", listen) };
}

/** Preferences kept in memory, like the real store. */
function store(initial: unknown) {
  // Quiet hours off so the test doesn't depend on the time it runs.
  const prefs = new Map<string, unknown>([["ui.routine", initial], ["ui.nudges", { days: {}, last: null, quiet: { on: false, from: "22:00", to: "08:00" } }]]);
  vi.spyOn(ipc, "prefsGet").mockImplementation(async (k: string) => (prefs.get(k) ?? null) as never);
  const set = vi.spyOn(ipc, "prefsSet").mockImplementation(async (k, v) => { prefs.set(k, v); return null; });
  return { set, get: () => prefs.get("ui.routine") as { done: string[]; items: { title: string }[]; remindedOn: string | null }, prefs };
}

const wrap = (node: React.ReactNode) => render(
  <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>{node}</QueryClientProvider>,
);

describe("Daily routine tile (FR-85)", () => {
  it("adds habits, ticks them off and shows progress", async () => {
    const s = store(null);
    const user = userEvent.setup();
    wrap(<RoutineCard />);
    expect(await screen.findByText("No routine yet")).toBeTruthy();
    for (const t of ["Drink water", "Revise 30 min", "Walk"]) {
      await user.type(screen.getByLabelText("New routine item"), `${t}{Enter}`);
    }
    expect(await screen.findByText("0 of 3 done")).toBeTruthy();
    await user.click(screen.getByRole("checkbox", { name: "Drink water" }));
    expect(await screen.findByText("1 of 3 done")).toBeTruthy();
    expect(screen.getByRole("checkbox", { name: "Drink water" }).getAttribute("aria-checked")).toBe("true");
    await user.click(screen.getByRole("button", { name: "Remove Walk" }));
    await user.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Remove" }));
    expect(await screen.findByText("1 of 2 done")).toBeTruthy();
    expect(s.get().items.map((i) => i.title)).toEqual(["Drink water", "Revise 30 min"]);
  });

  it("starts fresh on a new day", async () => {
    store({ items: [{ id: "a", title: "Read" }], day: "2000-01-01", done: ["a"], history: {}, remind: { on: true, at: "18:00" }, remindedOn: null });
    wrap(<RoutineCard />);
    expect(await screen.findByText("0 of 1 done")).toBeTruthy();
  });

  it("the bell turns the reminder off and on", async () => {
    store({ items: [{ id: "a", title: "Read" }], day: ymd(new Date()), done: [], history: {}, remind: { on: true, at: "18:00" }, remindedOn: null });
    const user = userEvent.setup();
    wrap(<RoutineCard />);
    await user.click(await screen.findByRole("button", { name: "Reminder at 18:00, turn off" }));
    expect(await screen.findByRole("button", { name: "Reminder off, turn on" })).toBeTruthy();
  });
});

function Reminder() { useRoutineReminder(); return null; }

describe("routine reminder (FR-85)", () => {
  const today = ymd(new Date());
  const items = [{ id: "a", title: "A" }, { id: "b", title: "B" }, { id: "c", title: "C" }];

  it("says how many are pending once the time has passed, once a day", async () => {
    const s = store({ items, day: today, done: [], history: {}, remind: { on: true, at: "00:00" }, remindedOn: null });
    wrap(<Reminder />);
    const said = petSaid();
    // Above the pet only (FR-43; nothing in the dashboard, owner 2026-10-23), and only once.
    await waitFor(() => expect(said.list).toContainEqual(expect.objectContaining({ text: "You have 3 daily routine tasks pending" })));
    await waitFor(() => expect(s.get().remindedOn).toBe(today));
    said.stop();
    expect(said.list).toHaveLength(1);
    expect(notify.info).not.toHaveBeenCalled();
  });

  it("stays quiet when everything is done or it already reminded today", async () => {
    const said = petSaid();
    store({ items, day: today, done: ["a", "b", "c"], history: {}, remind: { on: true, at: "00:00" }, remindedOn: null });
    wrap(<Reminder />);
    await new Promise((r) => setTimeout(r, 50));
    cleanup();
    store({ items, day: today, done: [], history: {}, remind: { on: true, at: "00:00" }, remindedOn: today });
    wrap(<Reminder />);
    await new Promise((r) => setTimeout(r, 50));
    said.stop();
    expect(said.list).toEqual([]);
  });

  it("respects the notification budget: no reminder if another nudge was shown in the last hour (FR-96)", async () => {
    const s = store({ items, day: today, done: [], history: {}, remind: { on: true, at: "00:00" }, remindedOn: null });
    s.prefs.set("ui.nudges", { days: {}, last: Date.now() - 5 * 60_000, quiet: { on: false, from: "22:00", to: "08:00" } });
    const said = petSaid();
    wrap(<Reminder />);
    await waitFor(() => expect(s.get().remindedOn).toBe(today));
    said.stop();
    expect(said.list).toEqual([]);
  });
});
