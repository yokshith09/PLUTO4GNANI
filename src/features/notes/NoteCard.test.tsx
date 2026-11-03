import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { NoteSummary } from "@/ipc";
import { NoteCard, NoteRow, PinnedCard } from "./NoteCard";

const note: NoteSummary = { id: "n1", title: "Plan", excerpt: "Read chapter 3", color: "default", pinned: false, archived: false, created_at: 0, edited_at: 0, labels: [], project: null };
const handlers = () => ({ onOpen: vi.fn(), onPin: vi.fn(), onArchive: vi.fn(), onDelete: vi.fn() });

describe("note card delete (owner 2026-10-27: the confirm shook and vanished)", () => {
  afterEach(cleanup);

  // The toolbar only shows on hover; the confirm box opens outside the card, so the toolbar has to
  // stay while it is open or the box loses the button it hangs from and jumps to the corner.
  it.each([["card", NoteCard], ["row", NoteRow], ["pinned", PinnedCard]] as const)("the %s keeps its toolbar while the confirm is open", (_, View) => {
    const h = handlers();
    render(<QueryClientProvider client={new QueryClient()}><View note={note} {...h} /></QueryClientProvider>);
    const del = screen.getByRole("button", { name: "Delete" });
    const toolbar = del.closest("div.hidden") as HTMLElement;
    expect(toolbar.className).toContain("has-[[data-state=open]]:flex");
    fireEvent.click(del);
    expect(del.getAttribute("data-state")).toBe("open");
    fireEvent.click(screen.getByRole("alertdialog").querySelector("button:last-child") as HTMLElement);
    expect(h.onDelete).toHaveBeenCalledWith("n1");
    expect(h.onOpen).not.toHaveBeenCalled();
  });
});
