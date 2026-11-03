import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { ipc, type TaskRow } from "@/ipc";
import { BUBBLE_H, PACK_MOTION, PetApp, bubbleHeight, bubbleText } from "./PetApp";
import { topTasks } from "./PetCard";

beforeAll(() => {
  // jsdom has no PointerEvent; a MouseEvent carries the same button and position fields.
  window.PointerEvent ??= class extends MouseEvent {} as never;
  window.matchMedia ??= ((q: string) => ({ matches: false, media: q, addEventListener() {}, removeEventListener() {} })) as never;
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.useRealTimers();
});

const renderPet = () =>
  render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <PetApp />
    </QueryClientProvider>,
  );

const bear = () => screen.getByRole("button", { name: /^Pluto\./ });
const tap = (el: HTMLElement) => {
  fireEvent.pointerDown(el, { button: 0, buttons: 1, screenX: 10, screenY: 10 });
  fireEvent.pointerUp(el, { button: 0, screenX: 10, screenY: 10 });
};

describe("desktop pet", () => {
  it("shows a message from the dashboard above the pet, e.g. the routine reminder (FR-43, FR-85)", async () => {
    renderPet();
    await screen.findByRole("button", { name: /^Pluto\./ });
    act(() => {
      window.dispatchEvent(new CustomEvent("pluto-pet-say", { detail: { text: "You have 3 daily routine tasks pending", pose: "wait" } }));
    });
    expect((await screen.findAllByText("You have 3 daily routine tasks pending")).length).toBeGreaterThan(0);
  });

  it("a bubble with an action (like Undo) runs it when the pet is clicked, instead of opening the dashboard", async () => {
    const open = vi.spyOn(ipc, "petClick").mockResolvedValue(null);
    const sent: string[] = [];
    const onAction = (e: Event) => sent.push((e as CustomEvent<string>).detail);
    window.addEventListener("pluto-pet-action", onAction);
    renderPet();
    await screen.findByRole("button", { name: /^Pluto\./ });
    act(() => {
      window.dispatchEvent(new CustomEvent("pluto-pet-say", { detail: { text: "Moved to Bin", actionId: "n1", actionLabel: "Undo" } }));
    });
    expect((await screen.findAllByText("Moved to Bin · click: Undo")).length).toBeGreaterThan(0);
    tap(bear());
    expect(sent).toEqual(["n1"]);
    // The action is used up: the bubble loses its hint and a second click is an ordinary click.
    await waitFor(() => expect(screen.queryByText(/click: Undo/)).toBeNull());
    tap(bear());
    await waitFor(() => expect(open).toHaveBeenCalledTimes(1));
    expect(sent).toEqual(["n1"]);
    window.removeEventListener("pluto-pet-action", onAction);
  });

  it("a reminder's Done is marked right from the pet", async () => {
    const done = vi.spyOn(ipc, "reminderSetDone").mockResolvedValue(undefined as never);
    renderPet();
    await screen.findByRole("button", { name: /^Pluto\./ });
    act(() => {
      window.dispatchEvent(new CustomEvent("pluto-pet-say", { detail: { text: "Stand up", actionId: "reminder-done:r1", actionLabel: "Done" } }));
    });
    expect((await screen.findAllByText("Stand up · click: Done")).length).toBeGreaterThan(0);
    tap(bear());
    expect(done).toHaveBeenCalledWith("r1", true);
  });

  it("shows the focus timer by the pet while a session runs, with pause (FR-110)", async () => {
    renderPet();
    await screen.findByRole("button", { name: /^Pluto\./ });
    await act(async () => { await ipc.focusStart({ kind: "FOCUS", minutes: 25, task_id: null }); });
    try {
      const timer = await screen.findByRole("timer", {}, { timeout: 2000 });
      expect(timer.getAttribute("aria-label")).toMatch(/^Focus: 2[45]:\d\d left$/);
      fireEvent.click(screen.getByRole("button", { name: "Pause focus" }));
      expect(await screen.findByRole("timer", { name: /paused$/ }, { timeout: 2000 })).toBeTruthy();
    } finally {
      await act(async () => { await ipc.focusStop(); });
    }
    await waitFor(() => expect(screen.queryByRole("timer")).toBeNull());
  });

  it("cheers when focus is done, and a click starts the break (FR-110)", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    renderPet();
    await screen.findByRole("button", { name: /^Pluto\./ });
    await act(async () => { await ipc.focusStart({ kind: "FOCUS", minutes: 1, task_id: null }); });
    await act(async () => { await vi.advanceTimersByTimeAsync(62_000); });
    expect((await screen.findAllByText("Nice: 1 min. Break 5 min? · click: Break", {}, { timeout: 3000 })).length).toBeGreaterThan(0);
    tap(bear());
    await waitFor(async () => expect((await ipc.focusCurrent())?.kind).toBe("BREAK"));
    await act(async () => { await ipc.focusStop(); });
  });

  it("with the hover card off, hovering shows today as a bubble instead (FR-110)", async () => {
    const all = await ipc.settingsGetAll();
    const loaded = vi.spyOn(ipc, "settingsGetAll").mockResolvedValue({ ...all, "pet.hover_card": false });
    renderPet();
    await screen.findByRole("button", { name: /^Pluto\./ });
    await waitFor(() => expect(loaded).toHaveBeenCalled());
    await act(async () => { await new Promise((r) => setTimeout(r, 20)); });
    fireEvent.mouseEnter(bear().closest("div.flex") as HTMLElement);
    expect((await screen.findAllByText(/today · next:|Nothing planned/, {}, { timeout: 2000 })).length).toBeGreaterThan(0);
    expect(screen.queryByRole("status", { name: "Today at a glance" })).toBeNull();
  });

  it("the hover card has shortcuts: focus, add note, sleep and wake; tasks stand out (FR-111)", async () => {
    const cmd = vi.spyOn(ipc, "petCommand").mockResolvedValue(null);
    renderPet();
    fireEvent.mouseEnter(bear().closest("div.flex") as HTMLElement);
    const card = await screen.findByRole("status", { name: "Today at a glance" }, { timeout: 2000 });
    const shortcuts = within(card).getByRole("group", { name: "Shortcuts" });
    fireEvent.click(within(shortcuts).getByRole("button", { name: "Add note" }));
    expect(cmd).toHaveBeenCalledWith("new-note");
    fireEvent.click(within(shortcuts).getByRole("button", { name: "Sleep" }));
    expect(await screen.findByRole("img", { name: "Pluto the bear (sleep)" })).toBeTruthy();
    fireEvent.click(within(shortcuts).getByRole("button", { name: "Wake" }));
    await waitFor(() => expect(screen.queryByRole("img", { name: "Pluto the bear (sleep)" })).toBeNull());
    fireEvent.click(within(shortcuts).getByRole("button", { name: "Focus" }));
    expect(cmd).toHaveBeenCalledWith("focus");
    // Tasks keep their tint and open the Tasks page (H2).
    const row = within(card).queryAllByRole("button", { name: /^Open task:/ })[0];
    if (row) { fireEvent.click(row); expect(cmd).toHaveBeenCalledWith("tasks"); }
    await ipc.focusStart({ kind: "FOCUS", minutes: 25 } as never);
    // While a session runs the card has its controls: pause, +5 min, stop (owner 2026-10-27).
    const controls = await within(card).findByRole("group", { name: "Focus controls" }, { timeout: 2000 });
    fireEvent.click(within(controls).getByRole("button", { name: "Pause focus" }));
    expect(await within(controls).findByRole("button", { name: "Resume focus" })).toBeTruthy();
    const before = (await ipc.focusCurrent())?.planned_ms ?? 0;
    fireEvent.click(within(controls).getByRole("button", { name: "Add 5 minutes" }));
    await waitFor(async () => expect((await ipc.focusCurrent())?.planned_ms).toBe(before + 5 * 60_000));
    fireEvent.click(within(controls).getByRole("button", { name: "Stop focus" }));
    await waitFor(async () => expect(await ipc.focusCurrent()).toBeNull());
  });

  it("\"Sleep or wake\" in the right-click menu puts the pet to sleep and wakes it (owner 2026-10-27)", async () => {
    renderPet();
    await screen.findByRole("button", { name: /^Pluto\./ });
    act(() => { window.dispatchEvent(new CustomEvent("pluto-pet-menu", { detail: "nap" })); });
    expect(await screen.findByRole("img", { name: "Pluto the bear (sleep)" })).toBeTruthy();
    act(() => { window.dispatchEvent(new CustomEvent("pluto-pet-menu", { detail: "nap" })); });
    await waitFor(() => expect(screen.queryByRole("img", { name: "Pluto the bear (sleep)" })).toBeNull());
  });

  it("adds a task for today from the card's +, without opening the editor (owner 2026-10-27)", async () => {
    const add = vi.spyOn(ipc, "taskQuickAdd");
    renderPet();
    fireEvent.mouseEnter(bear().closest("div.flex") as HTMLElement);
    const card = await screen.findByRole("status", { name: "Today at a glance" }, { timeout: 2000 });
    fireEvent.click(within(card).getByRole("button", { name: "Add a task" }));
    const box = within(card).getByRole("textbox", { name: "New task for today" });
    fireEvent.change(box, { target: { value: "  Buy milk  " } });
    fireEvent.keyDown(box, { key: "Enter" });
    await waitFor(() => expect(add).toHaveBeenCalledWith("Buy milk"));
    await waitFor(() => expect(within(card).queryByRole("textbox")).toBeNull());
    // Escape closes without adding; an empty line adds nothing.
    fireEvent.click(within(card).getByRole("button", { name: "Add a task" }));
    fireEvent.keyDown(within(card).getByRole("textbox"), { key: "Enter" });
    fireEvent.keyDown(within(card).getByRole("textbox"), { key: "Escape" });
    expect(add).toHaveBeenCalledTimes(1);
  });

  it("the timer under the pet is as wide as the pet", async () => {
    const hover = vi.spyOn(ipc, "petHover");
    renderPet();
    await screen.findByRole("button", { name: /^Pluto\./ });
    await act(async () => { await ipc.focusStart({ kind: "FOCUS", minutes: 25, task_id: null }); });
    try {
      const timer = await screen.findByRole("timer", {}, { timeout: 2000 });
      expect(timer.style.width).toBe("128px"); // size M
      await waitFor(() => expect(hover).toHaveBeenCalledWith(true, 128, expect.any(Number), true));
    } finally {
      await act(async () => { await ipc.focusStop(); });
    }
  });

  it("the focus timer asks for the strip below the pet (H1)", async () => {
    const hover = vi.spyOn(ipc, "petHover");
    renderPet();
    await screen.findByRole("button", { name: /^Pluto\./ });
    await act(async () => { await ipc.focusStart({ kind: "FOCUS", minutes: 25, task_id: null }); });
    try {
      await screen.findByRole("timer", {}, { timeout: 2000 });
      await waitFor(() => expect(hover).toHaveBeenCalledWith(true, expect.any(Number), expect.any(Number), true));
    } finally {
      await act(async () => { await ipc.focusStop(); });
      hover.mockRestore();
    }
  });

  it("wears the saved accessories on the bear, and a say from the dashboard makes it look again", async () => {
    let saved: unknown = { worn: ["crown", "scarf"], outfit: "royal", partyOnBirthday: true };
    vi.spyOn(ipc, "prefsGet").mockImplementation(async (k: string) => (k === "ui.wardrobe" ? saved : null) as never);
    const { container } = renderPet();
    await waitFor(() => expect(container.querySelector('[data-accessory="crown"]')).toBeTruthy());
    expect(container.querySelector('[data-accessory="scarf"]')).toBeTruthy();
    saved = { worn: [], outfit: "", partyOnBirthday: true };
    act(() => { window.dispatchEvent(new CustomEvent("pluto-pet-say", { detail: { text: "Back to plain me", pose: "cheer", ms: 3500 } })); });
    await waitFor(() => expect(container.querySelector('[data-accessory]')).toBeNull());
  });

  it("has no music buttons for now: they were taken out to be planned again (owner 2026-10-27)", async () => {
    vi.spyOn(window.navigator, "userAgent", "get").mockReturnValue("Mozilla/5.0 (Windows NT 10.0; Win64; x64)");
    const all = await ipc.settingsGetAll();
    vi.spyOn(ipc, "settingsGetAll").mockResolvedValue({ ...all, "pet.media": true });
    renderPet();
    await screen.findByRole("button", { name: /^Pluto\./ });
    expect(screen.queryByRole("group", { name: "Music" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Play or pause music" })).toBeNull();
  });

  it("shows the chosen pet pack instead of the bear (FR-63)", async () => {
    const all = await ipc.settingsGetAll();
    vi.spyOn(ipc, "settingsGetAll").mockResolvedValue({ ...all, "pet.pack": "boba" });
    const sheet = vi.spyOn(ipc, "petsSheet").mockResolvedValue("data:image/webp;base64,AA==");
    renderPet();
    const pet = await screen.findByRole("img", { name: /^boba \(/ });
    expect(sheet).toHaveBeenCalledWith("boba");
    // The bear's current pose plays as one of the pack's rows (idle, or running while you work).
    expect(["idle", "running"]).toContain(pet.getAttribute("data-row"));
    expect(screen.queryByRole("img", { name: /Pluto the bear/ })).toBeNull();
  });

  it("a picture pet waves in place when you hover it, and is shown asleep after Sleep", async () => {
    const all = await ipc.settingsGetAll();
    vi.spyOn(ipc, "settingsGetAll").mockResolvedValue({ ...all, "pet.pack": "boba" });
    vi.spyOn(ipc, "petsSheet").mockResolvedValue("data:image/webp;base64,AA==");
    renderPet();
    const pet = await screen.findByRole("img", { name: /^boba \(/ });
    fireEvent.mouseEnter(pet.closest("div.flex") as HTMLElement);
    await waitFor(() => expect(screen.getByRole("img", { name: /^boba \(wave\)/ }).getAttribute("data-row")).toBe("waving"));
    // The greeting is in place: no hop, which moved the pet when the cursor came near (owner 2026-10-27).
    expect(screen.getByRole("button", { name: /^Pluto\./ }).className).not.toContain("pet-hop");
    expect(screen.getByRole("img", { name: /^boba \(wave\)/ }).className).toContain("pet-pop");
    fireEvent.click(await screen.findByRole("button", { name: /^Sleep/ }, { timeout: 2000 }));
    await waitFor(() => expect(screen.getByRole("img", { name: /^boba \(sleep\)/ }).getAttribute("data-rest")).toBe("true"));
  });

  it("poses that share a picture row still move differently", () => {
    const shared = ["ring", "toss", "catch"] as const;
    expect(new Set(shared.map((p) => PACK_MOTION[p])).size).toBe(3);
    expect(PACK_MOTION.idle).toBeUndefined();
  });

  it("falls back to the bear when the chosen pack is missing (FR-63)", async () => {
    const all = await ipc.settingsGetAll();
    vi.spyOn(ipc, "settingsGetAll").mockResolvedValue({ ...all, "pet.pack": "gone" });
    vi.spyOn(ipc, "petsSheet").mockRejectedValue({ code: "NOT_FOUND", message: "That pet is not installed." });
    renderPet();
    expect(await screen.findByRole("img", { name: /Pluto the bear/ })).toBeTruthy();
  });

  it("hovering shows today's card beside the pet", async () => {
    const hover = vi.spyOn(ipc, "petHover");
    renderPet();
    fireEvent.mouseEnter(bear().closest("div.flex") as HTMLElement);
    expect(await screen.findByRole("status", { name: "Today at a glance" }, { timeout: 2000 })).toBeTruthy();
    await waitFor(() => expect(hover).toHaveBeenCalledWith(true, expect.any(Number), expect.any(Number), false));
  });

  it("the pet keeps its place when the cursor comes near: lay out first, then grow; shrink first, then clear (owner 2026-10-27)", async () => {
    const order: string[] = [];
    vi.spyOn(ipc, "petPlan").mockImplementation(async () => { order.push("plan"); return { pet: "right", card: "above" }; });
    vi.spyOn(ipc, "petHover").mockImplementation(async (expanded) => {
      // The page is already laid out for the grown window when it grows, and still when it shrinks.
      order.push(`${expanded ? "grow" : "shrink"}:${document.querySelector("[data-pet-bear]")?.parentElement?.className.includes("justify-end") ? "corner" : "centre"}`);
      return expanded ? { pet: "right", card: "above" } : null;
    });
    renderPet();
    const root = bear().closest("div.flex") as HTMLElement;
    fireEvent.mouseEnter(root);
    await screen.findByRole("status", { name: "Today at a glance" }, { timeout: 2000 });
    fireEvent.mouseLeave(root);
    await waitFor(() => expect(order).toContain("shrink:corner"));
    expect(order.slice(order.indexOf("plan"))).toEqual(["plan", "grow:corner", "shrink:corner"]);
    await waitFor(() => expect(screen.queryByRole("status", { name: "Today at a glance" })).toBeNull());
  });

  it("the pet is invisible only while the window changes size, so no old frame shows above the card (owner 2026-10-31)", async () => {
    const seen: string[] = [];
    vi.spyOn(ipc, "petPlan").mockResolvedValue({ pet: "right", card: "above" });
    vi.spyOn(ipc, "petHover").mockImplementation(async (expanded) => {
      const root = bear().closest("div.h-screen") as HTMLElement;
      seen.push(`${expanded ? "grow" : "shrink"}:${root.style.opacity === "0" ? "hidden" : "shown"}`);
      return expanded ? { pet: "right", card: "above" } : null;
    });
    renderPet();
    const root = bear().closest("div.flex") as HTMLElement;
    fireEvent.mouseEnter(root);
    await screen.findByRole("status", { name: "Today at a glance" }, { timeout: 2000 });
    await waitFor(() => expect((bear().closest("div.h-screen") as HTMLElement).style.opacity).toBe(""));
    fireEvent.mouseLeave(root);
    await waitFor(() => expect(seen).toContain("shrink:hidden"));
    await waitFor(() => expect(screen.queryByRole("status", { name: "Today at a glance" })).toBeNull());
    await waitFor(() => expect((bear().closest("div.h-screen") as HTMLElement).style.opacity).toBe(""));
    expect(seen.slice(0, 2)).toEqual(["grow:hidden", "shrink:hidden"]);
  });

  it("one click opens the dashboard, a double click hides it, a right click opens the menu", async () => {
    vi.useFakeTimers();
    const click = vi.spyOn(ipc, "petClick").mockResolvedValue(null);
    const dbl = vi.spyOn(ipc, "petDoubleClick").mockResolvedValue(null);
    const menu = vi.spyOn(ipc, "petMenu").mockResolvedValue(null);
    renderPet();

    tap(bear());
    await act(async () => { await vi.advanceTimersByTimeAsync(400); });
    expect(click).toHaveBeenCalledTimes(1);
    expect(dbl).not.toHaveBeenCalled();

    tap(bear());
    tap(bear());
    await act(async () => { await vi.advanceTimersByTimeAsync(400); });
    expect(dbl).toHaveBeenCalledTimes(1);
    expect(click).toHaveBeenCalledTimes(1);

    fireEvent.contextMenu(bear());
    expect(menu).toHaveBeenCalledTimes(1);
  });

  it("reacts to a new task with a pose and a speech bubble", async () => {
    renderPet();
    await act(async () => { await ipc.taskQuickAdd("Buy honey"); });
    expect(await screen.findByRole("img", { name: "Pluto the bear (catch)" }, { timeout: 2000 })).toBeTruthy();
    expect(screen.getAllByText("Got it: “Buy honey”").length).toBeGreaterThan(0);
  });

  it("completing a task is one bubble that also offers Undo; clicking the pet puts the task back", { timeout: 15_000 }, async () => {
    renderPet();
    let id = "";
    await act(async () => { id = (await ipc.taskQuickAdd("Water plants")).id; });
    await act(async () => { await ipc.taskTransition(id, "COMPLETED"); });
    expect((await screen.findAllByText(/(Done! \d+ left today|All done for today!).*Undo|Undo/, undefined, { timeout: 6000 })).length).toBeGreaterThan(0);
    // exactly one completion bubble, not a reaction plus a second message
    expect(screen.queryAllByText(/Completed “Water plants”/)).toHaveLength(0);
    fireEvent.pointerDown(bear(), { button: 0, buttons: 1, screenX: 10, screenY: 10 });
    fireEvent.pointerUp(bear(), { button: 0, screenX: 10, screenY: 10 });
    await waitFor(async () => expect((await ipc.taskGet(id)).status).toBe("PLANNED"));
  });

  it("falls asleep only after 15 quiet minutes, and wakes on hover", async () => {
    vi.useFakeTimers();
    vi.spyOn(ipc, "petHover").mockResolvedValue(null);
    renderPet();
    await act(async () => { await vi.advanceTimersByTimeAsync(14 * 60_000); });
    expect(screen.queryByRole("img", { name: "Pluto the bear (sleep)" })).toBeNull();
    await act(async () => { await vi.advanceTimersByTimeAsync(2 * 60_000); });
    expect(screen.getByRole("img", { name: "Pluto the bear (sleep)" })).toBeTruthy();
    fireEvent.mouseEnter(bear().closest("div.flex") as HTMLElement);
    expect(screen.queryByRole("img", { name: "Pluto the bear (sleep)" })).toBeNull();
  });

  it("drags with the card open without shrinking the window first, then lays the card out again", async () => {
    const hover = vi.spyOn(ipc, "petHover");
    renderPet();
    fireEvent.mouseEnter(bear().closest("div.flex") as HTMLElement);
    const card = await screen.findByRole("status", { name: "Today at a glance" }, { timeout: 2000 });
    hover.mockClear();
    fireEvent.pointerDown(bear(), { button: 0, buttons: 1, screenX: 10, screenY: 10 });
    fireEvent.pointerMove(bear(), { buttons: 1, screenX: 40, screenY: 40 });
    expect(card.parentElement?.className).toContain("invisible");
    expect(hover).not.toHaveBeenCalledWith(false, 0, 0);
    // The drag is over: the next move has no button pressed.
    fireEvent.pointerMove(bear(), { buttons: 0, screenX: 40, screenY: 40 });
    await waitFor(() => expect(hover).toHaveBeenCalledWith(true, expect.any(Number), expect.any(Number), false));
    expect(hover).not.toHaveBeenCalledWith(false, 0, 0);
  });

  it("a press that starts on the card, not the bear, still drags the window (0.1.3 report)", async () => {
    const hover = vi.spyOn(ipc, "petHover");
    renderPet();
    fireEvent.mouseEnter(bear().closest("div.flex") as HTMLElement);
    const card = await screen.findByRole("status", { name: "Today at a glance" }, { timeout: 2000 });
    hover.mockClear();
    fireEvent.pointerDown(card, { button: 0, buttons: 1, screenX: 10, screenY: 10 });
    fireEvent.pointerMove(card, { buttons: 1, screenX: 40, screenY: 40 });
    expect(card.parentElement?.className).toContain("invisible");
    fireEvent.pointerMove(card, { buttons: 0, screenX: 40, screenY: 40 });
    await waitFor(() => expect(hover).toHaveBeenCalledWith(true, expect.any(Number), expect.any(Number), false));
  });

  it("a press on a card button is a click even if the mouse wobbles, so Sleep works (owner 2026-10-27)", async () => {
    renderPet();
    fireEvent.mouseEnter(bear().closest("div.flex") as HTMLElement);
    const card = await screen.findByRole("status", { name: "Today at a glance" }, { timeout: 2000 });
    const sleep = within(card).getByRole("button", { name: "Sleep" });
    fireEvent.pointerDown(sleep, { button: 0, buttons: 1, screenX: 10, screenY: 10 });
    fireEvent.pointerMove(sleep, { buttons: 1, screenX: 30, screenY: 30 }); // well past the drag distance
    expect(card.parentElement?.className).not.toContain("invisible"); // no drag started
    fireEvent.click(sleep);
    expect(within(card).getByRole("button", { name: "Wake" })).toBeTruthy();
  });

  it("a reaction that arrives while the pet is being dragged waits, so the window is not resized under the drag", async () => {
    const hover = vi.spyOn(ipc, "petHover");
    renderPet();
    fireEvent.mouseEnter(bear().closest("div.flex") as HTMLElement);
    const card = await screen.findByRole("status", { name: "Today at a glance" }, { timeout: 2000 });
    await waitFor(() => expect(hover).toHaveBeenCalledWith(true, expect.any(Number), expect.any(Number), false));
    fireEvent.pointerDown(card, { button: 0, buttons: 1, screenX: 10, screenY: 10 });
    fireEvent.pointerMove(card, { buttons: 1, screenX: 40, screenY: 40 }); // the drag starts
    hover.mockClear();
    act(() => { window.dispatchEvent(new CustomEvent("pluto-pet-say", { detail: { text: "Stretch break", pose: "wave", ms: 4000 } })); });
    await new Promise((r) => setTimeout(r, 50));
    expect(hover).not.toHaveBeenCalled(); // nothing resized mid-drag
    fireEvent.pointerMove(card, { buttons: 0, screenX: 40, screenY: 40 }); // the drag ends
    await waitFor(() => expect(hover).toHaveBeenCalledWith(true, expect.any(Number), expect.any(Number), false));
  });

  it("the circle beside a task on the card completes it; the rest of the row opens it (owner 2026-10-27)", async () => {
    const cmd = vi.spyOn(ipc, "petCommand").mockResolvedValue(null);
    const move = vi.spyOn(ipc, "taskTransition");
    await ipc.taskQuickAdd("Send invoice");
    renderPet();
    fireEvent.mouseEnter(bear().closest("div.flex") as HTMLElement);
    const card = await screen.findByRole("status", { name: "Today at a glance" }, { timeout: 2000 });
    const circle = (await within(card).findAllByRole("button", { name: /^Complete task:/ }))[0] as HTMLElement;
    fireEvent.click(circle);
    await waitFor(() => expect(move).toHaveBeenCalledWith(expect.any(String), "COMPLETED"));
    expect(cmd).not.toHaveBeenCalledWith("tasks");
  });

  it("a plain click on the card (no drag) does not open or hide the dashboard", async () => {
    const click = vi.spyOn(ipc, "petClick").mockResolvedValue(null);
    renderPet();
    fireEvent.mouseEnter(bear().closest("div.flex") as HTMLElement);
    const card = await screen.findByRole("status", { name: "Today at a glance" }, { timeout: 2000 });
    fireEvent.pointerDown(card, { button: 0, buttons: 1, screenX: 10, screenY: 10 });
    fireEvent.pointerUp(card, { button: 0, screenX: 10, screenY: 10 });
    expect(click).not.toHaveBeenCalled();
  });

  it("puts overdue, then in-progress, then high-priority tasks first", () => {
    const row = (id: string, over: Partial<TaskRow["task"]>, overdue = false): TaskRow => ({
      overdue, done_today: false,
      task: { id, title: id, description: "", status: "PLANNED", priority: null, project: null, planned_date: null, due_date: null, note_id: null, source_action_item_id: null, created_at: 0, updated_at: 0, started_at: null, completed_at: null, cancelled_at: null, starred: false, repeat: null, repeat_until: null, days: [], links: [], ...over },
    });
    const rows = [row("low", { priority: "LOW" }), row("high", { priority: "HIGH" }), row("doing", { status: "IN_PROGRESS" }), row("late", {}, true)];
    expect(topTasks(rows).map((r) => r.task.id)).toEqual(["late", "doing"]);
    expect(topTasks(rows, 4).map((r) => r.task.id)).toEqual(["late", "doing", "high", "low"]);
  });
});

describe("the hover card in its own window (owner 2026-11-01)", () => {
  const renderWithCardWindow = () =>
    render(
      <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
        <PetApp cardWindow />
      </QueryClientProvider>,
    );
  const fromCard = (detail: { kind: "hover" | "nap" | "napping"; value: boolean }) =>
    act(() => { window.dispatchEvent(new CustomEvent("pluto-pet-card", { detail })); });

  it("hovering opens the card window at the card's own size; the pet window never grows or hides", async () => {
    const card = vi.spyOn(ipc, "petCard").mockResolvedValue(null);
    const hover = vi.spyOn(ipc, "petHover");
    const plan = vi.spyOn(ipc, "petPlan");
    renderWithCardWindow();
    // A resize still finishing from the test before this one (it was unmounted mid-await) is not ours.
    await act(async () => { await new Promise((r) => setTimeout(r, 100)); });
    hover.mockClear();
    plan.mockClear();
    const root = bear().closest("div.flex") as HTMLElement;
    fireEvent.mouseEnter(root);
    await waitFor(() => expect(card).toHaveBeenCalledWith(true, 300, 404));
    expect(hover).not.toHaveBeenCalledWith(true, expect.any(Number), expect.any(Number), expect.anything());
    expect(plan).not.toHaveBeenCalled();
    expect(screen.queryByRole("status", { name: "Today at a glance" })).toBeNull();
    expect((bear().closest("div.h-screen") as HTMLElement).style.opacity).toBe("");
    fireEvent.mouseLeave(root);
    await waitFor(() => expect(card).toHaveBeenLastCalledWith(false, 300, 404));
  });

  it("stays open while the pointer is on the card, and closes once it leaves the card too", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const card = vi.spyOn(ipc, "petCard").mockResolvedValue(null);
    renderWithCardWindow();
    const root = bear().closest("div.flex") as HTMLElement;
    fireEvent.mouseEnter(root);
    await waitFor(() => expect(card).toHaveBeenCalledWith(true, 300, 404));
    fireEvent.mouseLeave(root);
    fromCard({ kind: "hover", value: true });
    await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
    expect(card).not.toHaveBeenCalledWith(false, 300, 404);
    fromCard({ kind: "hover", value: false });
    await act(async () => { await vi.advanceTimersByTimeAsync(300); });
    expect(card).toHaveBeenLastCalledWith(false, 300, 404);
  });

  it("Sleep on the card puts the pet to sleep, and the card is told to show Wake", async () => {
    vi.spyOn(ipc, "petCard").mockResolvedValue(null);
    const told: boolean[] = [];
    const listen = (e: Event) => { const m = (e as CustomEvent<{ kind: string; value: boolean }>).detail; if (m.kind === "napping") told.push(m.value); };
    window.addEventListener("pluto-pet-card", listen);
    renderWithCardWindow();
    fireEvent.mouseEnter(bear().closest("div.flex") as HTMLElement);
    await waitFor(() => expect(told).toContain(false));
    fromCard({ kind: "nap", value: true });
    await waitFor(() => expect(told[told.length - 1]).toBe(true));
    expect(await screen.findByRole("img", { name: "Pluto the bear (sleep)" })).toBeTruthy();
    window.removeEventListener("pluto-pet-card", listen);
  });

  it("a drag puts the card window away and brings it back at the new spot", async () => {
    const card = vi.spyOn(ipc, "petCard").mockResolvedValue(null);
    renderWithCardWindow();
    fireEvent.mouseEnter(bear().closest("div.flex") as HTMLElement);
    await waitFor(() => expect(card).toHaveBeenCalledWith(true, 300, 404));
    card.mockClear();
    fireEvent.pointerDown(bear(), { button: 0, buttons: 1, screenX: 10, screenY: 10 });
    fireEvent.pointerMove(bear(), { buttons: 1, screenX: 40, screenY: 40 });
    await waitFor(() => expect(card).toHaveBeenCalledWith(false, 300, 404));
    fireEvent.pointerMove(bear(), { buttons: 0, screenX: 40, screenY: 40 });
    await waitFor(() => expect(card).toHaveBeenLastCalledWith(true, 300, 404));
  });
});

describe("typing along", () => {
  const typing = (on: boolean) => act(() => { window.dispatchEvent(new CustomEvent("pluto-typing", { detail: on })); });

  it("with the switch on, the bear takes out its laptop while you type and looks up when you stop", async () => {
    const all = await ipc.settingsGetAll();
    vi.spyOn(ipc, "settingsGetAll").mockResolvedValue({ ...all, "pet.typing": true });
    renderPet();
    await waitFor(() => expect(ipc.settingsGetAll).toHaveBeenCalled());
    await act(async () => { await new Promise((r) => setTimeout(r, 50)); });
    typing(true);
    expect(await screen.findByRole("img", { name: "Pluto the bear (type)" })).toBeTruthy();
    typing(false);
    expect(await screen.findByRole("img", { name: "Pluto the bear (wait)" })).toBeTruthy();
  });

  it("with the switch off (the default), typing changes nothing", async () => {
    renderPet();
    await act(async () => { await new Promise((r) => setTimeout(r, 50)); });
    typing(true);
    expect(screen.queryByRole("img", { name: "Pluto the bear (type)" })).toBeNull();
  });
});

describe("text bubbles", () => {
  it("a short line keeps the small bubble and a long hello grows it, up to six lines", () => {
    expect(bubbleHeight("Tossed it")).toBe(BUBBLE_H);
    expect(bubbleHeight("x".repeat(64))).toBe(BUBBLE_H);
    const hello = "Good morning, Asha! I'm Mochi, your assistant. 2 tasks are overdue, starting with “Finish the sync spec”.";
    expect(bubbleHeight(hello)).toBeGreaterThan(BUBBLE_H);
    expect(bubbleHeight("x".repeat(1000))).toBe(20 + 6 * 16);
  });
  it("adds the click hint when a bubble has an action", () => {
    expect(bubbleText({ bubble: "Reminder", action: { label: "Done" } })).toBe("Reminder · click: Done");
    expect(bubbleText({ bubble: "Hi" })).toBe("Hi");
  });
});
