import { describe, expect, it } from "vitest";
import { LEADS, leadLabel, nextLead } from "./TaskReminders";

describe("how early a reminder comes (owner 2026-10-25)", () => {
  it("steps up with + and down with −, within at-the-time and a day before", () => {
    expect(nextLead(5, 1)).toBe(10);
    expect(nextLead(5, -1)).toBe(0);
    expect(nextLead(0, -1)).toBe(0);
    expect(nextLead(1440, 1)).toBe(1440);
    // A value between steps goes to the next step either way.
    expect(nextLead(7, 1)).toBe(10);
    expect(nextLead(7, -1)).toBe(5);
    expect(LEADS[0]).toBe(0);
  });

  it("reads plainly", () => {
    expect(leadLabel(0)).toBe("At the time");
    expect(leadLabel(5)).toBe("5 min before");
    expect(leadLabel(60)).toBe("1 h before");
    expect(leadLabel(90)).toBe("1 h 30 min before");
    expect(leadLabel(1440)).toBe("1 day before");
  });
});
