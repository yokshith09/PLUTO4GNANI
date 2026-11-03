import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ConfirmAction } from "./ConfirmAction";

afterEach(cleanup);

describe("ConfirmAction", () => {
  const setup = () => {
    const onConfirm = vi.fn();
    render(<ConfirmAction message="Delete this reminder?" onConfirm={onConfirm}><button type="button">Trash</button></ConfirmAction>);
    return { onConfirm, user: userEvent.setup() };
  };

  it("does nothing until it is confirmed", async () => {
    const { onConfirm, user } = setup();
    await user.click(screen.getByRole("button", { name: "Trash" }));
    expect(await screen.findByRole("alertdialog", { name: "Delete this reminder?" })).toBeTruthy();
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it("puts the focus on Cancel, and Cancel and Escape keep everything", async () => {
    const { onConfirm, user } = setup();
    await user.click(screen.getByRole("button", { name: "Trash" }));
    const box = await screen.findByRole("alertdialog");
    expect(document.activeElement).toBe(within(box).getByRole("button", { name: "Cancel" }));
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("alertdialog")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Trash" }));
    await user.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Cancel" }));
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it("runs the action once when confirmed, and closes", async () => {
    const { onConfirm, user } = setup();
    await user.click(screen.getByRole("button", { name: "Trash" }));
    await user.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Delete" }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("does not click whatever contains the button (a card that opens when clicked)", async () => {
    const open = vi.fn();
    const onConfirm = vi.fn();
    render(
      <div role="button" tabIndex={0} aria-label="card" onClick={open}>
        <ConfirmAction message="Delete this note?" onConfirm={onConfirm}><button type="button" onClick={(e) => e.stopPropagation()}>Trash</button></ConfirmAction>
      </div>,
    );
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Trash" }));
    await user.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Cancel" }));
    await user.click(screen.getByRole("button", { name: "Trash" }));
    await user.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Delete" }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
    expect(open).not.toHaveBeenCalled();
  });
});
