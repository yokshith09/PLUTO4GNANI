import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { StackedBars } from "./StackedBars";

afterEach(cleanup);

describe("StackedBars", () => {
  it("splits a day by category on one shared scale", () => {
    const { container } = render(
      <StackedBars ariaLabel="per day" bars={[
        { key: "a", label: "M", title: "Mon", parts: [{ name: "Coding", seconds: 3000, cls: "bg-sky-400" }, { name: "Notes", seconds: 1000, cls: "bg-violet-400" }] },
        { key: "b", label: "T", title: "Tue", parts: [{ name: "Coding", seconds: 2000, cls: "bg-sky-400" }] },
        { key: "c", label: "W", title: "Wed", parts: [] },
      ]} />,
    );
    const heights = (key: string) => [...container.querySelectorAll(`[title="${key}"] [data-part]`)].map((e) => (e as HTMLElement).style.height);
    expect(heights("Mon")).toEqual(["75%", "25%"]); // shares of that day's total
    expect(heights("Tue")).toEqual(["100%"]);
    const col = (t: string) => (container.querySelector(`[title="${t}"] > div > div`) as HTMLElement).style.height;
    expect(col("Mon")).toBe("100%");
    expect(col("Tue")).toBe("50%"); // half the busiest day
    expect(col("Wed")).toBe("2%"); // an empty day is a thin line, not a gap
  });
});
