import { describe, expect, it } from "vitest";
import { formatDuration, formatWhen, fromLocalInput, greeting, timeAgo, toLocalInput } from "./time";

describe("timeAgo", () => {
  const now = 1_000_000_000_000;
  it("rounds to the friendliest unit", () => {
    expect(timeAgo(now - 10_000, now)).toBe("just now");
    expect(timeAgo(now - 5 * 60_000, now)).toBe("5 min ago");
    expect(timeAgo(now - 3 * 3_600_000, now)).toBe("3 h ago");
    expect(timeAgo(now - 24 * 3_600_000, now)).toBe("yesterday");
    expect(timeAgo(now - 72 * 3_600_000, now)).toBe("3 days ago");
  });
});

describe("greeting", () => {
  it("follows the clock", () => {
    expect(greeting(3)).toBe("Still up?");
    expect(greeting(9)).toBe("Good morning");
    expect(greeting(14)).toBe("Good afternoon");
    expect(greeting(20)).toBe("Good evening");
  });
});

describe("formatWhen", () => {
  const base = new Date(2026, 9, 4, 12, 0).getTime();
  it("names today and tomorrow", () => {
    expect(formatWhen(new Date(2026, 9, 4, 15, 30).getTime(), base)).toMatch(/^Today /);
    expect(formatWhen(new Date(2026, 9, 5, 9, 0).getTime(), base)).toMatch(/^Tomorrow /);
    expect(formatWhen(new Date(2026, 9, 3, 9, 0).getTime(), base)).toMatch(/^Yesterday /);
    expect(formatWhen(new Date(2026, 9, 9, 9, 0).getTime(), base)).toContain("·");
  });
  it("round-trips the datetime-local value", () => {
    const t = new Date(2026, 9, 4, 15, 30).getTime();
    expect(fromLocalInput(toLocalInput(t))).toBe(t);
  });
});

describe("formatDuration", () => {
  it("is short and human", () => {
    expect(formatDuration(35)).toBe("35s");
    expect(formatDuration(2100)).toBe("35m");
    expect(formatDuration(3600)).toBe("1h");
    expect(formatDuration(35220)).toBe("9h 47m");
  });
});
