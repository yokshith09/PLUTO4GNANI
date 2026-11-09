import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CaptureError } from "@/audio";
import type { AudioDeps } from "@/features/voice/VoiceProvider";
import { clock } from "./RecordDialog";

afterEach(cleanup);

interface Fake extends AudioDeps { captures: { stopped: boolean; onEnded?: (() => void) | undefined }[]; sessions: string[]; sent: number }
function fakeAudio(over: Partial<AudioDeps> = {}): Fake {
  const f: Fake = { captures: [], sessions: [], sent: 0 } as unknown as Fake;
  class FakeSender {
    constructor(readonly sessionId: string) { f.sessions.push(sessionId); }
    send() { f.sent += 1; }
    async close() { /* nothing to flush */ }
  }
  Object.assign(f, {
    startCapture: (async (o: { onFrame: (b: ArrayBuffer) => void; onEnded?: () => void }) => {
      const c = { stopped: false, onEnded: o.onEnded };
      f.captures.push(c);
      o.onFrame(new ArrayBuffer(1024));
      return { deviceSampleRate: 48000, stop: async () => { c.stopped = true; } };
    }) as never,
    FrameSender: FakeSender as never,
    pushFrame: (async () => undefined) as never,
    createPlayer: () => ({ enqueue: () => undefined, stop: () => undefined }),
    ...over,
  });
  return f;
}

let meetingAi: typeof import("@/ipc/meetingAi").meetingAi;
let voiceIpc: typeof import("@/ipc/voiceClient").voiceIpc;
beforeEach(async () => {
  vi.resetModules(); // a fresh in-memory shell for each test
  ({ meetingAi } = await import("@/ipc/meetingAi"));
  ({ voiceIpc } = await import("@/ipc/voiceClient"));
  await voiceIpc.settingsSet({ cloud_voice_enabled: true });
  await voiceIpc.keySet("gnani", "k");
});

async function mount(audio: AudioDeps) {
  const { RecordDialog } = await import("./RecordDialog");
  const onRecorded = vi.fn();
  const onClose = vi.fn();
  render(
    <QueryClientProvider client={new QueryClient()}>
      <RecordDialog open onClose={onClose} onRecorded={onRecorded} audio={audio} />
    </QueryClientProvider>,
  );
  return { onRecorded, onClose };
}
const dialog = () => within(screen.getByRole("dialog"));
/** Wait for the status line (not the title, which also says "Recording") to show `text`. */
const statusIs = (text: string) => waitFor(() => expect(dialog().getByRole("status").textContent).toBe(text));

describe("clock", () => {
  it("formats minutes and seconds", () => {
    expect(clock(0)).toBe("0:00");
    expect(clock(65_400)).toBe("1:05");
    expect(clock(600_000)).toBe("10:00");
  });
});

describe("Record a meeting (T-207), against the in-memory shell", () => {
  it("asks for the computer's sound only when ticked, and passes that on", async () => {
    const user = userEvent.setup();
    const { meetingAi: ai } = await import("@/ipc/meetingAi");
    const start = vi.spyOn(ai, "recStart");
    await mount(fakeAudio());
    const box = dialog().getByLabelText(/Include the computer's sound/) as HTMLInputElement;
    expect(box.checked).toBe(false);
    await user.click(dialog().getByLabelText(/Everyone in this meeting/));
    await user.click(box);
    await user.click(dialog().getByRole("button", { name: /Start recording/ }));
    await statusIs("Recording");
    expect(start).toHaveBeenCalledWith("", [], false, true, true);
  });

  it("cannot start without consent, here or in the shell", async () => {
    const user = userEvent.setup();
    await mount(fakeAudio());
    const start = dialog().getByRole("button", { name: /Start recording/ });
    expect((start as HTMLButtonElement).disabled).toBe(true);
    await expect(meetingAi.recStart("t", [], false, false)).rejects.toMatchObject({ code: "consent_required" });
    await user.click(dialog().getByRole("checkbox", { name: /knows it is being recorded/ }));
    expect((dialog().getByRole("button", { name: /Start recording/ }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("the pet's Stop button ends a recording started here", async () => {
    const user = userEvent.setup();
    const { emitNamed } = await import("@/ipc/namedEvents");
    const audio = fakeAudio();
    const { onRecorded } = await mount(audio);
    await user.type(dialog().getByLabelText("Meeting title"), "Launch sync");
    await user.click(dialog().getByLabelText(/Everyone in this meeting/));
    await user.click(dialog().getByRole("button", { name: /Start recording/ }));
    await statusIs("Recording");
    emitNamed("meeting://stop-request", null);
    await waitFor(() => expect(onRecorded).toHaveBeenCalledTimes(1));
    expect(audio.captures[0]?.stopped).toBe(true);
  });

  it("records, pauses (microphone released), resumes into a new part, and stops into the new meeting", async () => {
    const user = userEvent.setup();
    const audio = fakeAudio();
    const { onRecorded } = await mount(audio);
    await user.type(dialog().getByLabelText("Meeting title"), "Launch sync");
    await user.type(dialog().getByLabelText(/Participants/), "Asha, Ravi");
    await user.click(dialog().getByLabelText(/Everyone in this meeting/));
    await user.click(dialog().getByRole("button", { name: /Start recording/ }));

    await statusIs("Recording");
    expect(dialog().getByRole("timer").textContent).toBe("0:00");
    expect(audio.captures).toHaveLength(1);
    expect(audio.sent).toBeGreaterThan(0);
    expect((await voiceIpc.status()).state).toBe("recording");

    await user.click(dialog().getByRole("button", { name: "Pause" }));
    await statusIs("Paused");
    expect(audio.captures[0]?.stopped).toBe(true);

    await user.click(dialog().getByRole("button", { name: "Resume" }));
    await statusIs("Recording");
    expect(audio.captures).toHaveLength(2);
    expect(new Set(audio.sessions).size).toBe(1);

    await user.click(dialog().getByRole("button", { name: /Stop and save/ }));
    await waitFor(() => expect(onRecorded).toHaveBeenCalledTimes(1));
    expect(audio.captures[1]?.stopped).toBe(true);
    expect((await voiceIpc.status()).state).toBe("idle");
    const job = await meetingAi.jobGet(onRecorded.mock.calls[0]?.[0] as string);
    expect(job).not.toBeNull();
  });

  it("a missing microphone shows the reason and leaves nothing half-started", async () => {
    const user = userEvent.setup();
    const { onRecorded } = await mount(fakeAudio({
      startCapture: (async () => { throw new CaptureError("mic_permission_denied", "Pluto isn't allowed to use the microphone."); }) as never,
    }));
    await user.click(dialog().getByLabelText(/Everyone in this meeting/));
    await user.click(dialog().getByRole("button", { name: /Start recording/ }));
    expect(await dialog().findByRole("alert")).toBeTruthy();
    expect(dialog().getByRole("alert").textContent).toMatch(/isn't allowed to use the microphone/);
    expect(dialog().getByRole("button", { name: /Start recording/ })).toBeTruthy();
    expect(onRecorded).not.toHaveBeenCalled();
    expect((await voiceIpc.status()).state).not.toBe("recording");
  });

  it("a microphone that disappears pauses the recording with a message", async () => {
    const user = userEvent.setup();
    const audio = fakeAudio();
    await mount(audio);
    await user.click(dialog().getByLabelText(/Everyone in this meeting/));
    await user.click(dialog().getByRole("button", { name: /Start recording/ }));
    await statusIs("Recording");
    audio.captures[0]?.onEnded?.();
    await statusIs("Paused");
    expect(dialog().getByRole("alert").textContent).toMatch(/microphone stopped/);
    expect(dialog().getByRole("button", { name: "Resume" })).toBeTruthy();
  });

  it("cannot be dismissed with Escape while a recording is running", async () => {
    const user = userEvent.setup();
    const { onClose } = await mount(fakeAudio());
    await user.click(dialog().getByLabelText(/Everyone in this meeting/));
    await user.click(dialog().getByRole("button", { name: /Start recording/ }));
    await statusIs("Recording");
    await user.keyboard("{Escape}");
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog")).toBeTruthy();
  });
});
