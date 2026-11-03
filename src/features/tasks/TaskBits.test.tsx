import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ipc } from "@/ipc";
import { LinkChips, PlanBadge, StarButton, linkLabel } from "./TaskBits";
import { planOf, planStateOf, samePlan } from "./TaskPlanEditor";

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

const link = (id: string, url: string, title = "") => ({ id, url, title });

describe("links next to a task (FR-106)", () => {
  it("shows a name, else a tidy address", () => {
    expect(linkLabel(link("a", "https://www.github.com/x/y/"))).toBe("github.com/x/y");
    expect(linkLabel(link("a", "mailto:sam@example.org"))).toBe("sam@example.org");
    expect(linkLabel(link("a", "https://a.com", "Spec"))).toBe("Spec");
  });

  it("opens the page on click and folds extra links into +N", () => {
    const open = vi.spyOn(ipc, "openUrl").mockResolvedValue(null);
    render(<LinkChips links={[link("1", "https://a.com", "Spec"), link("2", "https://b.com"), link("3", "https://c.com"), link("4", "https://d.com")]} max={2} />);
    fireEvent.click(screen.getByRole("button", { name: "Open link Spec" }));
    expect(open).toHaveBeenCalledWith("https://a.com");
    expect(screen.queryByRole("button", { name: "Open link c.com" })).toBeNull();
    expect(screen.getByText("+2")).toBeTruthy();
  });

  it("shows nothing for a task without links", () => {
    const { container } = render(<LinkChips links={[]} />);
    expect(container.textContent).toBe("");
  });
});

describe("star (FR-107)", () => {
  it("says whether it is on and toggles", () => {
    const toggle = vi.fn();
    render(<StarButton starred={false} title="Essay" onToggle={toggle} />);
    const b = screen.getByRole("button", { name: "Star “Essay”" });
    expect(b.getAttribute("aria-pressed")).toBe("false");
    fireEvent.click(b);
    expect(toggle).toHaveBeenCalledOnce();
    cleanup();
    render(<StarButton starred title="Essay" onToggle={toggle} />);
    expect(screen.getByRole("button", { name: "Unstar “Essay”" }).getAttribute("aria-pressed")).toBe("true");
  });
});

describe("plan badge and editor state (FR-108, FR-109)", () => {
  it("labels repeating and multi-day tasks", () => {
    render(<PlanBadge task={{ repeat: { kind: "weekly", days: [1, 3, 5] }, days: [] }} />);
    expect(screen.getByText("Mon, Wed, Fri")).toBeTruthy();
    cleanup();
    render(<PlanBadge task={{ repeat: null, days: ["2026-10-27", "2026-10-29"] }} />);
    expect(screen.getByText("2 days")).toBeTruthy();
  });

  it("turns the editor's state into what the core takes, and back", () => {
    const task = { days: [], repeat: null, repeat_until: null, planned_date: "2026-10-23" };
    const s = planStateOf(task);
    expect(s.mode).toBe("one");
    expect(s.repeat).toEqual({ kind: "weekly", days: [5] }); // ready with the planned weekday (Friday)
    expect(planOf(s)).toEqual({ days: [], repeat: null, repeat_until: null });
    expect(planOf({ ...s, mode: "repeat", until: "2026-12-01" })).toEqual({ days: [], repeat: { kind: "weekly", days: [5] }, repeat_until: "2026-12-01" });
    expect(planOf({ ...s, mode: "days", days: ["2026-10-27"] })).toEqual({ days: ["2026-10-27"], repeat: null, repeat_until: null });
    expect(samePlan(planOf(s), planOf(planStateOf(task)))).toBe(true);
  });
});
