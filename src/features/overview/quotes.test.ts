import { describe, expect, it } from "vitest";
import { BUILT_IN_QUOTES, parseQuotes, pool, quoteFor } from "./quotes";

describe("quote of the day", () => {
  it("is on with the built-in quotes by default, and junk is cleaned", () => {
    expect(parseQuotes(null)).toEqual({ show: true, builtIns: true, custom: [] });
    const s = parseQuotes({ custom: [{ text: "  Mine  ", author: " Me " }, { text: "" }, 5, { author: "no text" }] });
    expect(s.custom).toEqual([{ text: "Mine", author: "Me" }]);
  });

  it("keeps one quote all day and changes it the next day", () => {
    const s = parseQuotes(null);
    const morning = quoteFor(s, new Date(2026, 9, 25, 7));
    expect(quoteFor(s, new Date(2026, 9, 25, 22))).toEqual(morning);
    expect(quoteFor(s, new Date(2026, 9, 26, 7))).not.toEqual(morning);
  });

  it("uses your own quotes first, can leave the built-in ones out, and can be turned off", () => {
    const mine = { text: "Ship small things.", author: "" };
    expect(pool({ show: true, builtIns: true, custom: [mine] })[0]).toEqual(mine);
    expect(pool({ show: true, builtIns: true, custom: [] })).toHaveLength(BUILT_IN_QUOTES.length);
    expect(quoteFor({ show: true, builtIns: false, custom: [mine] }, new Date())).toEqual(mine);
    expect(quoteFor({ show: true, builtIns: false, custom: [] }, new Date())).toBeNull();
    expect(quoteFor({ show: false, builtIns: true, custom: [mine] }, new Date())).toBeNull();
  });

  it("built-in quotes are short (two or three lines at most)", () => {
    for (const q of BUILT_IN_QUOTES) expect(q.text.length, q.text).toBeLessThanOrEqual(100);
  });
});
