import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import axe from "axe-core";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Slider } from "@/components/ui/slider";
import { WeekCalendar } from "@/features/calendar/WeekCalendar";
import { NoteCard, NoteRow, PinnedCard } from "@/features/notes/NoteCard";
import type { NoteSummary } from "@/ipc";

/**
 * Structure checks from axe-core on the pieces an automated scan found wrong in 0.5.2 (a list
 * holding buttons, a grid without rows, an article given the button role, a slider with no name).
 * jsdom has no layout, so contrast and size rules are off here; they were scanned in a real
 * browser. This guards the roles, names and parents from drifting back.
 */
async function violations(container: HTMLElement) {
  const result = await axe.run(container, {
    runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "best-practice"] },
    rules: { "color-contrast": { enabled: false }, region: { enabled: false } },
  });
  return result.violations.map((v) => `${v.id}: ${v.nodes[0]?.html.slice(0, 120)}`);
}

// Radix sliders measure themselves; jsdom has no ResizeObserver.
class NoopObserver { observe() { /* no layout in jsdom */ } unobserve() { /* nothing */ } disconnect() { /* nothing */ } }
vi.stubGlobal("ResizeObserver", NoopObserver);

const note: NoteSummary = { id: "n1", title: "Plan", excerpt: "Read chapter 3", color: "default", pinned: false, archived: false, created_at: 0, edited_at: 0, labels: [], project: null };
const handlers = () => ({ onOpen: vi.fn(), onPin: vi.fn(), onArchive: vi.fn(), onDelete: vi.fn() });
const wrap = (ui: React.ReactElement) => <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>{ui}</QueryClientProvider>;

afterEach(cleanup);

describe("accessibility structure (axe-core)", () => {
  it("a slider has a name on the control a screen reader lands on", async () => {
    const { container } = render(<Slider min={20} max={100} value={[60]} aria-label="Opacity" />);
    expect(screen.getByRole("slider", { name: "Opacity" })).toBeTruthy();
    expect(await violations(container)).toEqual([]);
  });

  it.each([["card", NoteCard], ["row", NoteRow], ["pinned", PinnedCard]] as const)("the note %s is a real button, not an article with a button role", async (_, View) => {
    const { container } = render(wrap(<View note={note} {...handlers()} />));
    expect(container.querySelector("article")).toBeNull();
    expect(screen.getByRole("button", { name: "Plan" })).toBeTruthy();
    expect(await violations(container)).toEqual([]);
  });

  it("the week is a list of seven days", async () => {
    const { container } = render(wrap(<WeekCalendar large onOpenTask={() => undefined} />));
    expect(screen.getByRole("list", { name: "Next 7 days" })).toBeTruthy();
    expect(screen.getAllByRole("listitem").length).toBeGreaterThanOrEqual(7);
    expect(await violations(container)).toEqual([]);
  });
});
