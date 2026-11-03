import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Task } from "@/ipc";

const removeAny = vi.fn(() => Promise.resolve());
vi.mock("@/hooks/usePluto", () => ({ useTaskActions: () => ({ removeAny }) }));

import { DeleteTaskButton } from "./DeleteTaskButton";

const task = { id: "t1", title: "Pay rent" } as Task;

afterEach(() => { cleanup(); removeAny.mockClear(); });

describe("task row delete (owner 2026-10-28: every delete asks first)", () => {
  it("asks before deleting and does nothing on Cancel", async () => {
    const user = userEvent.setup();
    render(<DeleteTaskButton task={task} />);
    await user.click(screen.getByRole("button", { name: "Delete Pay rent" }));
    const ask = await screen.findByRole("alertdialog");
    expect(removeAny).not.toHaveBeenCalled();
    await user.click(within(ask).getByRole("button", { name: "Cancel" }));
    expect(removeAny).not.toHaveBeenCalled();
  });

  it("deletes only after the confirming click", async () => {
    const user = userEvent.setup();
    render(<DeleteTaskButton task={task} />);
    await user.click(screen.getByRole("button", { name: "Delete Pay rent" }));
    await user.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(removeAny).toHaveBeenCalledWith(task));
  });
});
