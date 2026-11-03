import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

beforeAll(() => {
  // jsdom lacks these; Radix and the theme hook use them.
  window.matchMedia ??= ((q: string) => ({ matches: false, media: q, addEventListener() {}, removeEventListener() {} })) as never;
  window.ResizeObserver ??= class { observe() {} unobserve() {} disconnect() {} } as never;
  Element.prototype.hasPointerCapture ??= () => false;
  Element.prototype.setPointerCapture ??= () => {};
  Element.prototype.releasePointerCapture ??= () => {};
  Element.prototype.scrollIntoView ??= () => {};
});
afterEach(cleanup);

// Notifications go above the pet, never inside the dashboard: record what the pet is asked to say.
type Said = { text: string; actionId?: string; actionLabel?: string };
let said: Said[] = [];
const onSay = (e: Event) => said.push((e as CustomEvent<Said>).detail);
beforeEach(() => {
  said = [];
  window.addEventListener("pluto-pet-say", onSay);
});
afterEach(() => window.removeEventListener("pluto-pet-say", onSay));
/** Wait until the pet says a bubble matching `text`. */
const petSays = (text: string | RegExp) =>
  waitFor(() => {
    const hit = said.find((s) => (typeof text === "string" ? s.text === text : text.test(s.text)));
    expect(hit, `pet bubble ${String(text)}; got ${JSON.stringify(said.map((s) => s.text))}`).toBeTruthy();
    return hit as Said;
  });
/** Click the pet while it offers `label` (like Undo) in its bubble. */
const clickPetAction = (s: Said) => window.dispatchEvent(new CustomEvent("pluto-pet-action", { detail: s.actionId ?? "" }));

// A fresh in-memory shell (and so fresh preferences) for every test.
let App: typeof import("./App").App;
beforeEach(async () => {
  vi.resetModules();
  ({ App } = await import("./App"));
});

/** Click a sidebar item (the Overview also has tiles with the same names). */
async function goTo(user: ReturnType<typeof userEvent.setup>, name: RegExp) {
  const side = await screen.findByRole("complementary");
  await user.click(within(side).getByRole("button", { name }));
}
const openNotes = (user: ReturnType<typeof userEvent.setup>) => goTo(user, /^Notes/);

describe("Notes screen (against the in-memory shell)", () => {
  it("opens on Today overview: agenda, focus, routine, time and links, without search or New note", async () => {
    render(<App />);
    expect(await screen.findByRole("region", { name: "Today’s agenda" })).toBeTruthy();
    for (const name of ["Focus", "Daily routine", "Total time today", "Apps", "Websites", "Links"]) {
      expect(await screen.findByRole("region", { name })).toBeTruthy();
    }
    // Owner 2026-10-25: no activity feed and no week count on Today.
    expect(screen.queryByRole("region", { name: "Activity" })).toBeNull();
    expect(screen.queryByRole("region", { name: "Done this week" })).toBeNull();
    // The quote of the day sits beside the greeting.
    expect(await screen.findByRole("button", { name: "Edit quotes" })).toBeTruthy();
    expect(screen.queryByRole("textbox", { name: "Search notes" })).toBeNull();
    expect(screen.queryByRole("button", { name: /New note/ })).toBeNull();
  });

  it("Total time lists every app and website, and opens the active spans", async () => {
    const user = userEvent.setup();
    render(<App />);
    const apps = await screen.findByRole("region", { name: "Apps" });
    const sites = await screen.findByRole("region", { name: "Websites" });
    expect(await within(sites).findByRole("button", { name: /^github\.com/ })).toBeTruthy();
    await user.click(await within(apps).findByRole("button", { name: /^VS Code,/ }));
    const dialog = await screen.findByRole("dialog");
    expect(await within(dialog).findByText(/Active from/)).toBeTruthy();
  });

  it("a task opens from Today in the full editor: priority and deadline, then back to Today", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole("button", { name: "Edit Refactor the tray menu" }));
    const dialog = await screen.findByRole("dialog");
    // Each setting says what it is for (owner 2026-10-25).
    expect(within(dialog).getByText("The last day it can be done. After that it shows as overdue.")).toBeTruthy();
    await user.click(within(dialog).getByRole("radio", { name: "High" }));
    await user.type(within(dialog).getByLabelText("Deadline date"), "2030-05-04");
    await user.click(within(dialog).getByRole("button", { name: "Save changes" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    const row = (await screen.findByRole("button", { name: "Edit Refactor the tray menu" })).closest("li") as HTMLElement;
    expect(await within(row).findByText(/High/)).toBeTruthy();
    expect(within(row).getByTitle("Deadline")).toBeTruthy();
  });

  it("an overdue task on Today says how many days late it is", async () => {
    render(<App />);
    const late = await screen.findByRole("region", { name: "Overdue" });
    expect(within(late).getByRole("button", { name: "Edit Finish the sync spec" })).toBeTruthy();
    expect(within(late).getByText(/overdue$/)).toBeTruthy();
  });

  it("browsers list their sites with time under the browser", async () => {
    render(<App />);
    const list = await screen.findByRole("list", { name: "Sites in Google Chrome" });
    expect(await within(list).findByRole("button", { name: /^github\.com/ })).toBeTruthy();
  });

  it("the demo time data does not depend on the time of day (00:20 local)", async () => {
    const early = new Date();
    early.setHours(0, 20, 0, 0);
    vi.useFakeTimers({ toFake: ["Date"], now: early });
    try {
      vi.resetModules();
      ({ App } = await import("./App"));
      render(<App />);
      const sites = await screen.findByRole("region", { name: "Websites" });
      expect(await within(sites).findByRole("button", { name: /^github\.com/ })).toBeTruthy();
    } finally {
      vi.useRealTimers();
    }
  });

  it("every overview tile has a move handle", async () => {
    render(<App />);
    for (const t of ["Today", "Focus", "Daily routine", "Total time today", "Links"]) {
      expect(await screen.findByRole("button", { name: `Move ${t}` })).toBeTruthy();
    }
  });

  it("Tasks page: views, filters and search", async () => {
    const user = userEvent.setup();
    render(<App />);
    await goTo(user, /^Tasks/);
    const list = await screen.findByRole("region", { name: "Task list" });
    expect(await within(list).findByRole("button", { name: "Open Design the notes screen" })).toBeTruthy();
    await user.click(screen.getByRole("radio", { name: "All" }));
    expect(await within(list).findByRole("button", { name: "Open Write release notes" })).toBeTruthy();
    await user.type(screen.getByRole("textbox", { name: "Search tasks" }), "release");
    expect(within(list).queryByRole("button", { name: "Open Design the notes screen" })).toBeNull();
    expect(within(list).getByRole("button", { name: "Open Write release notes" })).toBeTruthy();
    await user.click(screen.getByRole("radio", { name: "Completed" }));
    await user.clear(screen.getByRole("textbox", { name: "Search tasks" }));
    expect(await within(list).findByRole("button", { name: "Open Reply to Sam" })).toBeTruthy();
  });

  it("a task can be started, logged, deferred and finished from its detail", async () => {
    const user = userEvent.setup();
    render(<App />);
    await goTo(user, /^Tasks/);
    await user.click(await screen.findByRole("button", { name: "Open Refactor the tray menu" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(await within(dialog).findByRole("button", { name: "Start" }));
    expect(await within(dialog).findByRole("button", { name: "Pause" })).toBeTruthy();
    await user.type(within(dialog).getByRole("textbox", { name: "New work update" }), "wired the menu{Enter}");
    expect(await within(dialog).findByText("wired the menu")).toBeTruthy();
    await user.click(within(dialog).getByRole("button", { name: "Defer to tomorrow" }));
    await user.click(await within(dialog).findByRole("button", { name: "Complete" }));
    expect(await within(dialog).findByRole("button", { name: "Reopen" })).toBeTruthy();
    // A finished task can be deleted, after a confirmation.
    await user.click(within(dialog).getByRole("button", { name: "Delete task" }));
    await user.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(screen.queryByRole("button", { name: "Open Refactor the tray menu" })).toBeNull();
  });

  it("a task makes a linked note, opens it, and the note lists the task back (FR-42)", async () => {
    const user = userEvent.setup();
    render(<App />);
    await goTo(user, /^Tasks/);
    await user.click(await screen.findByRole("button", { name: "Open Refactor the tray menu" }));
    const task = await screen.findByRole("dialog");
    await user.click(within(task).getByRole("button", { name: "New note from this task" }));
    // The note opens on top, titled like the task, and shows the task under "Linked tasks".
    const linked = await screen.findByRole("region", { name: "Linked tasks" });
    expect(within(linked).getByRole("button", { name: "Open task Refactor the tray menu" })).toBeTruthy();
    const dialogs = screen.getAllByRole("dialog");
    const note = dialogs[dialogs.length - 1] as HTMLElement;
    expect((within(note).getByRole("textbox", { name: "Title" }) as HTMLInputElement).value).toBe("Refactor the tray menu");
    await user.keyboard("{Escape}");
    // Back on the task: the link now opens the note instead of offering a new one.
    expect(await within(task).findByRole("button", { name: "Open note" })).toBeTruthy();
  });

  it("Notes shows the Links card too (FR-42, V2-08)", async () => {
    const user = userEvent.setup();
    render(<App />);
    await openNotes(user);
    expect(await screen.findByRole("button", { name: "Add link" })).toBeTruthy();
  });

  it("saving a task says so and goes back to where it was opened (FR-41)", async () => {
    const user = userEvent.setup();
    render(<App />);
    await goTo(user, /^Tasks/);
    await user.click(await screen.findByRole("button", { name: "Open Refactor the tray menu" }));
    const dialog = await screen.findByRole("dialog");
    const title = within(dialog).getByRole("textbox", { name: "Title" });
    await user.clear(title);
    await user.type(title, "Refactor the tray");
    await user.click(within(dialog).getByRole("button", { name: "Save changes" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await petSays("Task saved");
    expect(screen.queryByText("Task saved")).toBeNull(); // nothing pops up inside the dashboard
    expect(await screen.findByRole("button", { name: "Open Refactor the tray" })).toBeTruthy();
  });

  it("Save with nothing changed still closes the task (owner 2026-10-25)", async () => {
    const user = userEvent.setup();
    render(<App />);
    await goTo(user, /^Tasks/);
    await user.click(await screen.findByRole("button", { name: "Open Refactor the tray menu" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Save changes" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("a note previews as Markdown, never as raw HTML", async () => {
    const user = userEvent.setup();
    render(<App />);
    await openNotes(user);
    await user.click(await screen.findByRole("button", { name: "Groceries" }));
    const dialog = await screen.findByRole("dialog");
    const body = within(dialog).getByRole("textbox", { name: "Note" });
    await user.clear(body);
    await user.click(body);
    await user.paste("# Big heading\n\n- [x] done item\n\n<b>raw</b> text");
    await user.click(within(dialog).getByRole("button", { name: "Preview" }));
    const preview = within(dialog).getByLabelText("Preview");
    expect(within(preview).getByRole("heading", { level: 1, name: "Big heading" })).toBeTruthy();
    const box = within(preview).getByRole("checkbox") as HTMLInputElement;
    expect(box.checked).toBe(true);
    expect(box.disabled).toBe(false); // ticking in the preview edits the note
    expect(preview.textContent).toContain("<b>raw</b> text");
    expect(preview.querySelector("b")).toBeNull();
    await user.click(box);
    expect((within(preview).getByRole("checkbox") as HTMLInputElement).checked).toBe(false);
    await user.click(within(dialog).getByRole("button", { name: "Edit" }));
    expect((within(dialog).getByRole("textbox", { name: "Note" }) as HTMLTextAreaElement).value).toContain("- [ ] done item");
    await user.click(within(dialog).getByRole("button", { name: "Preview" }));
    await user.click(within(dialog).getByRole("button", { name: "Edit" }));
    expect(within(dialog).getByRole("textbox", { name: "Note" })).toBeTruthy();
  });

  it("Enter in a checklist starts the next box", async () => {
    const user = userEvent.setup();
    render(<App />);
    await openNotes(user);
    await user.click(await screen.findByRole("button", { name: "Groceries" }));
    const dialog = await screen.findByRole("dialog");
    const body = within(dialog).getByRole("textbox", { name: "Note" }) as HTMLTextAreaElement;
    await user.clear(body);
    await user.click(within(dialog).getByRole("button", { name: "Checklist" }));
    // No click into the box first: the caret must already be after the marker, or the first
    // letter typed replaces it (the bug the owner hit in the real window).
    await user.keyboard("milk{Enter}eggs");
    expect(body.value).toBe("- [ ] milk\n- [ ] eggs");
  });

  it("note editor shortcuts: Ctrl+D pins, Ctrl+E toggles preview", async () => {
    const user = userEvent.setup();
    render(<App />);
    await openNotes(user);
    await user.click(await screen.findByRole("button", { name: "Groceries" }));
    const dialog = await screen.findByRole("dialog");
    await user.keyboard("{Control>}d{/Control}");
    expect(await within(dialog).findByRole("button", { name: "Unpin" })).toBeTruthy();
    await user.keyboard("{Control>}e{/Control}");
    expect(within(dialog).getByLabelText("Preview")).toBeTruthy();
  });

  it("labels can be renamed and deleted from the label manager", async () => {
    const user = userEvent.setup();
    render(<App />);
    await openNotes(user);
    await user.click(await screen.findByRole("button", { name: "Manage labels" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Rename pet" }));
    const input = within(dialog).getByRole("textbox", { name: "New name for pet" });
    await user.clear(input);
    await user.type(input, "pets{Enter}");
    expect(await within(dialog).findByText("pets")).toBeTruthy();
    await user.click(within(dialog).getByRole("button", { name: "Delete home" }));
    await user.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(within(dialog).queryByText("home")).toBeNull());
    // A name that is taken is refused and the label keeps its name.
    await user.click(within(dialog).getByRole("button", { name: "Rename work" }));
    const second = within(dialog).getByRole("textbox", { name: "New name for work" });
    await user.clear(second);
    await user.type(second, "IDEAS{Enter}");
    expect(within(dialog).getByRole("textbox", { name: "New name for work" })).toBeTruthy();
  });

  it("the Time page lists past days with done of planned, and a day opens to its tasks", async () => {
    const user = userEvent.setup();
    render(<App />);
    await goTo(user, /^Time/);
    const log = await screen.findByRole("region", { name: "Daily log" });
    const yesterday = (await within(log).findAllByRole("button", { name: /^.+: \d+ of \d+ done$/, expanded: false }))[0] as HTMLElement;
    await user.click(yesterday);
    expect(yesterday.getAttribute("aria-expanded")).toBe("true");
    expect(within(log).getAllByRole("button", { name: /Copy .+ as Markdown/ }).length).toBeGreaterThan(0);
  });

  it("Time › website tracking is off by default and needs app tracking", async () => {
    const user = userEvent.setup();
    render(<App />);
    await goTo(user, /^Time/);
    // The tracking switches are in a dialog opened from the page header, not a tile at the bottom.
    await user.click(await screen.findByRole("button", { name: "Tracking and privacy settings" }));
    const sites = await screen.findByRole("switch", { name: "Track websites" });
    expect(sites.getAttribute("aria-checked")).toBe("false");
    await user.click(sites);
    await waitFor(() => expect(sites.getAttribute("aria-checked")).toBe("true"));
    await user.click(screen.getByRole("switch", { name: "Track app usage" }));
    await waitFor(() => expect((sites as HTMLButtonElement).disabled).toBe(true));
  });

  it("Settings holds no pet or tracking settings; they live on Character, Time and Today", async () => {
    const user = userEvent.setup();
    render(<App />);
    await goTo(user, /^Settings/);
    await screen.findByRole("heading", { name: "General" });
    expect(screen.queryByRole("switch", { name: "Type along with me" })).toBeNull();
    expect(screen.queryByRole("switch", { name: "Track app usage" })).toBeNull();
    await goTo(user, /^Character/);
    expect(await screen.findByRole("switch", { name: "Type along with me" })).toBeTruthy();
    await goTo(user, /^Time/);
    await user.click(await screen.findByRole("button", { name: "Tracking and privacy settings" }));
    expect(await screen.findByRole("switch", { name: "Track websites" })).toBeTruthy();
    await user.keyboard("{Escape}");
    await goTo(user, /^Today/);
    expect((await screen.findAllByText("Reminder alerts")).length).toBeGreaterThan(0);
  });

  it("Settings › Your data: export, refused import into a non-empty Pluto, delete all with the word DELETE", async () => {
    const user = userEvent.setup();
    render(<App />);
    await goTo(user, /^Settings/);
    await user.click(await screen.findByRole("button", { name: "Export…" }));
    await petSays(/^Export saved/);

    await user.click(screen.getByRole("button", { name: "Import…" }));
    await user.click(await screen.findByRole("button", { name: "Choose file…" }));
    await petSays(/Import only works into an empty Pluto/);

    await user.click(screen.getByRole("button", { name: "Delete all…" }));
    const dialog = await screen.findByRole("dialog");
    const confirm = within(dialog).getByRole("button", { name: "Delete everything" }) as HTMLButtonElement;
    expect(confirm.disabled).toBe(true);
    await user.type(within(dialog).getByRole("textbox", { name: "Type DELETE to confirm" }), "delete");
    expect(confirm.disabled).toBe(true);
    await user.clear(within(dialog).getByRole("textbox", { name: "Type DELETE to confirm" }));
    await user.type(within(dialog).getByRole("textbox", { name: "Type DELETE to confirm" }), "DELETE");
    await user.click(confirm);
    await petSays(/^Everything was deleted/);
    // An empty Pluto starts over at the welcome screen; past it, Notes is empty.
    await user.click(await screen.findByRole("button", { name: "Start" }, { timeout: 3000 }));
    await user.click(await screen.findByRole("button", { name: "Skip" }));
    await user.click(await screen.findByRole("button", { name: "Skip tour" }));
    await openNotes(user);
    expect(await screen.findByText("No notes yet")).toBeTruthy();
  });

  it("keyboard: Ctrl+3 opens Notes, Ctrl+T focuses a new task, Ctrl+, opens Settings", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("region", { name: "Today’s agenda" });
    await user.keyboard("{Control>}3{/Control}");
    expect(await screen.findByRole("textbox", { name: "Search notes" })).toBeTruthy();
    await user.keyboard("{Control>}t{/Control}");
    const field = await screen.findByRole("textbox", { name: "New task" });
    await waitFor(() => expect(document.activeElement).toBe(field));
    await user.keyboard("{Control>},{/Control}");
    expect(await screen.findByRole("heading", { name: "Settings" })).toBeTruthy();
  });

  it("pasted links become shortcuts", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole("button", { name: "Add link" }));
    await user.type(await screen.findByLabelText("Paste links"), "linear.app/team");
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByRole("button", { name: "Open linear.app" })).toBeTruthy();
  });

  it("a link can be renamed and its address changed (FR-61)", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole("button", { name: "Add link" }));
    await user.type(await screen.findByLabelText("Paste links"), "example.org/docs");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await user.click(await screen.findByRole("button", { name: "Rename example.org" }));
    // Not a link: refused, nothing changes.
    await user.clear(screen.getByLabelText("Link address"));
    await user.type(screen.getByLabelText("Link address"), "not a link");
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("That isn't a link.")).toBeTruthy();
    await user.clear(screen.getByLabelText("Link address"));
    await user.type(screen.getByLabelText("Link address"), "example.org/handbook");
    await user.clear(screen.getByLabelText("Link name"));
    await user.type(screen.getByLabelText("Link name"), "Course handbook");
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByRole("button", { name: "Open Course handbook" })).toBeTruthy();
    // Esc cancels an edit.
    await user.click(screen.getByRole("button", { name: "Rename Course handbook" }));
    await user.keyboard("{Escape}");
    expect(await screen.findByRole("button", { name: "Open Course handbook" })).toBeTruthy();
  });

  it("Notes has search and New note, shows Today first, then Pinned, and the rest (the week lives on Tasks)", async () => {
    const user = userEvent.setup();
    render(<App />);
    await openNotes(user);
    expect(await screen.findByRole("textbox", { name: "Search notes" })).toBeTruthy();
    expect(screen.getByRole("button", { name: /New note/ })).toBeTruthy();
    const regions = (await screen.findAllByRole("region")).map((r) => r.getAttribute("aria-label"));
    expect(regions).not.toContain("Next 7 days");
    const order = ["Today", "Pinned", "All notes"].map((n) => regions.indexOf(n));
    expect(order.every((n) => n >= 0)).toBe(true);
    expect([...order].sort((a, b) => a - b)).toEqual(order);
    expect(within(screen.getByRole("region", { name: "All notes" })).getByRole("button", { name: "Groceries" })).toBeTruthy();
  });

  it("the Time page filters by period and category", async () => {
    const user = userEvent.setup();
    render(<App />);
    await goTo(user, /^Time/);
    const filters = await screen.findByRole("group", { name: "Filters" });
    expect(within(filters).getByRole("radio", { name: "30 days" })).toBeTruthy();
    expect(screen.queryByRole("tablist", { name: "Days" })).toBeNull();
    await user.click(within(filters).getByRole("radio", { name: "7 days" }));
    expect(await screen.findByRole("region", { name: "Apps" })).toBeTruthy();
    await user.click(within(filters).getByRole("radio", { name: "Sites" }));
    expect(await screen.findByRole("region", { name: "Sites" })).toBeTruthy();
  });

  it("unpinning moves a note out of Pinned", async () => {
    const user = userEvent.setup();
    render(<App />);
    await openNotes(user);
    // The card is not itself a button (it holds buttons); the button that opens it is inside it.
    const card = (await screen.findByRole("button", { name: "Shortcuts I want" })).parentElement as HTMLElement;
    await user.click(within(card).getByRole("button", { name: "Unpin" }));
    await waitFor(() => {
      const pinned = screen.getByRole("region", { name: "Pinned" });
      expect(within(pinned).queryByRole("button", { name: "Shortcuts I want" })).toBeNull();
    });
  });

  it("deleting a note offers Undo, and Undo brings it back", async () => {
    const user = userEvent.setup();
    render(<App />);
    await openNotes(user);
    const card = (await screen.findByRole("button", { name: "Groceries" })).parentElement as HTMLElement;
    await user.click(within(card).getByRole("button", { name: "Delete" }));
    // It asks first: Cancel keeps the note, Delete moves it to the Bin.
    await user.click(within(await screen.findByRole("alertdialog", { name: "Move this note to the Bin?" })).getByRole("button", { name: "Cancel" }));
    expect(await screen.findByRole("button", { name: "Groceries" })).toBeTruthy();
    await user.click(within(card).getByRole("button", { name: "Delete" }));
    await user.click(within(await screen.findByRole("alertdialog", { name: "Move this note to the Bin?" })).getByRole("button", { name: "Delete" }));
    const bubble = await petSays(/^Moved to Bin/);
    expect(bubble.actionLabel).toBe("Undo");
    await waitFor(() => expect(screen.queryByRole("button", { name: "Groceries" })).toBeNull());
    clickPetAction(bubble);
    expect(await screen.findByRole("button", { name: "Groceries" })).toBeTruthy();
  });

  it("searches notes by text", async () => {
    const user = userEvent.setup();
    render(<App />);
    await openNotes(user);
    await screen.findByRole("button", { name: "Groceries" });
    await user.type(screen.getByRole("textbox", { name: "Search notes" }), "peanut");
    expect(await screen.findByRole("heading", { name: /Results for/ })).toBeTruthy();
    await waitFor(() => expect(screen.queryByRole("button", { name: "Friday prep" })).toBeNull());
    expect(screen.getByRole("button", { name: "Groceries" })).toBeTruthy();
  });

  it("restores a note from the Bin page", async () => {
    const user = userEvent.setup();
    render(<App />);
    await goTo(user, /^Bin/);
    expect(await screen.findByText(/Old grocery list/)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: /Restore “Old grocery list”/ }));
    await openNotes(user);
    expect(await screen.findByRole("button", { name: "Old grocery list" })).toBeTruthy();
  });

  it("adds a reminder", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByText("Review the sync PR");
    await user.click(screen.getByRole("button", { name: "Add reminder" }));
    await user.type(await screen.findByLabelText("Remind me to"), "Water the plants");
    await user.click(screen.getByRole("button", { name: "Set reminder" }));
    expect(await screen.findByText("Water the plants")).toBeTruthy();
  });

  it("edits a missed reminder: snoozing it makes it upcoming again", async () => {
    const user = userEvent.setup();
    render(<App />);
    const edit = await screen.findByRole("button", { name: "Edit reminder “Review the sync PR”" });
    const row = edit.closest("li") as HTMLElement;
    expect(within(row).getByText(/Missed/)).toBeTruthy();
    await user.click(edit);
    await user.click(await screen.findByRole("button", { name: "In 10 min" }));
    await waitFor(() => expect(within(screen.getByRole("button", { name: "Edit reminder “Review the sync PR”" }).closest("li") as HTMLElement).queryByText(/Missed/)).toBeNull());
  });

  it("renames a reminder", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole("button", { name: "Edit reminder “Stand-up”" }));
    const title = await screen.findByLabelText("Remind me to");
    await user.clear(title);
    await user.type(title, "Daily stand-up");
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByRole("button", { name: "Edit reminder “Daily stand-up”" })).toBeTruthy();
  });
});

describe("first run (F1-26)", () => {
  it("never shows the welcome to someone who already has notes or tasks", async () => {
    render(<App />);
    expect(await screen.findByRole("region", { name: "Today’s agenda" })).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "Welcome to Pluto" })).toBeNull();
  });

  it("welcomes a fresh Pluto (after Delete all), then gets out of the way for good", async () => {
    const { ipc } = await import("@/ipc");
    const user = userEvent.setup();
    render(<App />);
    // Wait for data: by then the page is listening for events (effects ran), so it hears Delete all.
    await screen.findByText("Review the sync PR");
    await ipc.dataDeleteAll("DELETE");
    expect(await screen.findByRole("heading", { name: "Welcome to Pluto" }, { timeout: 3000 })).toBeTruthy();
    // The three choices, with the pet on and the rest off.
    expect(screen.getByRole("switch", { name: "Show Pluto the bear on your desktop" }).getAttribute("aria-checked")).toBe("true");
    expect(screen.getByRole("switch", { name: "Track app time" }).getAttribute("aria-checked")).toBe("false");
    await user.click(screen.getByRole("button", { name: "Start" }));
    // One tap on who you are; it only orders the tour and the quiz.
    expect(await screen.findByRole("heading", { name: "What describes you best?" })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: /^Student who codes/ }));
    expect(await screen.findByRole("region", { name: "Today’s agenda" })).toBeTruthy();
    expect(await ipc.prefsGet("ui.onboarded")).toBe(true);
    expect(await ipc.prefsGet("ui.role")).toBe("student_dev");
    // The spotlight tour starts by itself on Today, in this role's order, and Skip ends it for good.
    const tour = await screen.findByRole("dialog", { name: "Pluto tour" });
    expect(within(tour).getByText("Step 1 of 9")).toBeTruthy();
    await user.click(within(tour).getByRole("button", { name: "Skip tour" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Pluto tour" })).toBeNull());
    expect(await ipc.prefsGet("ui.tour")).toBe("done");
  });

  it("can start straight from writing a note", async () => {
    const { ipc } = await import("@/ipc");
    const user = userEvent.setup();
    render(<App />);
    // Wait for data: by then the page is listening for events (effects ran), so it hears Delete all.
    await screen.findByText("Review the sync PR");
    await ipc.dataDeleteAll("DELETE");
    await user.click(await screen.findByRole("button", { name: /Write a note/ }, { timeout: 3000 }));
    await user.click(await screen.findByRole("button", { name: "Skip" }));
    await waitFor(() => expect(screen.queryByRole("heading", { name: "Welcome to Pluto" })).toBeNull());
    // Not on Today, so the tour waits (Settings → Your profile) instead of jumping you away from the note.
    expect(screen.queryByRole("dialog", { name: "Pluto tour" })).toBeNull();
    expect((await ipc.notesList(false, null, "last_edited")).length).toBe(1);
  });
});

describe("meetings (F1-17…19)", () => {
  const openMeetings = (user: ReturnType<typeof userEvent.setup>) => goTo(user, /^Meetings/);

  it("lists meetings newest first and filters by participant", async () => {
    const user = userEvent.setup();
    render(<App />);
    await openMeetings(user);
    const list = await screen.findByRole("list", { name: "Meetings" });
    const rows = within(list).getAllByRole("button").map((b) => b.getAttribute("aria-label"));
    expect(rows).toEqual(["Open meeting “Sprint planning”", "Open meeting “Design review”"]);
    await user.type(screen.getByLabelText("Participant"), "ava");
    await waitFor(() => expect(within(screen.getByRole("list", { name: "Meetings" })).getAllByRole("button")).toHaveLength(1));
    expect(screen.getByRole("button", { name: "Open meeting “Design review”" })).toBeTruthy();
  });

  it("turns an action item into a task that shows its status", async () => {
    const { ipc } = await import("@/ipc");
    const user = userEvent.setup();
    render(<App />);
    await openMeetings(user);
    await user.click(await screen.findByRole("button", { name: "Open meeting “Sprint planning”" }));
    const item = await screen.findByRole("textbox", { name: "Action item 1" });
    const row = item.closest("li") as HTMLElement;
    await user.click(within(row).getByRole("button", { name: "Convert to task" }));
    expect(await within(row).findByText("Task · Planned")).toBeTruthy();
    const task = (await ipc.tasksQuery("all")).find((r) => r.task.title === "Write the meetings spec")?.task;
    expect(task?.project).toBe("Sprint planning");
    expect(task?.description).toContain("Assignee: Mara");
  });

  it("a task made from a meeting shows on Today with the meeting named (FR-52)", async () => {
    const user = userEvent.setup();
    render(<App />);
    await openMeetings(user);
    await user.click(await screen.findByRole("button", { name: "Open meeting “Sprint planning”" }));
    const row = (await screen.findByRole("textbox", { name: "Action item 1" })).closest("li") as HTMLElement;
    await user.click(within(row).getByRole("button", { name: "Convert to task" }));
    expect(await within(row).findByText("Task · Planned")).toBeTruthy();
    await user.keyboard("{Escape}");
    await goTo(user, /^Today/);
    const agenda = await screen.findByRole("region", { name: "Tasks" });
    expect(await within(agenda).findByRole("button", { name: "Edit Write the meetings spec" })).toBeTruthy();
    expect(within(agenda).getByTitle("From the meeting “Sprint planning”")).toBeTruthy();
  });

  it("logs a new meeting with Ctrl+M: it saves itself once it has a title", async () => {
    const { ipc } = await import("@/ipc");
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("region", { name: "Today’s agenda" });
    await user.keyboard("{Control>}m{/Control}");
    const title = await screen.findByRole("textbox", { name: "Meeting title" });
    await user.type(title, "1:1 with Asha");
    await user.type(screen.getByRole("combobox", { name: "Add participant" }), "Asha{Enter}");
    await user.click(screen.getByRole("button", { name: "Add decision" }));
    await user.type(screen.getByRole("textbox", { name: "Decision 1" }), "Weekly check-ins");
    await user.click(screen.getByRole("button", { name: "Add action item" }));
    await user.type(screen.getByRole("textbox", { name: "Action item 1" }), "Share the roadmap");
    await waitFor(async () => {
      const m = (await ipc.meetingsList()).find((x) => x.title === "1:1 with Asha");
      expect(m?.participants).toEqual(["Asha"]);
      expect(m?.decisions).toBe(1);
      expect(m?.action_items).toBe(1);
    }, { timeout: 3000 });
    await user.click(screen.getByRole("button", { name: "Back to meetings" }));
    expect(await screen.findByRole("button", { name: "Open meeting “1:1 with Asha”" })).toBeTruthy();
  });

  it("deletes a meeting after a second click", async () => {
    const user = userEvent.setup();
    render(<App />);
    await openMeetings(user);
    await user.click(await screen.findByRole("button", { name: "Open meeting “Design review”" }));
    await user.click(await screen.findByRole("button", { name: "Delete meeting" }));
    await user.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Delete" }));
    await screen.findByRole("list", { name: "Meetings" });
    expect(screen.queryByRole("button", { name: "Open meeting “Design review”" })).toBeNull();
  });
});

describe("yesterday on Today overview (R1-70)", () => {
  it("says how yesterday went and opens what is still unfinished", async () => {
    const user = userEvent.setup();
    render(<App />);
    const line = await screen.findByRole("button", { name: /Yesterday: \d+ of \d+ done · 2 still open/ });
    await user.click(line);
    expect(await screen.findByText("Unfinished from yesterday")).toBeTruthy();
    const list = screen.getByRole("region", { name: "Task list" });
    expect(within(list).getByText("Finish the sync spec")).toBeTruthy();
    expect(within(list).getByText("Refactor the tray menu")).toBeTruthy();
    expect(within(list).queryByText("Design the notes screen")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Show all views" }));
    expect(screen.queryByText("Unfinished from yesterday")).toBeNull();
  });
});

describe("settings (F1-25)", () => {
  it("changes a global shortcut by pressing the new keys, and refuses one already in use", async () => {
    const { ipc } = await import("@/ipc");
    const user = userEvent.setup();
    render(<App />);
    await goTo(user, /^Settings/);
    await user.click(await screen.findByRole("button", { name: "Change shortcut: Open or hide Pluto, from any app" }));
    expect(screen.getByText(/Press the new keys/)).toBeTruthy();
    await user.keyboard("{Alt>}{F9}{/Alt}");
    await waitFor(async () => expect((await ipc.settingsGetAll())["shortcuts.global_open"]).toBe("Alt+F9"));
    // Alt+F9 is now taken by "Open or hide": nothing changes.
    await user.click(screen.getByRole("button", { name: "Change shortcut: New note, from any app" }));
    await user.keyboard("{Alt>}{F9}{/Alt}");
    await petSays(/already used by another Pluto shortcut/);
    expect((await ipc.settingsGetAll())["shortcuts.global_new_note"]).toBeNull();
    // Back to the default.
    await user.click(screen.getByRole("button", { name: "Reset shortcut: Open or hide Pluto, from any app" }));
    await waitFor(async () => expect((await ipc.settingsGetAll())["shortcuts.global_open"]).toBeNull());
  });

  it("shows where data lives, has no log rows (FR-47), and resets settings after a confirmation", async () => {
    const { ipc } = await import("@/ipc");
    const user = userEvent.setup();
    render(<App />);
    await ipc.settingSet("pet.size", "L");
    await goTo(user, /^Settings/);
    expect(await screen.findByText(/pluto\.db$/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Open logs folder" })).toBeNull();
    expect(screen.queryByRole("group", { name: "Log detail" })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Reset settings…" }));
    await user.click(screen.getByRole("button", { name: "Reset" }));
    await waitFor(async () => expect((await ipc.settingsGetAll())["pet.size"]).toBe("M"));
  });
});

describe("task upgrades (FR-106..FR-109)", () => {
  const openTask = async (user: ReturnType<typeof userEvent.setup>, name: string) => {
    await goTo(user, /^Tasks/);
    await user.click(await screen.findByRole("radio", { name: "All" }));
    await user.click(await screen.findByRole("button", { name: `Open ${name}` }));
    return screen.findByRole("dialog");
  };

  it("the pet's menu opens a page straight away, and ignores one that does not exist (owner 2026-10-27)", async () => {
    render(<App />);
    await screen.findByRole("heading", { name: "Today" });
    act(() => { window.dispatchEvent(new CustomEvent("pluto-command", { detail: "page:time" })); });
    expect(await screen.findByRole("heading", { name: "Time" })).toBeTruthy();
    act(() => { window.dispatchEvent(new CustomEvent("pluto-command", { detail: "page:nonsense" })); });
    expect(screen.getByRole("heading", { name: "Time" })).toBeTruthy();
  });

  it("the task editor opens calm: title, next step, when, priority and Save; the rest is folded away (owner 2026-10-27)", async () => {
    const user = userEvent.setup();
    render(<App />);
    const dialog = await openTask(user, "Clear design feedback");
    const more = within(dialog).getByText("Advanced options").closest("details") as HTMLDetailsElement;
    expect(more.open).toBe(false);
    // Only these sit outside the fold (plus the dialog's own close button).
    for (const name of ["Title", "Priority"]) expect(within(dialog).getByLabelText(name)).toBeTruthy();
    expect(within(within(dialog).getByRole("group", { name: "Status" })).getAllByRole("button")).toHaveLength(1);
    for (const label of ["Deadline date", "Project name", "Linked note", "Description"]) expect(more.contains(within(dialog).getByLabelText(label))).toBe(true);
    expect(more.contains(within(dialog).getByRole("button", { name: "Delete task" }))).toBe(true);
    // Never wider or taller than the window.
    expect(dialog.className).toContain("w-[min(48rem,calc(100vw-2rem))]");
    expect(dialog.className).toContain("max-h-[calc(100vh-2rem)]");
  });

  it("links added to a task show next to it in the list and open the page (FR-106)", async () => {
    const { ipc } = await import("@/ipc");
    const open = vi.spyOn(ipc, "openUrl").mockResolvedValue(null);
    const user = userEvent.setup();
    render(<App />);
    const dialog = await openTask(user, "Refactor the tray menu");
    const paste = async (name: string, text: string) => { await user.click(within(dialog).getByRole("textbox", { name })); await user.paste(text); };
    await paste("New link", "github.com/pluto/tray");
    await paste("Link name", "Tray PR");
    await user.click(within(dialog).getByRole("button", { name: "Add link" }));
    await paste("New link", "https://docs.rs/tauri");
    await user.click(within(dialog).getByRole("button", { name: "Add link" }));
    expect(await within(dialog).findByRole("button", { name: "Remove link docs.rs/tauri" })).toBeTruthy();
    // A bad address is refused, and says so above the pet.
    await paste("New link", "javascript:alert(1)");
    await user.click(within(dialog).getByRole("button", { name: "Add link" }));
    await petSays(/web links/);
    await user.keyboard("{Escape}");

    const row = (await screen.findByRole("button", { name: "Open Refactor the tray menu" })).closest("li") as HTMLElement;
    await user.click(within(row).getByRole("button", { name: "Open link Tray PR" }));
    expect(open).toHaveBeenCalledWith("https://github.com/pluto/tray");
    expect(within(row).getByRole("button", { name: "Open link docs.rs/tauri" })).toBeTruthy();
  });

  it("a starred task moves to the top of Today, and a reminder can be starred too (FR-107)", async () => {
    const user = userEvent.setup();
    render(<App />);
    const agenda = await screen.findByRole("region", { name: "Today’s agenda" });
    const tasks = await within(agenda).findByRole("region", { name: "Tasks" });
    const firstTitle = () => within(tasks).getAllByRole("button", { name: /^Edit / })[0]?.getAttribute("aria-label");
    await waitFor(() => expect(firstTitle()).not.toBe("Edit Refactor the tray menu"));
    await user.click(within(tasks).getByRole("button", { name: "Star “Refactor the tray menu”" }));
    await waitFor(() => expect(firstTitle()).toBe("Edit Refactor the tray menu"));
    expect(within(tasks).getByRole("button", { name: "Unstar “Refactor the tray menu”" }).getAttribute("aria-pressed")).toBe("true");

    const reminders = within(agenda).getByRole("region", { name: "Reminders" });
    await user.click(within(reminders).getByRole("button", { name: "Star “Review the sync PR”" }));
    expect(await within(reminders).findByRole("button", { name: "Unstar “Review the sync PR”" })).toBeTruthy();
  });

  it("a task can be planned on chosen days (FR-108)", async () => {
    const user = userEvent.setup();
    render(<App />);
    const dialog = await openTask(user, "Prepare weekly planning");
    await user.click(within(dialog).getByRole("radio", { name: "Chosen days" }));
    const days = within(dialog).getByRole("group", { name: "Next 7 days" });
    await user.click(within(days).getByRole("button", { name: "Today" }));
    await user.click(within(days).getAllByRole("button")[2] as HTMLElement);
    expect(within(within(dialog).getByRole("list", { name: "Chosen days" })).getAllByRole("listitem")).toHaveLength(2);
    await user.click(within(dialog).getByRole("button", { name: "Save changes" }));
    await petSays("Task saved");
    // It's planned today now (its first day), with "2 days" next to it.
    const row = (await screen.findByRole("button", { name: "Open Prepare weekly planning" })).closest("li") as HTMLElement;
    expect(await within(row).findByText("2 days")).toBeTruthy();
    await goTo(user, /^Today/);
    const agenda = await screen.findByRole("region", { name: "Today’s agenda" });
    expect(await within(agenda).findByRole("button", { name: "Edit Prepare weekly planning" })).toBeTruthy();
  });

  it("a repeating task is ticked for today, comes off Today, and keeps a kind record (FR-109)", async () => {
    const user = userEvent.setup();
    render(<App />);
    let dialog = await openTask(user, "Clear design feedback");
    await user.click(within(dialog).getByRole("radio", { name: "Repeats" }));
    await user.click(within(dialog).getByRole("radio", { name: "Every day" }));
    // Start it today (it was planned in 4 days).
    const { ymd } = await import("@/lib/dates");
    const start = within(dialog).getByLabelText("Starts");
    await user.clear(start);
    await user.type(start, ymd(new Date()));
    await user.click(within(dialog).getByRole("button", { name: "Save changes" }));
    await petSays("Task saved");

    const row = (await screen.findByRole("button", { name: "Open Clear design feedback" })).closest("li") as HTMLElement;
    expect(within(row).getByText("Every day")).toBeTruthy();
    await goTo(user, /^Today/);
    const agenda = await screen.findByRole("region", { name: "Today’s agenda" });
    await user.click(await within(agenda).findByRole("checkbox", { name: "Tick Clear design feedback for today" }));
    await petSays(/^Ticked for today/);
    await waitFor(() => expect(within(agenda).queryByRole("button", { name: "Edit Clear design feedback" })).toBeNull());

    // Still open as a task; its record shows today ticked.
    dialog = await openTask(user, "Clear design feedback");
    const record = await within(dialog).findByRole("region", { name: "Repeat days" });
    expect(await within(record).findByText(/Done on/)).toBeTruthy();
    expect(within(record).getByRole("button", { name: /: ticked\./ }).getAttribute("aria-pressed")).toBe("true");
    expect(within(dialog).getAllByText("Planned").length).toBeGreaterThan(0); // in the header and the status track
  });
});

describe("focus sessions (FR-05)", () => {
  it("starts from its own tile on Today, shows the time left with pause, +5 and stop, and from a task with its name", async () => {
    const user = userEvent.setup();
    render(<App />);
    // Owner 2026-10-25: no focus timer in the Today list; Focus has its own tile.
    const agenda = await screen.findByRole("region", { name: "Today’s agenda" });
    expect(within(agenda).queryByRole("button", { name: /Focus/ })).toBeNull();
    const tile = await screen.findByRole("region", { name: "Focus" });
    await user.click(within(tile).getByRole("button", { name: "5 minutes shorter" }));
    expect(within(tile).getByText("20:00")).toBeTruthy();
    await user.click(within(tile).getByRole("button", { name: "25 min" }));
    await user.click(within(tile).getByRole("button", { name: "Start" }));
    const timer = await within(tile).findByRole("timer", { name: "Time left" }, { timeout: 3000 });
    expect(timer.textContent).toMatch(/^2[45]:\d\d$/);
    await user.click(within(tile).getByRole("button", { name: "5 min" }));
    await waitFor(() => expect(within(tile).getByRole("timer").textContent).toMatch(/^(29|30):\d\d$/));
    await user.click(within(tile).getByRole("button", { name: "Pause" }));
    expect(await within(tile).findByRole("button", { name: "Resume" })).toBeTruthy();
    await user.click(within(tile).getByRole("button", { name: "Stop" }));
    expect(await within(tile).findByRole("button", { name: "Start" })).toBeTruthy();

    // From a task: the session carries its name.
    await goTo(user, /^Tasks/);
    await user.click(await screen.findByRole("button", { name: "Open Refactor the tray menu" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: /Focus 25 min/ }));
    expect(await within(dialog).findByText(/^Focusing · \d+:\d\d left$/)).toBeTruthy();
    const { ipc } = await import("@/ipc");
    expect((await ipc.focusCurrent())?.label).toBe("Refactor the tray menu");
  });
});

describe("auto-delete (FR-46)", () => {
  it("shows what a stricter rule would do before turning it on, and binned tasks can be restored", async () => {
    const { ipc } = await import("@/ipc");
    vi.spyOn(ipc, "retentionPreview").mockResolvedValue({ time_sessions: 0, tasks_binned: 2, tasks_purged: 0 });
    const apply = vi.spyOn(ipc, "retentionApply").mockResolvedValue({ time_sessions: 0, tasks_binned: 2, tasks_purged: 0 });
    const user = userEvent.setup();
    render(<App />);
    await goTo(user, /^Settings/);
    await user.selectOptions(await screen.findByRole("combobox", { name: "Move completed tasks to the Bin after" }), "90");
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("2 completed tasks will move to the Bin.")).toBeTruthy();
    // Keeping everything changes nothing.
    await user.click(within(dialog).getByRole("button", { name: "Keep everything" }));
    expect(await ipc.prefsGet("ui.autoDelete")).toBeNull();
    await user.selectOptions(screen.getByRole("combobox", { name: "Move completed tasks to the Bin after" }), "90");
    await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Turn on" }));
    await petSays(/^Auto-delete is on/);
    expect(await ipc.prefsGet("ui.autoDelete")).toEqual({ time_days: 0, tasks_days: 90 });
    expect(apply).toHaveBeenCalledWith({ time_days: 0, tasks_days: 90 });

    // The Bin lists binned tasks with Restore.
    const done = (await ipc.tasksQuery("completed"))[0]?.task;
    vi.spyOn(ipc, "tasksBinned").mockResolvedValue(done ? [done] : []);
    const restore = vi.spyOn(ipc, "taskRestore").mockResolvedValue(done as never);
    await goTo(user, /^Bin/);
    const section = await screen.findByRole("region", { name: "Tasks in the Bin" });
    await user.click(within(section).getByRole("button", { name: /^Restore task/ }));
    expect(restore).toHaveBeenCalledWith(done?.id);
  });
});

describe("pet shortcut (FR-48)", () => {
  it("is off by default and can be set and turned off again", async () => {
    const { ipc } = await import("@/ipc");
    const user = userEvent.setup();
    render(<App />);
    await goTo(user, /^Settings/);
    expect((await ipc.settingsGetAll())["shortcuts.global_pet"]).toBeNull();
    await user.click(await screen.findByRole("button", { name: "Change shortcut: Show or hide the pet, from any app" }));
    await user.keyboard("{Control>}{Shift>}p{/Shift}{/Control}");
    await waitFor(async () => expect((await ipc.settingsGetAll())["shortcuts.global_pet"]).toBe("CmdOrCtrl+Shift+P"));
    await user.click(screen.getByRole("button", { name: "Reset shortcut: Show or hide the pet, from any app" }));
    await waitFor(async () => expect((await ipc.settingsGetAll())["shortcuts.global_pet"]).toBeNull());
  });
});

describe("date presets (FR-45)", () => {
  it("one click sets a task's deadline, and a reminder's time", async () => {
    const { dayPresets, timePresets } = await import("@/lib/datePresets");
    const user = userEvent.setup();
    render(<App />);
    await goTo(user, /^Tasks/);
    await user.click(await screen.findByRole("button", { name: "Open Refactor the tray menu" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(within(dialog).getByRole("group", { name: "Deadline presets" })).getByRole("button", { name: "Tomorrow" }));
    expect((within(dialog).getByLabelText("Deadline date") as HTMLInputElement).value).toBe(dayPresets().find((p) => p.label === "Tomorrow")?.value);
    await user.keyboard("{Escape}");

    await goTo(user, /^Today/);
    await user.click(await screen.findByRole("button", { name: /Add (a )?reminder/i }));
    const presets = await screen.findByRole("group", { name: "When presets" });
    await user.click(within(presets).getByRole("button", { name: "Tomorrow 9:00" }));
    expect((document.getElementById("rem-when") as HTMLInputElement).value).toBe(timePresets().find((p) => p.label === "Tomorrow 9:00")?.value);
  });
});

describe("open tabs are easy to find (FR-62)", () => {
  it("shows on the Time page without switching to Sites, and offers to turn tracking on", async () => {
    const { ipc } = await import("@/ipc");
    await ipc.settingSet("tracking.sites", false);
    const user = userEvent.setup();
    render(<App />);
    await goTo(user, /^Time/);
    await user.click(await screen.findByRole("button", { name: "Turn on app and website time" }));
    await waitFor(async () => {
      const s = await ipc.settingsGetAll();
      expect([s["tracking.apps"], s["tracking.sites"]]).toEqual([true, true]);
    });
  });
});

describe("owner's UI reset, batch 1 (2026-10-25)", () => {
  it("a task reminder: when it starts, how many minutes before (+ / −), and sound", async () => {
    const { ipc } = await import("@/ipc");
    const user = userEvent.setup();
    render(<App />);
    await goTo(user, /^Tasks/);
    await user.click(await screen.findByRole("button", { name: "Open Refactor the tray menu" }));
    const dialog = await screen.findByRole("dialog");
    const card = within(dialog).getByRole("region", { name: "Reminder" });
    const start = within(card).getByLabelText("It starts at") as HTMLInputElement;
    await user.clear(start);
    await user.type(start, "2031-03-04T10:00");
    expect(within(card).getByText("5 min before")).toBeTruthy();
    await user.click(within(card).getByRole("button", { name: "Remind earlier" }));
    expect(within(card).getByText("10 min before")).toBeTruthy();
    await user.click(within(card).getByRole("switch", { name: "Play a sound" }));
    await user.click(within(card).getByRole("button", { name: "Add reminder" }));
    await waitFor(async () => {
      const r = (await ipc.remindersList(false)).find((x) => x.task_id === "t3");
      expect(r && [r.lead_min, r.sound, r.remind_at]).toEqual([10, false, new Date(2031, 2, 4, 9, 50).getTime()]);
    });
    expect(await within(card).findByRole("list", { name: "Reminders for this task" })).toBeTruthy();
  });

  it("Tasks has the next 7 days as a large tile, and a task opens from it", async () => {
    const user = userEvent.setup();
    render(<App />);
    await goTo(user, /^Tasks/);
    const week = await screen.findByRole("region", { name: "Next 7 days" });
    expect(await screen.findByRole("region", { name: "Task list" })).toBeTruthy();
    await user.click(await within(week).findByRole("button", { name: /Send research notes to Mara/ }));
    expect(await screen.findByRole("dialog")).toBeTruthy();
  });

  it("Today tiles have two sizes, normal and full width, and Reset puts them back (owner 2026-10-27)", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole("button", { name: /^Resize Focus: normal, make it full width/ }));
    expect(await screen.findByRole("button", { name: /^Resize Focus: full width, make it normal/ })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Reset layout" }));
    expect(await screen.findByRole("button", { name: /^Resize Focus: normal/ })).toBeTruthy();
  });

  it("the task editor suggests a next step and shows where the task is", async () => {
    const user = userEvent.setup();
    render(<App />);
    await goTo(user, /^Tasks/);
    const list = await screen.findByRole("region", { name: "Task list" });
    const [first] = await within(list).findAllByRole("button", { name: /^Open / });
    await user.click(first as HTMLElement);
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByRole("list", { name: "Progress" })).toBeTruthy();
    expect(within(dialog).queryByText("Unsaved changes")).toBeNull();
    await user.type(within(dialog).getByRole("textbox", { name: "Title" }), "!");
    expect(await within(dialog).findByText("Unsaved changes")).toBeTruthy();
  });

  it("Projects shows a project's tasks and notes, numbered, and adds to it", async () => {
    const { ipc } = await import("@/ipc");
    const user = userEvent.setup();
    render(<App />);
    await goTo(user, /^Projects/);
    const project = await screen.findByRole("region", { name: "Project Pluto v1" });
    const tasks = within(project).getByRole("region", { name: "Tasks" });
    expect(within(tasks).getByText("1.")).toBeTruthy();
    expect(within(project).getByRole("button", { name: "Open note Pluto v1 scope" })).toBeTruthy();
    await user.type(within(tasks).getByRole("textbox", { name: "New project task" }), "Write the release post{Enter}");
    await waitFor(async () => {
      const all = await ipc.tasksQuery("all");
      expect(all.find((r) => r.task.title === "Write the release post")?.task.project).toBe("Pluto v1");
    });
    // A new project starts empty and fills from here.
    await user.type(screen.getByRole("textbox", { name: "New project name" }), "Thesis{Enter}");
    expect(await screen.findByRole("region", { name: "Project Thesis" })).toBeTruthy();
  });

  it("a project can be renamed, and its tasks and notes follow", async () => {
    const { ipc } = await import("@/ipc");
    const user = userEvent.setup();
    render(<App />);
    await goTo(user, /^Projects/);
    await screen.findByRole("region", { name: "Project Pluto v1" });
    await user.click(screen.getByRole("button", { name: "Rename Pluto v1" }));
    const box = screen.getByRole("textbox", { name: "Project name" });
    await user.clear(box);
    await user.type(box, "Pluto v2{Enter}");
    expect(await screen.findByRole("region", { name: "Project Pluto v2" })).toBeTruthy();
    await waitFor(async () => {
      const all = await ipc.tasksQuery("all");
      expect(all.some((r) => r.task.project === "Pluto v1")).toBe(false);
      expect(all.some((r) => r.task.project === "Pluto v2")).toBe(true);
    });
  });

  it("the note editor formats (bold, numbered list) and can move to its own window", async () => {
    const { ipc } = await import("@/ipc");
    const open = vi.spyOn(ipc, "noteWindowOpen").mockResolvedValue(null);
    const user = userEvent.setup();
    render(<App />);
    await openNotes(user);
    await user.click(await screen.findByRole("button", { name: "Groceries" }));
    const dialog = await screen.findByRole("dialog");
    const body = within(dialog).getByRole("textbox", { name: "Note" }) as HTMLTextAreaElement;
    body.setSelectionRange(0, 4); // "Oats"
    await user.click(within(dialog).getByRole("button", { name: /^Bold/ }));
    await waitFor(() => expect(body.value.startsWith("**Oats**")).toBe(true));
    await new Promise((r) => setTimeout(r, 50)); // the editor puts the selection back on the next frame
    body.setSelectionRange(0, body.value.length);
    await user.click(within(dialog).getByRole("button", { name: "Numbered list" }));
    await waitFor(() => expect(body.value.split("\n")[1]).toBe("2. Peanut butter"));
    await user.click(within(dialog).getByRole("button", { name: /Own window/ }));
    await waitFor(() => expect(open).toHaveBeenCalledWith("n4"));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("quotes can be added and edited by the user", async () => {
    const { ipc } = await import("@/ipc");
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole("button", { name: "Edit quotes" }));
    const dialog = await screen.findByRole("dialog");
    await user.type(within(dialog).getByLabelText("Add a quote"), "Small steps every day.");
    await user.type(within(dialog).getByRole("textbox", { name: "Who said it" }), "Me");
    await user.click(within(dialog).getByRole("button", { name: "Add" }));
    expect(await within(dialog).findByRole("region", { name: "Your quotes" })).toBeTruthy();
    await waitFor(async () => expect(await ipc.prefsGet("ui.quotes")).toMatchObject({ custom: [{ text: "Small steps every day.", author: "Me" }] }));
  });
});
