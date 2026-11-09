import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CaptureError } from "@/audio";
import type { Proposal } from "@/ipc/generated";
import { ProposalCard, secondsLeft } from "./ProposalCard";
import type { AudioDeps } from "./VoiceProvider";

afterEach(cleanup);

const frames: ArrayBuffer[] = [];
function fakeAudio(over: Partial<AudioDeps> = {}): AudioDeps & { stopped: { n: number }; pushed: string[] } {
  const stopped = { n: 0 };
  const pushed: string[] = [];
  class FakeSender {
    constructor(readonly sessionId: string) { pushed.push(sessionId); }
    send(f: ArrayBuffer) { frames.push(f); }
    async close() { /* nothing to flush */ }
  }
  return {
    startCapture: (async (opts: { onFrame: (f: ArrayBuffer) => void }) => {
      opts.onFrame(new ArrayBuffer(1024));
      return { deviceSampleRate: 48000, stop: async () => { stopped.n += 1; } };
    }) as never,
    FrameSender: FakeSender as never,
    pushFrame: (async () => undefined) as never,
    createPlayer: () => ({ enqueue: () => undefined, stop: () => undefined }),
    stopped, pushed, ...over,
  };
}

let ipcMod: typeof import("@/ipc/voiceClient");
beforeEach(async () => {
  vi.resetModules(); // a fresh in-memory shell for each test
  ipcMod = await import("@/ipc/voiceClient");
});

async function mount(audio: AudioDeps, onOpenSettings = vi.fn()) {
  const { VoiceProvider: P } = await import("./VoiceProvider");
  const { VoiceBar: B } = await import("./VoiceBar");
  const { VoicePage: H } = await import("./VoicePage");
  render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <P audio={audio}><B onOpenSettings={onOpenSettings} /><H /></P>
    </QueryClientProvider>,
  );
  return onOpenSettings;
}
const enableCloud = async () => {
  await ipcMod.voiceIpc.settingsSet({ cloud_voice_enabled: true });
  await ipcMod.voiceIpc.keySet("gnani", "k");
};
const bar = () => within(screen.getByRole("region", { name: "Voice" }));

describe("Voice bar and approval (T-304), against the in-memory shell", () => {
  it("says cloud voice is off and points to Settings", async () => {
    const user = userEvent.setup();
    const open = await mount(fakeAudio());
    expect(await bar().findByText("Cloud voice is off")).toBeTruthy();
    await user.click(bar().getByRole("button", { name: /Turn on in Settings/ }));
    expect(open).toHaveBeenCalled();
  });

  it("talk, send, approve: the card appears, the change is confirmed, the history records it", async () => {
    const user = userEvent.setup();
    await enableCloud();
    const audio = fakeAudio();
    await mount(audio);
    await user.click(await bar().findByRole("button", { name: "Push to talk" }));
    expect(await bar().findByText("Listening…")).toBeTruthy();
    expect(audio.pushed.length).toBe(1);
    expect(frames.length).toBeGreaterThan(0);
    await user.click(bar().getByRole("button", { name: "Stop and send" }));
    expect(audio.stopped.n).toBe(1);

    const card = await screen.findByRole("alertdialog", { name: "Approve this change?" }, { timeout: 3000 });
    expect(within(card).getByText(/Send the revised budget/)).toBeTruthy();
    expect(within(card).getByRole("timer")).toBeTruthy();
    expect(await bar().findByText(/You: “Kal tak budget report/)).toBeTruthy();

    await user.click(within(card).getByRole("button", { name: "Approve" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    const history = await screen.findByRole("list", { name: "Assistant history" });
    expect(await within(history).findByText(/Added “Send the revised budget”/)).toBeTruthy();
    expect(within(history).getByText("task_create")).toBeTruthy();
  });

  it("the push-to-talk hotkey opens the microphone for its session and closes it after (T-301)", async () => {
    const { emitNamed } = await import("@/ipc/namedEvents");
    const audio = fakeAudio();
    await mount(audio);
    await bar().findByRole("button", { name: "Push to talk" });
    // Rust reports Listening for a session the page didn't open: the hotkey was pressed.
    act(() => emitNamed("voice://status", { state: "listening", mode: "assistant", detail: null, session_id: "hk-1" }));
    await waitFor(() => expect(audio.pushed).toEqual(["hk-1"]));
    expect(bar().getByRole("button", { name: "Stop and send" })).toBeTruthy();
    // A repeat of the same status changes nothing.
    act(() => emitNamed("voice://status", { state: "listening", mode: "assistant", detail: null, session_id: "hk-1" }));
    // Released: Rust moves on to Thinking, and the page lets go of the microphone.
    act(() => emitNamed("voice://status", { state: "thinking", mode: "assistant", detail: null, session_id: null }));
    await waitFor(() => expect(audio.stopped.n).toBe(1));
    expect(audio.pushed).toEqual(["hk-1"]);
    await waitFor(() => expect(bar().getByRole("button", { name: "Push to talk" }).getAttribute("aria-pressed")).toBe("false"));
  });

  it("Cancel stops a turn in the middle and sends nothing", async () => {
    const user = userEvent.setup();
    await enableCloud();
    const audio = fakeAudio();
    await mount(audio);
    await user.click(await bar().findByRole("button", { name: "Push to talk" }));
    await bar().findByText("Listening…");
    await user.click(bar().getByRole("button", { name: "Cancel" }));
    expect(audio.stopped.n).toBe(1);
    await waitFor(() => expect(bar().getByRole("button", { name: "Push to talk" }).getAttribute("aria-pressed")).toBe("false"));
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(bar().queryByRole("button", { name: "Cancel" })).toBeNull();
  });

  it("with the native microphone on, this window opens none and Talk hands the turn to Rust", async () => {
    const user = userEvent.setup();
    const { ipc } = await import("@/ipc");
    await ipc.settingSet("voice.native_mic", true);
    await enableCloud();
    const toggle = vi.spyOn(ipcMod.voiceIpc, "handsFreeToggle");
    const audio = fakeAudio();
    await mount(audio);
    await user.click(await bar().findByRole("button", { name: "Push to talk" }));
    await waitFor(() => expect(toggle).toHaveBeenCalledTimes(1));
    expect(audio.pushed.length, "no microphone opened here").toBe(0);
  });

  it("if Rust cannot open the microphone the window takes over for the running turn", async () => {
    const { emitNamed } = await import("@/ipc/namedEvents");
    const { ipc } = await import("@/ipc");
    await ipc.settingSet("voice.native_mic", true);
    await enableCloud();
    const audio = fakeAudio();
    await mount(audio);
    await bar().findByRole("button", { name: "Push to talk" });
    act(() => emitNamed("voice://status", { state: "listening", mode: "assistant", detail: null, session_id: "hk-2" }));
    expect(audio.pushed.length).toBe(0);
    vi.spyOn(ipcMod.voiceIpc, "status").mockResolvedValue({ state: "listening", mode: "assistant", detail: null, session_id: "hk-2" });
    act(() => emitNamed("voice://native-mic-failed", "no microphone found"));
    await waitFor(() => expect(audio.pushed).toEqual(["hk-2"]));
  });

  it("speaks answers that ran at once and the result of a click approval, but not a proposal twice", async () => {
    const user = userEvent.setup();
    const { emitNamed } = await import("@/ipc/namedEvents");
    const speak = vi.spyOn(ipcMod.voiceIpc, "speak");
    // Outside Tauri a playback Channel can't be built; a stand-in lets `speak` be called.
    vi.doMock("@/audio", async (original) => ({ ...(await original<typeof import("@/audio")>()), playbackChannel: () => ({}) }));
    await enableCloud();
    await mount(fakeAudio());
    await bar().findByRole("button", { name: "Push to talk" });
    // A read ("meetings kholo") arrives with its result already set: it is still spoken.
    const decision = { intent: "navigate", tool: "open_page", args: {}, confidence: 1, source: "llm", latency_ms: 1 };
    const result = { tool: "open_page", ok: true, message: "Opening Meetings.", record_id: null };
    act(() => emitNamed("voice://turn", { interaction_id: "nav-1", transcript: "open meetings", language: "en-IN", decision, reply_text: "Opening Meetings.", proposal: null, result } as never));
    await waitFor(() => expect(speak.mock.calls.map((c) => c[0])).toContain("Opening Meetings."));

    // A proposal is spoken once; after the click its result is spoken, not the summary again.
    await user.click(bar().getByRole("button", { name: "Push to talk" }));
    await user.click(bar().getByRole("button", { name: "Stop and send" }));
    const card = await screen.findByRole("alertdialog", {}, { timeout: 3000 });
    await waitFor(() => expect(speak.mock.calls.some((c) => /Send the revised budget/.test(c[0]))).toBe(true));
    const summaries = () => speak.mock.calls.filter((c) => /Send the revised budget/.test(c[0]) && !/^Added/.test(c[0])).length;
    await user.click(within(card).getByRole("button", { name: "Approve" }));
    await waitFor(() => expect(speak.mock.calls.some((c) => /^Added/.test(c[0]))).toBe(true));
    expect(summaries()).toBe(1);
  });

  it("reject changes nothing and the history says so", async () => {
    const user = userEvent.setup();
    await enableCloud();
    await mount(fakeAudio());
    await user.click(await bar().findByRole("button", { name: "Push to talk" }));
    await user.click(bar().getByRole("button", { name: "Stop and send" }));
    const card = await screen.findByRole("alertdialog", {}, { timeout: 3000 });
    await user.click(within(card).getByRole("button", { name: "Reject" }));
    const history = await screen.findByRole("list", { name: "Assistant history" });
    expect(await within(history).findByText("Okay, I didn't change anything.")).toBeTruthy();
  });

  it("a microphone problem is shown as a message and the turn is cancelled", async () => {
    const user = userEvent.setup();
    await enableCloud();
    await mount(fakeAudio({ startCapture: (async () => { throw new CaptureError("mic_permission_denied", "Pluto isn't allowed to use the microphone."); }) as never }));
    await user.click(await bar().findByRole("button", { name: "Push to talk" }));
    expect(await bar().findByText(/isn't allowed to use the microphone/)).toBeTruthy();
    await user.click(bar().getByRole("button", { name: "Dismiss" }));
    await waitFor(() => expect(bar().queryByText(/isn't allowed/)).toBeNull());
    expect(bar().getByRole("button", { name: "Push to talk" }).getAttribute("aria-pressed")).toBe("false");
  });

  it("asking while cloud voice is off gives the shell's message", async () => {
    await mount(fakeAudio());
    await bar().findByText("Cloud voice is off");
    await act(async () => {
      await expect(ipcMod.voiceIpc.pttStart()).rejects.toMatchObject({ code: "cloud_voice_disabled" });
    });
  });
});

describe("ProposalCard", () => {
  const proposal = (over: Partial<Proposal> = {}): Proposal => ({
    id: "p1", interaction_id: "i1", tool: "task_create", args: { title: "Send report", due_date: "2026-11-06", priority: "HIGH" },
    summary_text: "Add “Send report”, due Friday?", language: "en-IN", expires_at: 100_000, ...over,
  });

  it("counts down in whole seconds and asks to close when the time is up", () => {
    expect(secondsLeft(100_000, 70_000)).toBe(30);
    expect(secondsLeft(100_000, 99_001)).toBe(1);
    expect(secondsLeft(100_000, 120_000)).toBe(0);
    vi.useFakeTimers();
    let t = 70_000;
    const onExpire = vi.fn();
    render(<ProposalCard proposal={proposal()} onRespond={vi.fn()} onExpire={onExpire} now={() => t} />);
    expect(screen.getByRole("timer").textContent).toContain("30s");
    t = 101_000;
    act(() => { vi.advanceTimersByTime(300); });
    expect(onExpire).toHaveBeenCalled();
    vi.useRealTimers();
  });

  it("Edit lets the title and date change, and sends the whole edited arguments", async () => {
    const user = userEvent.setup();
    const onRespond = vi.fn();
    render(<ProposalCard proposal={proposal({ expires_at: Date.now() + 30_000 })} onRespond={onRespond} onExpire={vi.fn()} />);
    await user.click(screen.getByRole("button", { name: /Edit/ }));
    const title = screen.getByLabelText("Title");
    await user.clear(title);
    await user.type(title, "Send the final report");
    expect(screen.queryByLabelText(/priority/i)).toBeNull();
    await user.click(screen.getByRole("button", { name: /Save and approve/ }));
    expect(onRespond).toHaveBeenCalledWith({ kind: "edit", args: { title: "Send the final report", due_date: "2026-11-06", priority: "HIGH" } });
  });

  it("a proposal with nothing editable has no Edit button", () => {
    render(<ProposalCard proposal={proposal({ args: { minutes: 25 }, expires_at: Date.now() + 30_000 })} onRespond={vi.fn()} onExpire={vi.fn()} />);
    expect(screen.queryByRole("button", { name: /Edit/ })).toBeNull();
    expect(screen.getByRole("button", { name: "Approve" })).toBeTruthy();
  });
});
