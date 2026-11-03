import { describe, expect, it } from "vitest";
import { hourName, sessionStats } from "./sessionStats";

const at = (day: number, h: number, m = 0) => new Date(2026, 9, day, h, m).getTime();
const span = (a: number, b: number) => ({ started_at: a, ended_at: b });

describe("session statistics", () => {
  it("is empty and calm with no sessions", () => {
    const s = sessionStats([]);
    expect(s).toMatchObject({ total: 0, count: 0, longest: 0, average: 0, peakHour: null, daily: [] });
    expect(s.hourly).toHaveLength(24);
  });
  it("totals, counts, the longest and the average", () => {
    const s = sessionStats([span(at(26, 9), at(26, 10)), span(at(26, 14), at(26, 14, 20))]);
    expect(s.total).toBe(4800);
    expect(s.count).toBe(2);
    expect(s.longest).toBe(3600);
    expect(s.longestStartedAt).toBe(at(26, 9));
    expect(s.average).toBe(2400);
  });
  it("fills every hour a long session covers, and finds the peak", () => {
    const s = sessionStats([span(at(26, 9, 30), at(26, 11, 15))]);
    expect(s.hourly[9]).toBe(1800);
    expect(s.hourly[10]).toBe(3600);
    expect(s.hourly[11]).toBe(900);
    expect(s.peakHour).toBe(10);
    expect(s.hourly.reduce((a, b) => a + b, 0)).toBe(s.total);
  });
  it("adds up per day, earliest first, counting a session on the day it starts", () => {
    const s = sessionStats([span(at(27, 9), at(27, 10)), span(at(26, 23), at(27, 1))]);
    expect(s.daily.map((d) => [new Date(d.day).getDate(), d.seconds, d.sessions])).toEqual([[26, 3600, 1], [27, 7200, 1]]);
  });
  it("names hours the way people say them", () => {
    expect([0, 9, 12, 15, 23].map(hourName)).toEqual(["12 AM", "9 AM", "12 PM", "3 PM", "11 PM"]);
  });
});
