import { render, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "@/ipc";
import { useWellbeingNudges } from "./useWellbeing";
import { parseWellbeing, type Wellbeing } from "./wellbeing";

function Harness() {
  useWellbeingNudges();
  return null;
}

/** The saved settings, with a break already overdue by an hour. */
const overdueBreak = (): Wellbeing => ({
  break: { on: true, everyMin: 50, lastAt: Date.now() - 60 * 60_000, lastScreenSec: null },
  water: { on: false, everyMin: 90, lastAt: null, lastScreenSec: null },
});

describe("break and water nudges in the app", () => {
  let saved: Record<string, unknown>;
  beforeEach(() => {
    // Quiet hours are on by default (night), so without this the tests would pass or fail with
    // the hour they happen to run at. Tests about quiet hours set their own.
    saved = { "ui.nudges": { quiet: { on: false, from: "22:00", to: "07:00" } } };
    vi.spyOn(api.ipc, "prefsGet").mockImplementation(async (k: string) => (saved[k] ?? null) as never);
    vi.spyOn(api.ipc, "prefsSet").mockImplementation(async (k: string, v: unknown) => { saved[k] = v; return null as never; });
    vi.spyOn(api.ipc, "trackingStatus").mockResolvedValue({ enabled: false, supported: true, running: false } as never);
  });
  afterEach(() => vi.restoreAllMocks());

  const run = () => render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><Harness /></QueryClientProvider>,
  );

  it("says something when a break is overdue, and remembers it so it does not repeat", async () => {
    saved["ui.wellbeing"] = overdueBreak();
    const say = vi.spyOn(api, "petSay").mockResolvedValue(undefined as never);
    run();
    await waitFor(() => expect(say).toHaveBeenCalledWith(expect.objectContaining({ pose: "stow" })));
    expect(say.mock.calls[0]?.[0].text).toMatch(/stretch/i);
    await waitFor(() => expect(parseWellbeing(saved["ui.wellbeing"]).break.lastAt).toBeGreaterThan(Date.now() - 5_000));
    expect(say).toHaveBeenCalledTimes(1);
  });

  it("says nothing while both are switched off", async () => {
    const say = vi.spyOn(api, "petSay").mockResolvedValue(undefined as never);
    run();
    await new Promise((r) => setTimeout(r, 60));
    expect(say).not.toHaveBeenCalled();
  });

  it("stays quiet during quiet hours", async () => {
    saved["ui.wellbeing"] = overdueBreak();
    // Quiet right now, whatever the hour the tests run at.
    const h = new Date().getHours();
    const two = (n: number) => String((n + 24) % 24).padStart(2, "0");
    saved["ui.nudges"] = { quiet: { on: true, from: `${two(h - 1)}:00`, to: `${two(h + 2)}:00` } };
    const say = vi.spyOn(api, "petSay").mockResolvedValue(undefined as never);
    run();
    await new Promise((r) => setTimeout(r, 60));
    expect(say).not.toHaveBeenCalled();
  });

  it("counts time at the screen when app tracking is on", async () => {
    saved["ui.wellbeing"] = {
      break: { on: true, everyMin: 50, lastAt: Date.now() - 10 * 60 * 60_000, lastScreenSec: 1_000 },
      water: { on: false, everyMin: 90, lastAt: null, lastScreenSec: null },
    };
    vi.spyOn(api.ipc, "trackingStatus").mockResolvedValue({ enabled: true, supported: true, running: true } as never);
    // Ten hours have passed but only five minutes at the screen: nothing is due.
    const usage = vi.spyOn(api.ipc, "usageSummary").mockResolvedValue({ total_seconds: 1_300, apps: [], hourly_seconds: [] } as never);
    const say = vi.spyOn(api, "petSay").mockResolvedValue(undefined as never);
    run();
    await waitFor(() => expect(usage).toHaveBeenCalled());
    await new Promise((r) => setTimeout(r, 60));
    expect(say).not.toHaveBeenCalled();
  });
});
