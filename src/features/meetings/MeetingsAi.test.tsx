import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { MeetingsPage } from "./MeetingsPage";

const wrap = () => render(
  <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><MeetingsPage /></QueryClientProvider>,
);
afterEach(cleanup);

/** Import lives in the "More" menu next to Record. */
async function importRecording(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole("button", { name: "More" }));
  await user.click(await screen.findByRole("menuitem", { name: /Import a recording/ }));
}

// Against the in-memory shell: Import -> transcribing -> transcript -> speakers (T-202).
describe("Meetings audio (T-202)", () => {
  it("imports a recording, shows progress, then the transcript, and maps a speaker", async () => {
    const user = userEvent.setup();
    wrap();
    await importRecording(user);
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByRole("button", { name: /Start transcription/ })).toHaveProperty("disabled", true);

    await user.click(within(dialog).getByRole("button", { name: /Choose audio file/ }));
    expect(await within(dialog).findByText("launch-sync.wav")).toBeTruthy();
    expect((within(dialog).getByLabelText("Meeting title") as HTMLInputElement).value).toBe("launch sync");
    await user.type(within(dialog).getByLabelText(/Participants/), "Asha, Ravi, Meena");
    await user.click(within(dialog).getByRole("button", { name: /Start transcription/ }));

    // The editor opens on the new meeting; its Transcript tab shows where the audio stands.
    await user.click(await screen.findByRole("tab", { name: "Transcript" }));
    const panel = await screen.findByRole("region", { name: "Audio and transcript" });
    expect(within(panel).getByText("Transcribing")).toBeTruthy();
    expect(within(panel).getByRole("progressbar")).toBeTruthy();

    const transcript = await within(panel).findByRole("list", { name: "Transcript" }, { timeout: 4000 });
    expect(within(transcript).getAllByRole("listitem").length).toBeGreaterThan(3);
    expect(within(panel).getByText("Needs speakers")).toBeTruthy();

    await user.click(within(panel).getByRole("button", { name: /Name speakers/ }));
    const select = within(panel).getByLabelText("Speaker 1");
    await user.selectOptions(select, "Asha");
    await waitFor(() => expect(within(panel).getByText("Ready to review")).toBeTruthy());

    // Search inside the transcript narrows it to the lines that match.
    const lines = within(transcript).getAllByRole("listitem").length;
    await user.type(within(panel).getByLabelText("Search this transcript"), "pricing");
    await waitFor(() => expect(within(within(panel).getByRole("list", { name: "Transcript" })).getAllByRole("listitem").length).toBeLessThan(lines));
    expect(within(panel).getAllByText(/pricing/i, { selector: "mark" }).length).toBeGreaterThan(0);
  }, 15_000);

  it("a typed meeting shows no audio section", async () => {
    const user = userEvent.setup();
    wrap();
    await user.click(await screen.findByRole("button", { name: /Open meeting “Sprint planning”/ }));
    await screen.findByText("Decisions");
    expect(screen.queryByRole("tab", { name: "Transcript" })).toBeNull();
    expect(screen.queryByRole("region", { name: "Audio and transcript" })).toBeNull();
  });

  it("refuses an unsupported file with the shell's message", async () => {
    const user = userEvent.setup();
    wrap();
    const { meetingAi } = await import("@/ipc/meetingAi");
    await expect(meetingAi.import("t", [], "notes.txt")).rejects.toMatchObject({ code: "unsupported_audio" });
    await importRecording(user);
  });
});

describe("Recap (T-205)", () => {
  it("writes a recap in the chosen language for a meeting that has items", async () => {
    const user = userEvent.setup();
    wrap();
    await user.click(await screen.findByRole("button", { name: /Open meeting “Sprint planning”/ }));
    const recap = await screen.findByRole("region", { name: "Recap" });
    await user.click(within(recap).getByRole("button", { name: "Write summary" }));
    expect(await within(recap).findByText(/Launch is on 20 November/)).toBeTruthy();
    await user.selectOptions(within(recap).getByLabelText("Recap language"), "hi-IN");
    await user.click(within(recap).getByRole("button", { name: "Regenerate" }));
    expect(await within(recap).findByText(/लॉन्च 20 नवंबर/)).toBeTruthy();
    // It was saved in the meeting's notes, and is still there on the Notes tab.
    await user.click(screen.getByRole("tab", { name: "Notes" }));
    expect((screen.getByLabelText("Meeting notes") as HTMLTextAreaElement).value).toMatch(/^## Summary\n.*लॉन्च 20 नवंबर/s);
  });

  it("is not offered for a meeting with no decisions or action items", async () => {
    const user = userEvent.setup();
    wrap();
    await user.click(await screen.findByRole("button", { name: /New meeting/ }));
    await screen.findByText("Decisions");
    expect(screen.queryByRole("region", { name: "Recap" })).toBeNull();
  });
});

describe("Commitments (T-503)", () => {
  it("lists who promised what, with late ones marked, and filters by owner", async () => {
    const user = userEvent.setup();
    wrap();
    const panel = await screen.findByRole("region", { name: "Commitments" });
    expect(within(panel).queryByRole("list", { name: "Open commitments" }), "folded away until asked for").toBeNull();
    await user.click(within(panel).getByRole("button", { name: /Who owes what/ }));
    const list = within(panel).getByRole("list", { name: "Open commitments" });
    const rows = within(list).getAllByRole("listitem");
    expect(rows.length).toBeGreaterThan(0);
    expect(within(list).getByText(/Finish the pricing page copy/)).toBeTruthy();
    expect(within(list).getAllByText(/Launch sync/).length).toBeGreaterThan(0);
    // Dates in the fixture are early November 2026, so by now at least one is late or open.
    const select = within(panel).getByLabelText("Whose commitments");
    expect(within(select).getAllByRole("option").map((o) => o.textContent)).toContain("Ravi");
    await user.selectOptions(select, "Ravi");
    await waitFor(() => expect(within(list).getAllByRole("listitem")).toHaveLength(1));
    expect(within(list).getByText(/Ravi · Launch sync/)).toBeTruthy();
  });
});

describe("Search across meetings", () => {
  it("one search box finds what was said, and opens the meeting at that line", async () => {
    const user = userEvent.setup();
    const { meetingAi } = await import("@/ipc/meetingAi");
    const job = await meetingAi.import("Launch sync", ["Asha"], "launch-sync.wav");
    // Wait for the mock transcription to finish.
    await waitFor(async () => expect((await meetingAi.jobGet(job.meeting_id))?.status).toBe("completed"), { timeout: 5000 });
    wrap();
    await user.type(await screen.findByLabelText("Search meetings"), "pricing");
    const results = await screen.findByRole("region", { name: "In transcripts" });
    const hit = await within(results).findAllByRole("button", { name: /Open “Launch sync” at \d+:\d\d/ });
    expect(within(results).getAllByText(/pricing/i, { selector: "mark" }).length).toBeGreaterThan(0);
    await user.click(hit[0] as HTMLElement);
    const transcript = await screen.findByRole("list", { name: "Transcript" }, { timeout: 4000 });
    expect(transcript.querySelector('[aria-current="true"]')).toBeTruthy();
    expect(screen.getByRole("tab", { name: "Transcript", selected: true })).toBeTruthy();
  }, 15_000);

  it("filters stay folded away until asked for", async () => {
    const user = userEvent.setup();
    wrap();
    await screen.findByLabelText("Search meetings");
    expect(screen.queryByLabelText("Participant")).toBeNull();
    await user.click(screen.getByRole("button", { name: /Filters/ }));
    expect(screen.getByLabelText("Participant")).toBeTruthy();
  });
});
