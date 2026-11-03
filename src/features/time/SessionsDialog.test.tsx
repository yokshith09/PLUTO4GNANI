import { cleanup, render, screen, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { ipc } from "@/ipc";
import { SessionsDialog } from "./SessionsDialog";

beforeAll(() => {
  globalThis.ResizeObserver ??= class { observe() {} unobserve() {} disconnect() {} } as never;
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

const at = (day: number, h: number, m = 0) => new Date(2026, 9, day, h, m).getTime();
const wrap = (node: React.ReactNode) => render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>{node}</QueryClientProvider>);

describe("one app's sessions (owner 2026-10-26: clean and rich)", () => {
  it("shows the totals, the hours you use it, the days and every session", async () => {
    vi.spyOn(ipc, "usageAppSessions").mockResolvedValue([
      { started_at: at(25, 9), ended_at: at(25, 10, 30) },
      { started_at: at(26, 9, 15), ended_at: at(26, 9, 45) },
    ]);
    wrap(<SessionsDialog detail={{ kind: "app", name: "VS Code", category: "Coding" }} from={at(25, 0)} to={at(27, 0)} onClose={() => {}} />);
    const summary = await screen.findByRole("list", { name: "Summary" });
    expect(within(summary).getByText("2h")).toBeTruthy(); // total: 1h30 + 30m
    expect(within(summary).getByText("Sessions").nextSibling?.textContent).toBe("2");
    expect(screen.getByRole("img", { name: /Time by hour of the day. Most used: 9 AM/ })).toBeTruthy();
    expect(screen.getByRole("region", { name: "By day" })).toBeTruthy();
    expect(screen.getAllByRole("listitem").length).toBeGreaterThanOrEqual(2 + 4);
  });

  it("says so when nothing was recorded", async () => {
    vi.spyOn(ipc, "usageAppSessions").mockResolvedValue([]);
    wrap(<SessionsDialog detail={{ kind: "app", name: "Slack", category: "Communication" }} from={at(25, 0)} to={at(27, 0)} onClose={() => {}} />);
    expect(await screen.findByText(/Nothing was recorded for Slack/)).toBeTruthy();
  });
});
