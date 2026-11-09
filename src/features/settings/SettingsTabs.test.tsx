import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { SettingsPage } from "./SettingsPage";

beforeAll(() => {
  window.matchMedia ??= ((q: string) => ({ matches: false, media: q, addEventListener() {}, removeEventListener() {} })) as never;
});
afterEach(cleanup);
const wrap = (ui: React.ReactElement) => <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>{ui}</QueryClientProvider>;

describe("Settings has two tabs", () => {
  it("General holds the app's settings, Voice & privacy holds voice, keys and what is sent", async () => {
    const user = userEvent.setup();
    render(wrap(<SettingsPage />));
    const tabs = screen.getByLabelText("Settings sections");
    expect(tabs).toBeTruthy();

    // General first: the app's own settings, and none of the voice ones.
    expect(await screen.findByText("Keyboard shortcuts")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Export…" })).toBeTruthy();
    expect(screen.queryByText("Use cloud voice")).toBeNull();

    await user.click(screen.getByRole("radio", { name: "Voice & privacy" }));
    expect(await screen.findByText("Use cloud voice")).toBeTruthy();
    expect(screen.getByText("Sent to the cloud")).toBeTruthy();
    expect(screen.queryByText("Keyboard shortcuts")).toBeNull();
    expect(screen.queryByRole("button", { name: "Export…" })).toBeNull();

    await user.click(screen.getByRole("radio", { name: "General" }));
    expect(await screen.findByText("Keyboard shortcuts")).toBeTruthy();
  });

  it("can be driven from outside, so Turn on in Settings lands on the voice tab", async () => {
    const seen: string[] = [];
    const user = userEvent.setup();
    render(wrap(<SettingsPage tab="voice" onTab={(t) => seen.push(t)} />));
    expect(await screen.findByText("Use cloud voice")).toBeTruthy();
    await user.click(screen.getByRole("radio", { name: "General" }));
    expect(seen).toEqual(["general"]);
  });
});
