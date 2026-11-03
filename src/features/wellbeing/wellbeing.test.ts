import { describe, expect, it } from "vitest";
import { emptyNudges } from "@/lib/nudges";
import { dueIn, emptyWellbeing, isDue, markShown, nextCheckMs, parseWellbeing, SAYS, type Wellbeing } from "./wellbeing";

const at = (h: number, m = 0) => new Date(2026, 9, 28, h, m, 0);

describe("break and water nudges", () => {
  it("are off until they are turned on, with the owner's gaps ready", () => {
    const w = emptyWellbeing();
    expect(w.break.on).toBe(false);
    expect(w.water.on).toBe(false);
    expect(w.break.everyMin).toBe(50);
    expect(w.water.everyMin).toBe(90);
    expect(dueIn(w.break, Date.now(), null)).toBeNull();
  });

  it("never nudge the moment Pluto opens: the gap starts now", () => {
    const w = { ...emptyWellbeing().break, on: true };
    expect(dueIn(w, Date.now(), null)).toBe(50 * 60_000);
  });

  it("count the clock when app tracking is off", () => {
    const now = at(14).getTime();
    const w = { on: true, everyMin: 50, lastAt: now - 20 * 60_000, lastScreenSec: null };
    expect(dueIn(w, now, null)).toBe(30 * 60_000);
    expect(dueIn({ ...w, lastAt: now - 60 * 60_000 }, now, null)).toBe(0);
  });

  it("count screen time when tracking is on, so time away does not count", () => {
    const now = at(14).getTime();
    // An hour on the clock but only ten minutes at the screen: not due yet.
    const w = { on: true, everyMin: 50, lastAt: now - 60 * 60_000, lastScreenSec: 3_000 };
    expect(dueIn(w, now, 3_600)).toBe(40 * 60_000);
    expect(dueIn(w, now, 6_000)).toBe(0);
  });

  it("start again at midnight, when today's screen time goes back to nothing", () => {
    const now = at(9).getTime();
    const w = { on: true, everyMin: 50, lastAt: now - 10 * 60 * 60_000, lastScreenSec: 20_000 };
    expect(dueIn(w, now, 600)).toBe(50 * 60_000 - 600_000);
  });

  it("wait for quiet hours to end", () => {
    const nudges = emptyNudges(); // quiet 22:00–08:00
    const due = (clock: Date) => isDue({ on: true, everyMin: 50, lastAt: clock.getTime() - 60 * 60_000, lastScreenSec: null }, nudges, clock, null);
    expect(due(at(23))).toBe(false); // due, but the house is asleep
    expect(due(at(14))).toBe(true);
  });

  it("do not use up the daily nudge budget: they repeat all day", () => {
    const now = at(10).getTime();
    let w: Wellbeing = { break: { on: true, everyMin: 50, lastAt: now, lastScreenSec: null }, water: emptyWellbeing().water };
    for (let i = 1; i <= 5; i++) {
      const later = now + i * 50 * 60_000;
      expect(dueIn(w.break, later, null)).toBe(0);
      w = markShown(w, "break", later, null);
    }
    expect(w.break.lastAt).toBe(now + 5 * 50 * 60_000);
  });

  it("look again when the soonest one is due, and never more than once a minute", () => {
    const now = at(10).getTime();
    const w = {
      break: { on: true, everyMin: 50, lastAt: now - 49 * 60_000, lastScreenSec: null },
      water: { on: true, everyMin: 90, lastAt: now, lastScreenSec: null },
    };
    expect(nextCheckMs(w, now, null)).toBe(60_000);
    expect(nextCheckMs(emptyWellbeing(), now, null)).toBeNull();
    const soon = { ...w, break: { ...w.break, lastAt: now - 50 * 60_000 } };
    expect(nextCheckMs(soon, now, null)).toBe(60_000);
  });

  it("reads old or broken settings without losing the defaults", () => {
    expect(parseWellbeing(null)).toEqual(emptyWellbeing());
    expect(parseWellbeing("nonsense")).toEqual(emptyWellbeing());
    const w = parseWellbeing({ break: { on: true, everyMin: 9999 }, water: { on: true, everyMin: 1 } });
    expect(w.break.everyMin).toBe(240);
    expect(w.water.everyMin).toBe(10);
    expect(w.break.on).toBe(true);
  });

  it("says something kind, with a pose for each", () => {
    expect(SAYS.break.text).toMatch(/stretch/i);
    expect(SAYS.water.text).toMatch(/water/i);
    expect(SAYS.break.pose).not.toBe(SAYS.water.pose);
  });
});
