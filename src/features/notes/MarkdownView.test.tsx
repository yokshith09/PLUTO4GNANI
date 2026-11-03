import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MarkdownView } from "./MarkdownView";

describe("checklist boxes in a note", () => {
  afterEach(cleanup);

  it("are clickable when the note can be edited, and report their source line", async () => {
    const onToggle = vi.fn();
    render(<MarkdownView source={"intro\n\n- [ ] milk\n- [x] eggs"} onToggle={onToggle} />);
    const boxes = screen.getAllByRole("checkbox") as HTMLInputElement[];
    expect(boxes).toHaveLength(2);
    const [milk, eggs] = boxes as [HTMLInputElement, HTMLInputElement];
    expect(milk.checked).toBe(false);
    expect(eggs.checked).toBe(true);
    await userEvent.click(milk);
    expect(onToggle).toHaveBeenCalledWith(3);
    await userEvent.click(eggs);
    expect(onToggle).toHaveBeenLastCalledWith(4);
  });

  it("stay read-only without a handler", () => {
    render(<MarkdownView source={"- [ ] milk"} />);
    expect((screen.getByRole("checkbox") as HTMLInputElement).disabled).toBe(true);
  });
});
