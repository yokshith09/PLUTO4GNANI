import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { PAGE, ShowMore, useProgressive } from "./progressive";

afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

function List({ n }: { n: number }) {
  const items = Array.from({ length: n }, (_, i) => i);
  const page = useProgressive(items);
  return (
    <div>
      <ul>{page.visible.map((i) => <li key={i}>row {i}</li>)}</ul>
      <ShowMore more={page.more} onMore={page.showMore} noun="rows" />
    </div>
  );
}

describe("long lists are drawn in steps", () => {
  it("a short list is drawn whole, with no button", () => {
    render(<List n={PAGE} />);
    expect(screen.getAllByRole("listitem")).toHaveLength(PAGE);
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("a long list starts with one step and the button adds the next, until all are shown", () => {
    render(<List n={250} />);
    expect(screen.getAllByRole("listitem")).toHaveLength(100);
    fireEvent.click(screen.getByRole("button", { name: "Show more rows (150 more)" }));
    expect(screen.getAllByRole("listitem")).toHaveLength(200);
    fireEvent.click(screen.getByRole("button", { name: "Show more rows (50 more)" }));
    expect(screen.getAllByRole("listitem")).toHaveLength(250);
    expect(screen.queryByRole("button")).toBeNull();
    expect(screen.getByText("row 249")).toBeTruthy();
  });

  it("scrolling near the end adds the next step by itself, and keeps going while the end stays near", () => {
    const watchers: Array<(entries: Array<{ isIntersecting: boolean }>) => void> = [];
    vi.stubGlobal("IntersectionObserver", class {
      constructor(cb: (entries: Array<{ isIntersecting: boolean }>) => void) { watchers.push(cb); }
      observe() { /* the test fires it */ }
      disconnect() { /* nothing to release */ }
    });
    render(<List n={250} />);
    expect(screen.getAllByRole("listitem")).toHaveLength(100);
    act(() => watchers[watchers.length - 1]?.([{ isIntersecting: false }]));
    expect(screen.getAllByRole("listitem")).toHaveLength(100);
    act(() => watchers[watchers.length - 1]?.([{ isIntersecting: true }]));
    expect(screen.getAllByRole("listitem")).toHaveLength(200);
    act(() => watchers[watchers.length - 1]?.([{ isIntersecting: true }]));
    expect(screen.getAllByRole("listitem")).toHaveLength(250);
    expect(screen.queryByRole("button")).toBeNull();
  });
});
