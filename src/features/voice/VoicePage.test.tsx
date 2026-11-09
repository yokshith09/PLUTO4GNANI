import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

afterEach(cleanup);

let voiceIpc: typeof import("@/ipc/voiceClient").voiceIpc;
beforeEach(async () => {
  vi.resetModules();
  ({ voiceIpc } = await import("@/ipc/voiceClient"));
});

async function mount() {
  const { VoicePage } = await import("./VoicePage");
  render(<QueryClientProvider client={new QueryClient()}><VoicePage /></QueryClientProvider>);
}

describe("Voice page", () => {
  it("a typed command runs through the assistant and shows up in the history", async () => {
    const user = userEvent.setup();
    await mount();
    await user.type(screen.getByLabelText("Type a command"), "which tasks do I have today{Enter}");
    expect(screen.queryByRole("list", { name: "Assistant history" }), "history starts hidden").toBeNull();
    await user.click(await screen.findByRole("button", { name: /Show history/ }));
    expect(await screen.findByText("“which tasks do I have today”", {}, { timeout: 4000 })).toBeTruthy();
    expect((screen.getByLabelText("Type a command") as HTMLInputElement).value).toBe("");
  });

  it("an example runs when pressed", async () => {
    const user = userEvent.setup();
    const run = vi.spyOn(voiceIpc, "textTurn");
    await mount();
    await user.click(screen.getByRole("button", { name: "Open Meetings" }));
    await waitFor(() => expect(run).toHaveBeenCalledWith("Open Meetings"));
  });

  it("clear history asks first, then empties it", async () => {
    const user = userEvent.setup();
    await mount();
    await user.type(screen.getByLabelText("Type a command"), "hello{Enter}");
    await user.click(await screen.findByRole("button", { name: /Show history/ }));
    await screen.findByText("“hello”", {}, { timeout: 4000 });
    await user.click(screen.getByRole("button", { name: /Clear history/ }));
    expect(screen.getByText(/Delete all 1 entries/)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(screen.queryByText("“hello”")).toBeNull());
    expect(screen.getByText(/Nothing yet/)).toBeTruthy();
  });
});
