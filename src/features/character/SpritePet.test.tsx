import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SpritePet } from "./SpritePet";

describe("SpritePet", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => { cleanup(); vi.useRealTimers(); });

  it("plays its row frame by frame", () => {
    render(<SpritePet sheet="data:," row="waving" label="pet" />);
    expect(screen.getByRole("img").getAttribute("data-frame")).toBe("0");
    act(() => { vi.advanceTimersByTime(180); }); // 700 ms over 4 frames
    expect(screen.getByRole("img").getAttribute("data-frame")).toBe("1");
  });

  it("holds the first frame, dimmed, while asleep (a pack has no sleeping row)", () => {
    render(<SpritePet sheet="data:," row="idle" rest label="pet" />);
    const pet = screen.getByRole("img");
    act(() => { vi.advanceTimersByTime(10_000); });
    expect(pet.getAttribute("data-frame")).toBe("0");
    expect(pet.getAttribute("data-rest")).toBe("true");
    expect(pet.style.filter).toContain("brightness");
  });

  it("moves again once it wakes", () => {
    const { rerender } = render(<SpritePet sheet="data:," row="idle" rest label="pet" />);
    rerender(<SpritePet sheet="data:," row="idle" label="pet" />);
    act(() => { vi.advanceTimersByTime(1000); });
    expect(screen.getByRole("img").getAttribute("data-frame")).not.toBe("0");
    expect(screen.getByRole("img").getAttribute("data-rest")).toBeNull();
  });
});
