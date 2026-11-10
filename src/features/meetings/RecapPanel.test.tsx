import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { useState } from "react";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { meetingAi } from "@/ipc/meetingAi";
import { voiceIpc } from "@/ipc/voiceClient";
import type { AudioDeps } from "@/features/voice/VoiceProvider";
import { RecapPanel } from "./RecapPanel";
import { extractSummary } from "./summaryNote";

// There is no Tauri channel in the test browser.
vi.mock("@/audio", async (original) => ({ ...(await original<typeof import("@/audio")>()), playbackChannel: () => ({}) }));

afterEach(cleanup);

const fakeAudio = () => {
  const player = { enqueue: vi.fn(), stop: vi.fn() };
  const audio = { createPlayer: () => player } as unknown as AudioDeps;
  return { audio, player };
};
/** The editor owns the notes; this stands in for it and records what it was asked to save. */
function Harness({ audio, initial = "", onSaved, onOpen }: { audio: AudioDeps; initial?: string; onSaved?: (notes: string) => void; onOpen?: () => void }) {
  const [notes, setNotes] = useState(initial);
  return (
    <RecapPanel
      meetingId="as1" notes={notes} audio={audio}
      onNotes={(n) => { setNotes(n); onSaved?.(n); }} {...(onOpen ? { onOpenNotes: onOpen } : {})}
    />
  );
}
const wrap = (audio: AudioDeps, props: { initial?: string; onSaved?: (n: string) => void; onOpen?: () => void } = {}) =>
  render(<QueryClientProvider client={new QueryClient()}><Harness audio={audio} {...props} /></QueryClientProvider>);

describe("The meeting summary (MA-09)", () => {
  it("Read aloud only appears once a summary is written, and speaks that text in its language", async () => {
    const user = userEvent.setup();
    const speak = vi.spyOn(voiceIpc, "speak").mockResolvedValue(null);
    wrap(fakeAudio().audio);
    expect(screen.queryByRole("button", { name: /Read aloud/ })).toBeNull();
    await user.selectOptions(screen.getByLabelText("Recap language"), "hi-IN");
    await user.click(screen.getByRole("button", { name: "Write summary" }));
    const written = (await screen.findByText(/\S+/, { selector: "p[aria-live]" })).textContent;
    await user.click(await screen.findByRole("button", { name: /Read aloud/ }));
    await waitFor(() => expect(speak).toHaveBeenCalledTimes(1));
    expect(speak.mock.calls[0]?.[0]).toBe(written);
    expect(speak.mock.calls[0]?.[1]).toBe("hi-IN");
    speak.mockRestore();
  });

  it("is saved in the meeting's notes the moment it is written, above the person's own notes", async () => {
    const user = userEvent.setup();
    const saved: string[] = [];
    wrap(fakeAudio().audio, { initial: "My own notes", onSaved: (n) => saved.push(n) });
    await user.click(screen.getByRole("button", { name: "Write summary" }));
    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0]).toMatch(/^## Summary\n.+\n\nMy own notes$/s);
    expect(extractSummary(saved[0] ?? "")).toBeTruthy();
  });

  it("Regenerate replaces the summary and saves again, without adding a second one", async () => {
    const user = userEvent.setup();
    const saved: string[] = [];
    const texts = ["First version.", "Second version."];
    const recap = vi.spyOn(meetingAi, "recapText").mockImplementation(async () => texts.shift() ?? "");
    wrap(fakeAudio().audio, { onSaved: (n) => saved.push(n) });
    await user.click(screen.getByRole("button", { name: "Write summary" }));
    await screen.findByText("First version.");
    await user.click(await screen.findByRole("button", { name: "Regenerate" }));
    await screen.findByText("Second version.");
    expect(saved).toEqual(["## Summary\nFirst version.", "## Summary\nSecond version."]);
    expect(screen.queryByText("First version.")).toBeNull();
    recap.mockRestore();
  });

  it("shows a summary that is already in the notes, and Open in notes goes to them", async () => {
    const user = userEvent.setup();
    const open = vi.fn();
    wrap(fakeAudio().audio, { initial: "## Summary\nWe ship Friday.\n\nMore notes", onOpen: open });
    expect(await screen.findByText("We ship Friday.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Regenerate" })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: /Open in notes/ }));
    expect(open).toHaveBeenCalledTimes(1);
  });

  it("can't be written until there is something to summarise", () => {
    render(<QueryClientProvider client={new QueryClient()}><RecapPanel meetingId="as1" hasItems={false} audio={fakeAudio().audio} /></QueryClientProvider>);
    expect((screen.getByRole("button", { name: "Write summary" }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByText(/Add decisions or action items first/)).toBeTruthy();
  });

  it("Save as mp3 asks for the audio in the chosen language", async () => {
    const user = userEvent.setup();
    const recap = vi.spyOn(meetingAi, "recap").mockResolvedValue("C:\\recap.mp3");
    wrap(fakeAudio().audio);
    await user.selectOptions(screen.getByLabelText("Recap language"), "ta-IN");
    await user.click(screen.getByRole("button", { name: "Write summary" }));
    await user.click(await screen.findByRole("button", { name: /Save as mp3/ }));
    await waitFor(() => expect(recap).toHaveBeenCalledWith("as1", "ta-IN", false, true));
    recap.mockRestore();
  });

  it("a speaking failure shows its message and leaves the summary on screen", async () => {
    const user = userEvent.setup();
    vi.spyOn(voiceIpc, "speak").mockRejectedValue({ code: "cloud_voice_disabled", message: "Cloud voice is off.", retryable: false });
    wrap(fakeAudio().audio);
    await user.click(screen.getByRole("button", { name: "Write summary" }));
    const text = (await screen.findByText(/\S+/, { selector: "p[aria-live]" })).textContent;
    await user.click(await screen.findByRole("button", { name: /Read aloud/ }));
    expect((await screen.findByRole("alert")).textContent).toMatch(/Cloud voice is off/);
    expect(screen.getByText(text ?? "")).toBeTruthy();
  });
});
