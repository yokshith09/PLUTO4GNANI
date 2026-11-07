import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AudioDeps } from "./VoiceProvider";

afterEach(cleanup);

function fakeAudio(): AudioDeps & { opened: string[]; stopped: { n: number } } {
  const opened: string[] = [];
  const stopped = { n: 0 };
  class FakeSender {
    constructor(readonly sessionId: string) { opened.push(sessionId); }
    send() { /* frames go nowhere in the mock */ }
    async close() { /* nothing to flush */ }
  }
  return {
    startCapture: (async () => ({ deviceSampleRate: 48000, stop: async () => { stopped.n += 1; } })) as never,
    FrameSender: FakeSender as never,
    pushFrame: (async () => undefined) as never,
    createPlayer: () => ({ enqueue: () => undefined, stop: () => undefined }),
    opened, stopped,
  };
}

let ipc: typeof import("@/ipc/voiceClient");
beforeEach(async () => {
  vi.resetModules(); // a fresh in-memory shell for each test
  ipc = await import("@/ipc/voiceClient");
});

const ready = async () => {
  await ipc.voiceIpc.settingsSet({ cloud_voice_enabled: true });
  await ipc.voiceIpc.keySet("gnani", "k");
};

/** Teaches the in-memory shell directly (the dialog has its own test). */
const teach = async () => {
  await ipc.voiceIpc.wakeTeachBegin();
  for (let i = 0; i < 6; i++) await ipc.voiceIpc.wakeClipStop("clip");
  await ipc.voiceIpc.wakeTeachFinish();
};

async function mount(audio: AudioDeps) {
  const { VoiceProvider } = await import("./VoiceProvider");
  const { VoiceBar } = await import("./VoiceBar");
  const { VoiceSection } = await import("@/features/settings/VoiceSection");
  render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <VoiceProvider audio={audio}><VoiceSection /><VoiceBar onOpenSettings={() => undefined} /></VoiceProvider>
    </QueryClientProvider>,
  );
}

describe("Always-on listening (T-705)", () => {
  it("can't be turned on before Pluto knows your voice", async () => {
    await ready();
    await mount(fakeAudio());
    const toggle = await screen.findByRole("switch", { name: "Always-on listening" });
    expect(toggle.hasAttribute("disabled")).toBe(true);
    expect(screen.getByRole("button", { name: "Teach Pluto your voice" })).toBeTruthy();
  });

  it("asks before turning on, then listens with one microphone and says so", async () => {
    const user = userEvent.setup();
    await ready();
    await teach();
    const audio = fakeAudio();
    await mount(audio);
    const toggle = await screen.findByRole("switch", { name: "Always-on listening" });
    await waitFor(() => expect(toggle.hasAttribute("disabled")).toBe(false));
    await user.click(toggle);
    const dialog = await screen.findByRole("dialog", { name: "Turn on always-on listening?" });
    expect(within(dialog).getByText(/Nothing is sent until you say “Pluto”/)).toBeTruthy();
    expect(audio.opened).toEqual([]);
    await user.click(within(dialog).getByRole("button", { name: "Turn on" }));

    await waitFor(() => expect(audio.opened).toHaveLength(1));
    expect(audio.opened[0]).toMatch(/^wake-/);
    const bar = within(screen.getByRole("region", { name: "Voice" }));
    expect(await bar.findByText("Say “Pluto” to start")).toBeTruthy();
    expect(screen.getByText("Listening for “Pluto”.")).toBeTruthy();

    // Push-to-talk now uses the same microphone: no second capture opens.
    await user.click(bar.getByRole("button", { name: "Push to talk" }));
    await waitFor(() => expect(bar.getByRole("button", { name: "Stop and send" })).toBeTruthy());
    expect(audio.opened).toHaveLength(1);
  });

  it("turning it off closes the microphone", async () => {
    const user = userEvent.setup();
    await ready();
    await teach();
    await ipc.voiceIpc.settingsSet({ always_on_enabled: true });
    const audio = fakeAudio();
    await mount(audio);
    await waitFor(() => expect(audio.opened).toHaveLength(1));
    await user.click(await screen.findByRole("switch", { name: "Always-on listening" }));
    await waitFor(() => expect(audio.stopped.n).toBe(1));
  });
});

describe("Teaching Pluto your voice", () => {
  it("asks again for an unclear recording, finishes once there are enough, and keeps nothing on cancel", async () => {
    const user = userEvent.setup();
    const { TeachDialog, ASK_FOR } = await import("./WakeSetup");
    let clips = 0;
    const record = vi.fn(async () => {
      // The second recording is too quiet; every other one is kept.
      if (record.mock.calls.length === 2) return { ok: false, message: "Too quiet. Say “Pluto” again, a little louder.", clips };
      clips += 1;
      return { ok: true, message: "Got it.", clips };
    });
    const forget = vi.spyOn(ipc.voiceIpc, "wakeForget");
    const onClose = vi.fn();
    render(<TeachDialog record={record} onClose={onClose} />);
    await user.click(screen.getByRole("button", { name: "Start" }));
    const say = () => screen.getByRole("button", { name: "Say “Pluto”" });
    await user.click(await screen.findByRole("button", { name: "Say “Pluto”" }));
    await user.click(say());
    expect(await screen.findByText(/Too quiet/)).toBeTruthy();
    expect(screen.getByLabelText("Recordings").textContent).toBe(`1 of ${ASK_FOR}`);
    expect(screen.queryByRole("button", { name: "Finish" })).toBeNull();
    for (let i = 0; i < 5; i++) await user.click(say());
    // 6 good ones: enough to finish early; it also offers the remaining two.
    expect(screen.getByRole("button", { name: "Finish" })).toBeTruthy();
    expect(say()).toBeTruthy();

    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(forget).toHaveBeenCalledTimes(1);
    expect(onClose).toHaveBeenCalled();
  });
});
