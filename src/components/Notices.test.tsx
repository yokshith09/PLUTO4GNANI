import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { notify, setPetShown } from "@/lib/notify";
import { Notices } from "./Notices";

afterEach(() => { cleanup(); setPetShown(true); vi.useRealTimers(); });

describe("messages in the dashboard while the pet is hidden", () => {
  it("go to the pet when it's shown, and to the dashboard when it's hidden, with their action", () => {
    vi.useFakeTimers();
    const said: unknown[] = [];
    const listen = (e: Event) => said.push((e as CustomEvent).detail);
    window.addEventListener("pluto-pet-say", listen);
    render(<Notices />);
    act(() => notify.success("Task saved"));
    expect(said).toHaveLength(1);
    expect(screen.queryByText("Task saved")).toBeNull();

    setPetShown(false);
    const undo = vi.fn();
    act(() => notify.info("Moved to Bin", { action: { label: "Undo", onClick: undo } }));
    expect(said).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: "Undo" }));
    expect(undo).toHaveBeenCalledOnce();
    expect(screen.queryByText("Moved to Bin")).toBeNull();

    act(() => { for (const t of ["a", "b", "c", "d"]) notify.info(t); });
    expect(screen.getAllByRole("status")).toHaveLength(3);
    act(() => { vi.advanceTimersByTime(6000); });
    expect(screen.queryAllByRole("status")).toHaveLength(0);
    window.removeEventListener("pluto-pet-say", listen);
  });
});
