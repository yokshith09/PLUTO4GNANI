import { describe, expect, it } from "vitest";
import { createEndpointer } from "./endpoint";

const frame = (amp: number) => {
  const a = new Int16Array(512).fill(amp);
  return a.buffer;
};
const feedAll = (e: ReturnType<typeof createEndpointer>, amp: number, ms: number) => {
  let last: "continue" | "end" = "continue";
  for (let i = 0; i < Math.round(ms / 32); i++) {
    last = e.feed(frame(amp));
    if (last === "end") return { ended: true, at: i };
  }
  return { ended: false, at: -1 };
};

describe("ending a talk turn by itself", () => {
  it("ends a pause after speech, but not a short breath in the middle", () => {
    const e = createEndpointer();
    expect(feedAll(e, 5000, 1500).ended).toBe(false);
    expect(feedAll(e, 20, 800).ended).toBe(false); // a breath between words
    expect(feedAll(e, 5000, 600).ended).toBe(false);
    expect(feedAll(e, 20, 1500).ended).toBe(true);
  });

  it("gives up when nothing is said", () => {
    const e = createEndpointer();
    expect(feedAll(e, 20, 5000).ended).toBe(false);
    expect(feedAll(e, 20, 2000).ended).toBe(true);
  });

  it("stops at the longest command even while talking", () => {
    const e = createEndpointer({ endSilenceMs: 1200, noSpeechMs: 6000, maxMs: 3000 });
    expect(feedAll(e, 5000, 3500).ended).toBe(true);
  });

  it("a steady background hum is not speech", () => {
    const e = createEndpointer();
    // Quiet room first so the floor is low, then speech, then the same hum: still ends.
    feedAll(e, 100, 500);
    feedAll(e, 5000, 800);
    expect(feedAll(e, 100, 1500).ended).toBe(true);
  });
});
