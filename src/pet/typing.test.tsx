import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { LEAVE_MS, MAX_WAIT_MS, STOW_MS, WAIT_MS, useTypingAlong } from "./typing";

/** A fake shell: `send(true|false)` is the typing signal. */
function fakeShell() {
  let cb: ((t: boolean) => void) | null = null;
  const subscribe = (f: (t: boolean) => void) => {
    cb = f;
    return () => { cb = null; };
  };
  return { subscribe, send: (t: boolean) => act(() => cb?.(t)), listening: () => cb !== null };
}

describe("useTypingAlong", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("types, then waits looking at you, then puts the laptop away", () => {
    const shell = fakeShell();
    const { result } = renderHook(() => useTypingAlong(true, shell.subscribe));
    expect(result.current).toBe("off");
    shell.send(true);
    expect(result.current).toBe("type");
    shell.send(false);
    expect(result.current).toBe("wait");
    act(() => vi.advanceTimersByTime(WAIT_MS - 1));
    expect(result.current).toBe("wait");
    act(() => vi.advanceTimersByTime(1));
    expect(result.current).toBe("stow");
    act(() => vi.advanceTimersByTime(STOW_MS));
    expect(result.current).toBe("off");
  });

  it("goes straight back to typing if you carry on while it waits", () => {
    const shell = fakeShell();
    const { result } = renderHook(() => useTypingAlong(true, shell.subscribe));
    shell.send(true);
    shell.send(false);
    act(() => vi.advanceTimersByTime(5_000));
    shell.send(true);
    expect(result.current).toBe("type");
    act(() => vi.advanceTimersByTime(WAIT_MS * 2));
    expect(result.current).toBe("type");
  });

  it("does nothing while the setting is off", () => {
    const shell = fakeShell();
    const { result } = renderHook(() => useTypingAlong(false, shell.subscribe));
    expect(shell.listening()).toBe(false);
    expect(result.current).toBe("off");
  });

  it("a stop without a start is ignored", () => {
    const shell = fakeShell();
    const { result } = renderHook(() => useTypingAlong(true, shell.subscribe));
    shell.send(false);
    expect(result.current).toBe("off");
  });

  it("keeps waiting while a text box is focused and puts the laptop away soon after you leave it", () => {
    const keys = fakeShell();
    const box = fakeShell();
    const { result } = renderHook(() => useTypingAlong(true, keys.subscribe, box.subscribe));
    box.send(true);
    keys.send(true);
    keys.send(false);
    act(() => vi.advanceTimersByTime(WAIT_MS * 3));
    expect(result.current).toBe("wait");
    box.send(false);
    act(() => vi.advanceTimersByTime(LEAVE_MS - 1));
    expect(result.current).toBe("wait");
    act(() => vi.advanceTimersByTime(1));
    expect(result.current).toBe("stow");
    act(() => vi.advanceTimersByTime(STOW_MS));
    expect(result.current).toBe("off");
  });

  it("outside a text box it puts the laptop away quickly, and never waits forever inside one", () => {
    const keys = fakeShell();
    const box = fakeShell();
    const { result } = renderHook(() => useTypingAlong(true, keys.subscribe, box.subscribe));
    box.send(false);
    keys.send(true);
    keys.send(false);
    act(() => vi.advanceTimersByTime(LEAVE_MS));
    expect(result.current).toBe("stow");

    box.send(true);
    keys.send(true);
    keys.send(false);
    act(() => vi.advanceTimersByTime(MAX_WAIT_MS));
    expect(result.current).toBe("stow");
  });
});
