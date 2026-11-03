import { cleanup, render, screen, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { TimePage } from "./TimePage";

beforeAll(() => {
  window.matchMedia ??= ((q: string) => ({ matches: false, media: q, addEventListener() {}, removeEventListener() {} })) as never;
  window.ResizeObserver ??= class { observe() {} unobserve() {} disconnect() {} } as never;
});
afterEach(cleanup);

describe("Time analytics", () => {
  it("switches between General and Analytics, keeping General as it was", async () => {
    const user = userEvent.setup();
    render(
      <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
        <TimePage />
      </QueryClientProvider>,
    );
    expect(await screen.findByRole("group", { name: "Filters" })).toBeTruthy();
    expect(screen.queryByTestId("time-analytics")).toBeNull();
    const view = screen.getByRole("radiogroup", { name: "Time view" });
    await user.click(within(view).getByText("Analytics"));
    expect(await screen.findByTestId("time-analytics")).toBeTruthy();
    expect(screen.getByText("Top apps")).toBeTruthy();
    expect(screen.getByText("Screen time by hour")).toBeTruthy();
    expect(screen.queryByText("Busiest hours")).toBeNull();
    expect(screen.queryByText("Total time")).toBeNull();
    await user.click(within(view).getByText("General"));
    expect(screen.queryByTestId("time-analytics")).toBeNull();
    expect(screen.getByText("Total time")).toBeTruthy();
  });
});
