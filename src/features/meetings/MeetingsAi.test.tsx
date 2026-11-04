import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { MeetingsPage } from "./MeetingsPage";

const wrap = () => render(
  <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><MeetingsPage /></QueryClientProvider>,
);
afterEach(cleanup);

// Against the in-memory shell: Import -> transcribing -> transcript -> speakers (T-202).
describe("Meetings audio (T-202)", () => {
  it("imports a recording, shows progress, then the transcript, and maps a speaker", async () => {
    const user = userEvent.setup();
    wrap();
    await user.click(await screen.findByRole("button", { name: /Import recording/ }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByRole("button", { name: /Start transcription/ })).toHaveProperty("disabled", true);

    await user.click(within(dialog).getByRole("button", { name: /Choose audio file/ }));
    expect(await within(dialog).findByText("launch-sync.wav")).toBeTruthy();
    expect((within(dialog).getByLabelText("Meeting title") as HTMLInputElement).value).toBe("launch sync");
    await user.type(within(dialog).getByLabelText(/Participants/), "Asha, Ravi, Meena");
    await user.click(within(dialog).getByRole("button", { name: /Start transcription/ }));

    // The editor opens on the new meeting with the audio section.
    const panel = await screen.findByRole("region", { name: "Audio and transcript" });
    expect(within(panel).getByText("Transcribing")).toBeTruthy();
    expect(within(panel).getByRole("progressbar")).toBeTruthy();

    await within(panel).findByRole("tab", { name: "Transcript" }, { timeout: 4000 });
    const transcript = await within(panel).findByRole("list", { name: "Transcript" });
    expect(within(transcript).getAllByRole("listitem").length).toBeGreaterThan(3);
    expect(within(panel).getByText("Needs speakers")).toBeTruthy();

    await user.click(within(panel).getByRole("tab", { name: /Speakers/ }));
    const select = within(panel).getByLabelText("Speaker 1");
    await user.selectOptions(select, "Asha");
    await waitFor(() => expect(within(panel).getByText("Ready to review")).toBeTruthy());
  }, 15_000);

  it("a typed meeting shows no audio section", async () => {
    const user = userEvent.setup();
    wrap();
    await user.click(await screen.findByRole("button", { name: /Open meeting “Sprint planning”/ }));
    await screen.findByText("Decisions");
    expect(screen.queryByRole("region", { name: "Audio and transcript" })).toBeNull();
  });

  it("refuses an unsupported file with the shell's message", async () => {
    const user = userEvent.setup();
    wrap();
    const { meetingAi } = await import("@/ipc/meetingAi");
    await expect(meetingAi.import("t", [], "notes.txt")).rejects.toMatchObject({ code: "unsupported_audio" });
    await user.click(await screen.findByRole("button", { name: /Import recording/ }));
  });
});

describe("Recap (T-205)", () => {
  it("writes a recap in the chosen language for a meeting that has items", async () => {
    const user = userEvent.setup();
    wrap();
    await user.click(await screen.findByRole("button", { name: /Open meeting “Sprint planning”/ }));
    const recap = await screen.findByRole("region", { name: "Recap" });
    await user.click(within(recap).getByRole("button", { name: "Write recap" }));
    expect(await within(recap).findByText(/Launch is on 20 November/)).toBeTruthy();
    await user.selectOptions(within(recap).getByLabelText("Recap language"), "hi-IN");
    await user.click(within(recap).getByRole("button", { name: "Write again" }));
    expect(await within(recap).findByText(/लॉन्च 20 नवंबर/)).toBeTruthy();
  });

  it("is not offered for a meeting with no decisions or action items", async () => {
    const user = userEvent.setup();
    wrap();
    await user.click(await screen.findByRole("button", { name: /New meeting/ }));
    await screen.findByText("Decisions");
    expect(screen.queryByRole("region", { name: "Recap" })).toBeNull();
  });
});
