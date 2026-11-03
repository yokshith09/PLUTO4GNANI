import { cleanup, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ipc } from "@/ipc";
import type { DomainUsage } from "@/ipc";
import { PrivacyRadar } from "./PrivacyRadar";

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

const site = (domain: string, is_tracker: boolean, total_seconds = 600): DomainUsage => ({ domain, browser: "Google Chrome", category: "Other", total_seconds, sessions: 1, is_tracker });

function renderRadar(enabled: boolean, domains: DomainUsage[]) {
  vi.spyOn(ipc, "usageSummary").mockResolvedValue({ total_seconds: 0, apps: [], domains, hourly_seconds: new Array<number>(24).fill(0) });
  return render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <PrivacyRadar from={0} to={1} enabled={enabled} />
    </QueryClientProvider>,
  );
}

describe("PrivacyRadar", () => {
  it("asks to turn on website tracking when it is off", () => {
    renderRadar(false, []);
    expect(screen.getByText(/Turn on website tracking/)).toBeTruthy();
  });

  it("lists only the visited sites that are known trackers", async () => {
    renderRadar(true, [site("doubleclick.net", true), site("github.com", false)]);
    expect(await screen.findByText(/Known ad or tracking companies: 1 of 2 visited sites/)).toBeTruthy();
    expect(screen.getByLabelText("Known trackers you visited").textContent).toContain("doubleclick.net");
    expect(screen.getByLabelText("Known trackers you visited").textContent).not.toContain("github.com");
  });

  it("never calls an unflagged site clean", async () => {
    renderRadar(true, [site("github.com", false)]);
    expect(await screen.findByText("No visited site is on the known-tracker list")).toBeTruthy();
    expect(screen.getByText(/not proven clean/)).toBeTruthy();
  });
});
