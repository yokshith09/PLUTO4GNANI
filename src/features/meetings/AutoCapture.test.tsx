import { act, cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AudioDeps } from "@/features/voice/VoiceProvider";
import { ipc } from "@/ipc";
import { AutoCapture } from "./AutoCapture";

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

const send = (on: boolean, session = "s1") => act(() => { window.dispatchEvent(new CustomEvent("pluto-auto-capture", { detail: { session_id: session, on } })); });

function fakeAudio(failWith?: Error) {
  const handle = { stop: vi.fn().mockResolvedValue(undefined) };
  const close = vi.fn().mockResolvedValue(undefined);
  const made: string[] = [];
  const audio = {
    startCapture: vi.fn(async () => { if (failWith) throw failWith; return handle; }),
    FrameSender: class { constructor(session: string) { made.push(session); } send() {} close = close; },
    pushFrame: vi.fn(),
  } as unknown as AudioDeps;
  return { audio, handle, close, made };
}

describe("Google Meet auto mode, page side", () => {
  it("opens the microphone for the session the shell names, and closes it when told to", async () => {
    const f = fakeAudio();
    render(<AutoCapture audio={f.audio} />);
    send(true);
    await waitFor(() => expect(f.audio.startCapture).toHaveBeenCalledTimes(1));
    expect(f.made).toEqual(["s1"]);
    send(true); // the same session again changes nothing
    expect(f.audio.startCapture).toHaveBeenCalledTimes(1);
    send(false, "other"); // another session's end is not ours
    expect(f.handle.stop).not.toHaveBeenCalled();
    send(false);
    await waitFor(() => expect(f.handle.stop).toHaveBeenCalledTimes(1));
    expect(f.close).toHaveBeenCalledTimes(1);
  });

  it("closes the microphone when the page goes away", async () => {
    const f = fakeAudio();
    const { unmount } = render(<AutoCapture audio={f.audio} />);
    send(true);
    await waitFor(() => expect(f.audio.startCapture).toHaveBeenCalled());
    unmount();
    await waitFor(() => expect(f.handle.stop).toHaveBeenCalledTimes(1));
  });

  it("tells the shell when the microphone cannot be opened, so the recording ends", async () => {
    const failed = vi.spyOn(ipc, "autoMeetingCaptureFailed").mockResolvedValue(null);
    const f = fakeAudio(new Error("Permission denied"));
    render(<AutoCapture audio={f.audio} />);
    send(true);
    await waitFor(() => expect(failed).toHaveBeenCalledWith("Permission denied"));
  });
});
