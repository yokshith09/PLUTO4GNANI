import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { meetingAi } from "@/ipc/meetingAi";
import { voiceIpc } from "@/ipc/voiceClient";
import type { AudioDeps } from "@/features/voice/VoiceProvider";
import { RecapPanel } from "./RecapPanel";

// There is no Tauri channel in the test browser.
vi.mock("@/audio", async (original) => ({ ...(await original<typeof import("@/audio")>()), playbackChannel: () => ({}) }));

afterEach(cleanup);

const fakeAudio = () => {
  const player = { enqueue: vi.fn(), stop: vi.fn() };
  const audio = { createPlayer: () => player } as unknown as AudioDeps;
  return { audio, player };
};
const wrap = (audio: AudioDeps) =>
  render(<QueryClientProvider client={new QueryClient()}><RecapPanel meetingId="as1" audio={audio} /></QueryClientProvider>);

describe("Spoken recap (MA-09)", () => {
  it("Read aloud only appears once a recap is written, and speaks that text in the recap's language", async () => {
    const user = userEvent.setup();
    const speak = vi.spyOn(voiceIpc, "speak").mockResolvedValue(null);
    const { audio } = fakeAudio();
    wrap(audio);
    expect(screen.queryByRole("button", { name: /Read aloud/ })).toBeNull();

    await user.selectOptions(screen.getByLabelText("Recap language"), "hi-IN");
    await user.click(screen.getByRole("button", { name: "Write recap" }));
    const written = (await screen.findByText(/\S+/, { selector: "p[aria-live]" })).textContent;
    await user.click(await screen.findByRole("button", { name: /Read aloud/ }));

    await waitFor(() => expect(speak).toHaveBeenCalledTimes(1));
    expect(speak.mock.calls[0]?.[0]).toBe(written);
    expect(speak.mock.calls[0]?.[1]).toBe("hi-IN");
    speak.mockRestore();
  });

  it("Save as mp3 asks for the recap file in the chosen language", async () => {
    const user = userEvent.setup();
    const recap = vi.spyOn(meetingAi, "recap").mockResolvedValue("C:\\recap.mp3");
    wrap(fakeAudio().audio);
    await user.selectOptions(screen.getByLabelText("Recap language"), "ta-IN");
    await user.click(screen.getByRole("button", { name: "Write recap" }));
    await user.click(await screen.findByRole("button", { name: /Save as mp3/ }));
    await waitFor(() => expect(recap).toHaveBeenCalledWith("as1", "ta-IN", false, true));
    recap.mockRestore();
  });

  it("a speaking failure shows its message and leaves the recap on screen", async () => {
    const user = userEvent.setup();
    const speak = vi.spyOn(voiceIpc, "speak").mockRejectedValue({ code: "cloud_voice_disabled", message: "Cloud voice is off.", retryable: false });
    wrap(fakeAudio().audio);
    await user.click(screen.getByRole("button", { name: "Write recap" }));
    await user.click(await screen.findByRole("button", { name: /Read aloud/ }));
    expect((await screen.findByRole("alert")).textContent).toMatch(/Cloud voice is off/);
    expect(screen.getByRole("button", { name: /Read aloud/ })).toBeTruthy();
    speak.mockRestore();
  });
});
