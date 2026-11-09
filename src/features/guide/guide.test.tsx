import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ipc } from "@/ipc";
import { Tour } from "./Tour";
import { STEPS, TOUR_STEPS } from "./tourSteps";
import { GuideSection } from "./GuideSection";
import { CREDIT, GUIDE } from "./content";

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

describe("the tour (data)", () => {
  it("is made of real steps, starts on Today, covers privacy and ends on Settings with the way to replay it", () => {
    const ids = TOUR_STEPS.map((x) => x.id);
    expect(ids[0]).toBe("today");
    expect(ids).toContain("privacy");
    expect(ids[ids.length - 1]).toBe("settings");
    expect(new Set(ids).size).toBe(ids.length);
    for (const id of ids) expect(STEPS[id]).toBeTruthy();
    expect(STEPS.settings.body).toMatch(/Settings → Guide/);
  });
});

function wrap(ui: React.ReactElement) {
  return <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>{ui}</QueryClientProvider>;
}

describe("the spotlight tour", () => {
  beforeEach(() => {
    document.body.innerHTML = '<button data-tour="nav-today">Today</button><button data-tour="nav-tasks">Tasks</button>';
  });
  const steps = [STEPS.today, STEPS.tasks, STEPS.pet];

  it("goes Next and Back, opens each step's page, and Done finishes it", () => {
    const onNavigate = vi.fn();
    const onClose = vi.fn();
    render(<Tour steps={steps} onNavigate={onNavigate} onClose={onClose} />);
    expect(screen.getByRole("dialog", { name: "Pluto tour" })).toBeTruthy();
    expect(screen.getByText("Step 1 of 3")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    expect(screen.getByText("Step 2 of 3")).toBeTruthy();
    expect(onNavigate).toHaveBeenLastCalledWith("tasks");
    fireEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(screen.getByText("Step 1 of 3")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    expect(onClose).toHaveBeenCalledWith(true);
  });
  it("Skip and Escape end it without finishing, and Next has the focus", () => {
    const onClose = vi.fn();
    render(<Tour steps={steps} onNavigate={() => undefined} onClose={onClose} />);
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Next" }));
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(onClose).toHaveBeenLastCalledWith(false);
    fireEvent.click(screen.getByRole("button", { name: "Skip tour" }));
    expect(onClose).toHaveBeenCalledTimes(2);
  });
  it("keeps Tab inside the card", () => {
    render(<Tour steps={steps} onNavigate={() => undefined} onClose={() => undefined} />);
    const next = screen.getByRole("button", { name: "Next" });
    next.focus();
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Tab" });
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Skip tour" }));
  });
  it("a step whose target is missing still shows its card", () => {
    document.body.innerHTML = "";
    render(<Tour steps={[STEPS.privacy]} onNavigate={() => undefined} onClose={() => undefined} />);
    expect(screen.getByText(/Privacy radar/)).toBeTruthy();
  });
});

describe("Settings → Guide", () => {
  it("replays the tour when asked", async () => {
    const store: Record<string, unknown> = { "ui.tour": "done" };
    vi.spyOn(ipc, "prefsGet").mockImplementation(async (k: string) => (store[k] ?? null) as never);
    vi.spyOn(ipc, "prefsSet").mockImplementation(async (k: string, v: unknown) => { store[k] = v; return null as never; });
    render(wrap(<GuideSection />));
    fireEvent.click(await screen.findByRole("button", { name: "Replay the tour" }));
    await waitFor(() => expect(store["ui.tour"]).toBe("pending"));
  });

  it("has a short answer per feature and covers tracking and privacy", async () => {
    expect(new Set(GUIDE.map((g) => g.id)).size).toBe(GUIDE.length);
    expect(GUIDE.map((g) => g.id)).toEqual(expect.arrayContaining(["tracking", "privacy", "data"]));
    expect(CREDIT).toBe("Built by QUORBIT TECHNOLOGIES");
    vi.spyOn(ipc, "prefsGet").mockImplementation(async (k: string) => (k === "ui.role" ? "general" : null) as never);
    render(wrap(<GuideSection />));
    expect(await screen.findByText("Add a task")).toBeTruthy();
    expect(screen.getByText("App and website time")).toBeTruthy();
    // The study pack is gone, so its guide entries are too.
    expect(screen.queryByText("Courses and exams")).toBeNull();
    expect(screen.queryByText("Weekly report")).toBeNull();
  });
});
