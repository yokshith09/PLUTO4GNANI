import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ipc } from "@/ipc";
import { NudgeSettings } from "./NudgeSettings";

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

describe("Pet notifications settings (FR-96)", () => {
  it("lets the user set nudges a day, the gap and quiet hours", async () => {
    let saved: unknown = null;
    vi.spyOn(ipc, "prefsGet").mockImplementation(async () => saved as never);
    vi.spyOn(ipc, "prefsSet").mockImplementation(async (_k, v) => { saved = v; return null; });
    const user = userEvent.setup();
    render(<QueryClientProvider client={new QueryClient()}><NudgeSettings /></QueryClientProvider>);
    await user.selectOptions(await screen.findByLabelText("Nudges a day"), "2");
    await user.selectOptions(screen.getByLabelText("Time between nudges"), "120");
    expect((screen.getByLabelText("Quiet from") as HTMLInputElement).value).toBe("22:00");
    await user.click(screen.getByRole("switch", { name: "Quiet hours" }));
    await waitFor(() => expect(saved).toMatchObject({ maxPerDay: 2, gapMin: 120, quiet: { on: false } }));
    expect(screen.queryByLabelText("Quiet from")).toBeNull();
  });
});
