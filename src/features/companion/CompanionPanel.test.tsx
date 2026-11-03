import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ipc } from "@/ipc";
import { ymd } from "@/lib/dates";
import { CompanionPanel } from "./CompanionPanel";
import { useDailyGreeting } from "./useCompanion";

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

function prefs(initial: Record<string, unknown>) {
  const m = new Map<string, unknown>(Object.entries(initial));
  vi.spyOn(ipc, "prefsGet").mockImplementation(async (k: string) => (m.get(k) ?? null) as never);
  vi.spyOn(ipc, "prefsSet").mockImplementation(async (k, v) => { m.set(k, v); return null; });
  return m;
}
const quietOff = { days: {}, last: null, quiet: { on: false, from: "22:00", to: "08:00" } };
const wrap = (node: React.ReactNode) => render(
  <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>{node}</QueryClientProvider>,
);

describe("About you and your pet (FR-93)", () => {
  it("remembers the day you met as the pet's birthday and shows its age", async () => {
    const m = prefs({});
    wrap(<CompanionPanel />);
    expect(await screen.findByText("Pluto is born today")).toBeTruthy();
    await waitFor(() => expect((m.get("ui.companion") as { born: string }).born).toBe(ymd(new Date())));
  });

  it("saves your name and birthday (no year)", async () => {
    const m = prefs({ "ui.companion": { born: "2026-01-01", userName: "", userBirthday: null, greet: true, greetedOn: null } });
    const user = userEvent.setup();
    wrap(<CompanionPanel />);
    const name = await screen.findByLabelText("Your first name");
    await user.type(name, "Asha");
    await user.tab();
    await user.selectOptions(screen.getByLabelText("Birthday month"), "03");
    await user.selectOptions(screen.getByLabelText("Birthday day"), "14");
    await waitFor(() => expect(m.get("ui.companion")).toMatchObject({ userName: "Asha", userBirthday: "03-14" }));
  });

  it("saves the pet's name, counts days together and lists what comes later", async () => {
    const m = prefs({ "ui.companion": { born: "2026-01-01", userName: "", userBirthday: null, greet: true, greetedOn: null } });
    const user = userEvent.setup();
    wrap(<CompanionPanel />);
    await user.type(await screen.findByLabelText("Pet’s name"), "Biscuit");
    await user.tab();
    await waitFor(() => expect(m.get("ui.companion")).toMatchObject({ petName: "Biscuit" }));
    expect(await screen.findByText(/^Biscuit is /)).toBeTruthy();
    expect(screen.getByText(/days together/)).toBeTruthy();
    expect(screen.queryByText("Later")).toBeNull();
  });

  it("the hello can be turned off", async () => {
    const m = prefs({ "ui.companion": { born: "2026-01-01", userName: "", userBirthday: null, greet: true, greetedOn: null } });
    const user = userEvent.setup();
    wrap(<CompanionPanel />);
    await user.click(await screen.findByRole("switch", { name: "Say hello once a day" }));
    await waitFor(() => expect(m.get("ui.companion")).toMatchObject({ greet: false }));
  });
});

function Greeter() { useDailyGreeting(); return null; }

describe("hello once a day (FR-94)", () => {
  const said = (): unknown[] => {
    const out: unknown[] = [];
    window.addEventListener("pluto-pet-say", (e) => out.push((e as CustomEvent).detail));
    return out;
  };

  it("greets by name once, and not again the same day", async () => {
    const out = said();
    const m = prefs({ "ui.companion": { born: "2026-01-01", userName: "Asha", userBirthday: null, greet: true, greetedOn: null }, "ui.nudges": quietOff });
    wrap(<Greeter />);
    await waitFor(() => expect(out).toContainEqual(expect.objectContaining({ text: expect.stringMatching(/^Good (morning|afternoon|evening), Asha! I'm Pluto, your assistant\./) })));
    await waitFor(() => expect((m.get("ui.companion") as { greetedOn: string }).greetedOn).toBe(ymd(new Date())));
    cleanup();
    const before = out.length;
    wrap(<Greeter />);
    window.dispatchEvent(new Event("focus"));
    await new Promise((r) => setTimeout(r, 50));
    expect(out.length).toBe(before);
  });

  it("says hello once even when several checks start together (owner 2026-10-25)", async () => {
    const out = said();
    const m = prefs({ "ui.companion": { born: "2026-01-01", userName: "Asha", userBirthday: null, greet: true, greetedOn: null }, "ui.nudges": quietOff });
    // Two mounts and a burst of wake/focus events: the old code let each one greet.
    wrap(<><Greeter /><Greeter /></>);
    window.dispatchEvent(new Event("focus"));
    window.dispatchEvent(new Event("pluto-wake"));
    await waitFor(() => expect((m.get("ui.companion") as { greetedOn: string }).greetedOn).toBe(ymd(new Date())));
    window.dispatchEvent(new Event("focus"));
    await new Promise((r) => setTimeout(r, 50));
    expect(out.filter((s) => (s as { text: string }).text.includes("Asha"))).toHaveLength(1);
  });

  it("wishes the user a happy birthday on their birthday", async () => {
    const out = said();
    const md = ymd(new Date()).slice(5);
    prefs({ "ui.companion": { born: "2025-01-01", userName: "Asha", userBirthday: md, greet: true, greetedOn: null }, "ui.nudges": quietOff });
    wrap(<Greeter />);
    await waitFor(() => expect(out).toContainEqual(expect.objectContaining({ text: "Happy birthday, Asha! 🎂", pose: "party" })));
  });

  it("stays quiet when turned off or during quiet hours", async () => {
    const out = said();
    prefs({ "ui.companion": { born: "2026-01-01", userName: "", userBirthday: null, greet: false, greetedOn: null }, "ui.nudges": quietOff });
    wrap(<Greeter />);
    await new Promise((r) => setTimeout(r, 50));
    cleanup();
    prefs({ "ui.companion": { born: "2026-01-01", userName: "", userBirthday: null, greet: true, greetedOn: null }, "ui.nudges": { days: {}, last: null, quiet: { on: true, from: "00:00", to: "23:59" } } });
    wrap(<Greeter />);
    await new Promise((r) => setTimeout(r, 50));
    expect(out).toEqual([]);
  });

  it("catches up when the computer wakes, even with the dashboard hidden (bug S1)", async () => {
    const out = said();
    const m = prefs({
      "ui.companion": { born: "2026-01-01", userName: "Asha", userBirthday: null, greet: true, greetedOn: null },
      // A nudge a few minutes ago: the 1-hour gap holds the greeting back for now.
      "ui.nudges": { ...quietOff, last: Date.now() - 5 * 60_000 },
    });
    wrap(<Greeter />);
    await new Promise((r) => setTimeout(r, 50));
    expect(out).toEqual([]);
    // Later the PC wakes up and the gap has passed.
    m.set("ui.nudges", { ...quietOff, last: Date.now() - 2 * 60 * 60_000 });
    window.dispatchEvent(new Event("pluto-wake"));
    await waitFor(() => expect(out).toContainEqual(expect.objectContaining({ text: expect.stringContaining("Asha") })));
  });
});
