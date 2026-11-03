import { describe, expect, it } from "vitest";
import { dayPresets, timePresets } from "./datePresets";

describe("date and time presets (FR-45)", () => {
  it("knows today, tomorrow, the weekend, next Monday and in 3 days", () => {
    const wed = new Date(2026, 9, 21, 10); // Wednesday
    // "In 3 days" is Saturday too, so it is left out: one button per day.
    expect(dayPresets(wed).map((p) => p.value)).toEqual(["2026-10-21", "2026-10-22", "2026-10-24", "2026-10-26"]);
    // On a Saturday "This weekend" is today: only Today shows (owner 2026-10-25, two buttons were lit).
    const sat = new Date(2026, 9, 24, 10);
    expect(dayPresets(sat).map((p) => p.label)).toEqual(["Today", "Tomorrow", "Next week", "In 3 days"]);
    expect(dayPresets(new Date(2026, 9, 26)).find((p) => p.label === "Next week")?.value).toBe("2026-11-02");
  });

  it("offers this evening only while it's still ahead, and rounds the hour", () => {
    const morning = timePresets(new Date(2026, 9, 21, 10, 7));
    expect(morning.map((p) => p.label)).toEqual(["In 1 hour", "This evening", "Tomorrow 9:00", "Monday 9:00"]);
    expect(morning[0]?.value).toBe("2026-10-21T11:10");
    expect(morning[1]?.value).toBe("2026-10-21T18:00");
    expect(timePresets(new Date(2026, 9, 21, 19)).map((p) => p.label)).not.toContain("This evening");
  });
});
