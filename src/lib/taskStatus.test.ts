import { describe, expect, it } from "vitest";
import { STATUSES, transitionsFrom } from "./taskStatus";

// PRD §6.2.1, same layout as the table in pluto-core: rows = from, columns = to, in STATUSES order.
const TABLE: Record<string, string> = {
  PLANNED: ". x . x x",
  IN_PROGRESS: ". . x x x",
  PENDING: ". x . x x",
  COMPLETED: "x . . . .",
  CANCELLED: "x . . . .",
};

describe("task status moves", () => {
  it("match the PRD table in every one of the 25 cells", () => {
    for (const from of STATUSES) {
      const cells = (TABLE[from] ?? "").split(" ");
      const offered = transitionsFrom(from).map((t) => t.to);
      STATUSES.forEach((to, i) => expect(offered.includes(to), `${from} -> ${to}`).toBe(cells[i] === "x"));
    }
  });
  it("uses the same button names as the core", () => {
    expect(transitionsFrom("PLANNED").map((t) => t.label)).toEqual(["Start", "Complete", "Cancel"]);
    expect(transitionsFrom("IN_PROGRESS").map((t) => t.label)).toEqual(["Pause", "Complete", "Cancel"]);
    expect(transitionsFrom("PENDING").map((t) => t.label)).toEqual(["Resume", "Complete", "Cancel"]);
    expect(transitionsFrom("COMPLETED").map((t) => t.label)).toEqual(["Reopen"]);
  });
});
