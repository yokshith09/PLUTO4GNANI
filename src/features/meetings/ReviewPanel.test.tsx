import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import type { ExtractedItem } from "@/ipc/generated";
import { MeetingsPage } from "./MeetingsPage";
import { toApply, warningText } from "./ReviewPanel";

afterEach(cleanup);

const item = (id: string, kind: ExtractedItem["kind"], over: Partial<ExtractedItem> = {}): ExtractedItem => ({
  id, kind, body: `body ${id}`, owner: null, due_phrase: null, due_date: null, priority: null, source_segment_id: "s", quote: "q", duplicate_of_task_id: null, ...over,
});
const row = (over = {}) => ({ accepted: true, body: "b", owner: "", due: "", createTask: true, ...over });

describe("toApply", () => {
  it("sends only accepted items; owner, date and task apply to action items only", () => {
    const items = [item("a", "action_item"), item("d", "decision"), item("r", "risk")];
    const out = toApply("m1", items, {
      a: row({ body: " Do it ", owner: "Ravi", due: "2026-11-06" }),
      d: row({ owner: "Ravi", due: "2026-11-06", createTask: true }),
      r: row({ accepted: false }),
    });
    expect(out.accepted).toEqual([
      { item_id: "a", body: "Do it", owner: "Ravi", due_date: "2026-11-06", priority: null, create_task: true },
      { item_id: "d", body: "b", owner: null, due_date: null, priority: null, create_task: false },
    ]);
  });
  it("drops an item whose text was cleared", () => {
    expect(toApply("m1", [item("a", "decision")], { a: row({ body: "   " }) }).accepted).toEqual([]);
  });
});

describe("warningText", () => {
  it("names the item and skips warnings about unknown items", () => {
    const items = [item("a", "action_item", { body: "Send it" })];
    expect(warningText({ kind: "no_decisions" }, items)).toMatch(/No decisions/);
    expect(warningText({ kind: "action_item_without_owner", item_id: "a" }, items)).toBe("“Send it” has no owner.");
    expect(warningText({ kind: "unresolved_due_date", item_id: "zzz" }, items)).toBeNull();
  });
});

describe("Review and apply (T-204), against the in-memory shell", () => {
  it("finds items, lets the user drop one, applies, and the meeting and tasks update", async () => {
    const user = userEvent.setup();
    render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><MeetingsPage /></QueryClientProvider>);

    await user.click(await screen.findByRole("button", { name: /Import recording/ }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: /Choose audio file/ }));
    await within(dialog).findByText("launch-sync.wav");
    await user.type(within(dialog).getByLabelText(/Participants/), "Asha, Ravi, Meena");
    await user.click(within(dialog).getByRole("button", { name: /Start transcription/ }));

    const panel = await screen.findByRole("region", { name: "Audio and transcript" });
    await within(panel).findByRole("tab", { name: "Transcript" }, { timeout: 4000 });
    await user.click(within(panel).getByRole("tab", { name: "Review" }));
    await user.click(within(panel).getByRole("button", { name: /Find decisions and action items/ }));

    const review = await within(panel).findByRole("region", { name: "Review" });
    const actions = within(review).getByRole("list", { name: "Action items" });
    expect(within(actions).getAllByRole("listitem")).toHaveLength(2);
    expect(within(review).getAllByText(/Show in transcript/).length).toBe(4);
    expect(within(review).getByRole("list", { name: "Warnings" })).toBeTruthy();

    // Drop the risk, keep the rest.
    await user.click(within(review).getByRole("checkbox", { name: /Keep: Launch may be missed/ }));
    const apply = within(review).getByRole("button", { name: /Apply 3 items/ });
    await user.click(apply);

    await waitFor(() => expect(within(panel).queryByRole("region", { name: "Review" })).toBeNull(), { timeout: 4000 });
    // The decisions and action items are now on the meeting, with their tasks.
    await screen.findByDisplayValue("Start with the annual plan.");
    expect(screen.getByDisplayValue("Finish the pricing page copy.")).toBeTruthy();
    expect(screen.getAllByText(/Task · /).length).toBe(2);
  }, 20_000);

  it("shows the quote and jumps to the transcript line", async () => {
    const user = userEvent.setup();
    render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><MeetingsPage /></QueryClientProvider>);
    await user.click(await screen.findByRole("button", { name: /Import recording/ }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: /Choose audio file/ }));
    await within(dialog).findByText("launch-sync.wav");
    await user.click(within(dialog).getByRole("button", { name: /Start transcription/ }));
    const panel = await screen.findByRole("region", { name: "Audio and transcript" });
    await within(panel).findByRole("tab", { name: "Review" }, { timeout: 4000 });
    await user.click(within(panel).getByRole("tab", { name: "Review" }));
    await user.click(within(panel).getByRole("button", { name: /Find decisions/ }));
    const review = await within(panel).findByRole("region", { name: "Review" });
    expect(within(review).getByText(/Main Thursday tak copy complete kar dunga/)).toBeTruthy();
    await user.click(within(review).getAllByText("Show in transcript")[0] as HTMLElement);
    const line = await within(panel).findByRole("list", { name: "Transcript" });
    expect(line.querySelector('[aria-current="true"]')).toBeTruthy();
  }, 20_000);
});
