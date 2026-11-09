import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { BACK_TURN_SHAKE, EMOTE_EXIT_MS, EMOTE_STATIC_MS, playEmote } from "./emotes";

describe("playEmote", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("plays the back-turn shake in order, then restores the pose", () => {
    const seen: string[] = [];
    const done = vi.fn();
    playEmote(BACK_TURN_SHAKE, { restore: "idle", set: (p) => seen.push(p), onDone: done });
    vi.advanceTimersByTime(BACK_TURN_SHAKE.reduce((n, s) => n + s.ms, 0));
    expect(seen).toEqual(["turn", "back", "back_look", "back", "turn", "idle"]);
    expect(done).toHaveBeenCalledWith(true);
  });

  it("turns away only through drawn views, never a front pose", () => {
    expect(BACK_TURN_SHAKE.map((s) => s.pose).every((p) => ["turn", "back", "back_look"].includes(p))).toBe(true);
  });

  it("cancel takes a short exit through the three-quarter view, then restores", () => {
    const seen: string[] = [];
    const done = vi.fn();
    const run = playEmote(BACK_TURN_SHAKE, { restore: "work", set: (p) => seen.push(p), onDone: done });
    vi.advanceTimersByTime(700);
    run.cancel();
    expect(seen.at(-1)).toBe("turn");
    vi.advanceTimersByTime(EMOTE_EXIT_MS);
    expect(seen.at(-1)).toBe("work");
    expect(done).toHaveBeenCalledWith(false);
    vi.advanceTimersByTime(10_000);
    expect(seen.filter((p) => p === "work")).toHaveLength(1);
  });

  it("cancel after the end does nothing", () => {
    const set = vi.fn();
    const run = playEmote(BACK_TURN_SHAKE, { restore: "idle", set });
    vi.advanceTimersByTime(10_000);
    const calls = set.mock.calls.length;
    run.cancel();
    vi.advanceTimersByTime(1000);
    expect(set.mock.calls.length).toBe(calls);
  });

  it("reduced motion shows one still pose and never the shaking view", () => {
    const seen: string[] = [];
    playEmote(BACK_TURN_SHAKE, { restore: "idle", set: (p) => seen.push(p), reducedMotion: true });
    vi.advanceTimersByTime(EMOTE_STATIC_MS);
    expect(seen).toEqual(["turn", "idle"]);
  });

  it("reduced motion cancel returns at once", () => {
    const seen: string[] = [];
    const run = playEmote(BACK_TURN_SHAKE, { restore: "idle", set: (p) => seen.push(p), reducedMotion: true });
    run.cancel();
    expect(seen).toEqual(["turn", "idle"]);
  });
});
