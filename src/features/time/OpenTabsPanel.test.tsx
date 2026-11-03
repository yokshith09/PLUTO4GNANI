import { act, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ipc } from "@/ipc";
import { OpenTabsPanel } from "./OpenTabsPanel";

afterEach(() => vi.restoreAllMocks());

const renderPanel = (enabled: boolean) =>
  render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <OpenTabsPanel enabled={enabled} />
    </QueryClientProvider>,
  );

describe("OpenTabsPanel", () => {
  it("asks for permission first when website tracking is off, and never calls the shell", async () => {
    const spy = vi.spyOn(ipc, "petOpenTabs");
    renderPanel(false);
    expect(await screen.findByRole("button", { name: "Turn on app and website time" })).toBeTruthy();
    expect(spy).not.toHaveBeenCalled();
  });

  it("lists live tabs and marks the active one", async () => {
    vi.spyOn(ipc, "petOpenTabs").mockResolvedValue([
      { browser: "Google Chrome", window_index: 0, tab_index: 0, title: "Today", domain: "localhost", active: true },
      { browser: "Google Chrome", window_index: 0, tab_index: 1, title: "GitHub", domain: "github.com", active: false },
    ]);
    renderPanel(true);
    expect(await screen.findByText("Today")).toBeTruthy();
    expect(screen.getByText("GitHub")).toBeTruthy();
    expect(screen.getByText("github.com")).toBeTruthy();
  });

  it("closing a tab calls the shell with its browser and indices, then refreshes the list", async () => {
    const closeSpy = vi.spyOn(ipc, "petCloseTab").mockResolvedValue(true);
    vi.spyOn(ipc, "petOpenTabs")
      .mockResolvedValueOnce([{ browser: "Google Chrome", window_index: 0, tab_index: 2, title: "Stack Overflow", domain: "stackoverflow.com", active: false }])
      .mockResolvedValueOnce([]);
    renderPanel(true);
    const closeButton = await screen.findByRole("button", { name: /Close Stack Overflow/ });
    await act(async () => { closeButton.click(); });
    expect(closeSpy).not.toHaveBeenCalled(); // asks first
    await act(async () => { within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Close" }).click(); });
    expect(closeSpy).toHaveBeenCalledWith("Google Chrome", 0, 2);
    await waitFor(() => expect(screen.queryByText("Stack Overflow")).toBeNull());
  });

  it("an empty live list explains only the front window can be read", async () => {
    vi.spyOn(ipc, "petOpenTabs").mockResolvedValue([]);
    renderPanel(true);
    expect(await screen.findByText(/No browser tabs found/)).toBeTruthy();
  });
});
