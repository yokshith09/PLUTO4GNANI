import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ErrorBoundary } from "./ErrorBoundary";

let broken = true;
function Flaky() {
  if (broken) throw new Error("boom");
  return <p>fine</p>;
}

beforeEach(() => { broken = true; vi.spyOn(console, "error").mockImplementation(() => undefined); });
afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.useRealTimers(); });

describe("a crashing screen", () => {
  it("shows a calm message instead of a blank window, and Try again recovers", () => {
    render(<ErrorBoundary><Flaky /></ErrorBoundary>);
    expect(screen.getByRole("alert").textContent).toContain("Nothing is lost");
    broken = false;
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(screen.getByText("fine")).toBeTruthy();
  });

  it("the pet window shows nothing and tries again by itself", async () => {
    vi.useFakeTimers();
    const { container } = render(<ErrorBoundary quiet><Flaky /></ErrorBoundary>);
    expect(container.textContent).toBe("");
    broken = false;
    await vi.advanceTimersByTimeAsync(2100);
    expect(screen.getByText("fine")).toBeTruthy();
  });

  it("gives up after three quiet tries rather than looping forever", async () => {
    vi.useFakeTimers();
    const { container } = render(<ErrorBoundary quiet><Flaky /></ErrorBoundary>);
    await vi.advanceTimersByTimeAsync(10_000);
    expect(container.textContent).toBe("");
    expect(console.error).toHaveBeenCalled();
  });
});
