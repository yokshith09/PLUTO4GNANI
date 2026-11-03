import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { BEAR_POSES, Bear } from "./Bear";
import { TURNED_POSES } from "./BearViews";

describe("Bear views", () => {
  it("renders every pose, including the turned views", () => {
    for (const pose of BEAR_POSES) {
      const { container, unmount } = render(<Bear pose={pose} />);
      expect(container.querySelector("svg")).not.toBeNull();
      unmount();
    }
  });

  it("the back view has no face; three-quarter and look-back do", () => {
    const face = (pose: (typeof TURNED_POSES)[number]) => {
      const { container, unmount } = render(<Bear pose={pose} />);
      const has = container.querySelector('[data-part="face"]') !== null;
      const back = container.querySelector('[data-part="back-of-head"]') !== null;
      unmount();
      return { has, back };
    };
    expect(face("back")).toEqual({ has: false, back: true });
    expect(face("turn")).toEqual({ has: true, back: false });
    expect(face("back_look")).toEqual({ has: true, back: false });
  });

  it("only the look-back view shakes, and only when animated", () => {
    const shakes = (pose: (typeof TURNED_POSES)[number], animated: boolean) => {
      const { container, unmount } = render(<Bear pose={pose} animated={animated} />);
      const n = container.querySelectorAll(".bear-butt-shake").length;
      unmount();
      return n;
    };
    expect(shakes("back_look", true)).toBe(1);
    expect(shakes("back_look", false)).toBe(0);
    expect(shakes("back", true)).toBe(0);
    expect(shakes("turn", true)).toBe(0);
  });

  it("switching between front and turned poses keeps rendering", () => {
    const { container, rerender } = render(<Bear pose="idle" />);
    rerender(<Bear pose="back" />);
    rerender(<Bear pose="wave" />);
    expect(container.querySelector("svg")).not.toBeNull();
  });
});
