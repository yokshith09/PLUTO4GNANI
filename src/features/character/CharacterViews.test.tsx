import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { TURNED_POSES, TURNED_VIEWS } from "./CharacterViews";

const ids = Object.keys(TURNED_VIEWS) as (keyof typeof TURNED_VIEWS)[];

describe("built-in characters: turned views", () => {
  it("covers every built-in character", () => {
    expect(ids.sort()).toEqual(["bolt", "ghosty", "mira", "mochi", "pip", "raja"]);
  });

  for (const id of ids) {
    const View = TURNED_VIEWS[id];
    const has = (pose: (typeof TURNED_POSES)[number], sel: string, animated = false) => {
      const { container } = render(<View pose={pose} animated={animated} />);
      return container.querySelector(sel) !== null;
    };

    it(`${id}: every turned pose renders an svg`, () => {
      for (const pose of TURNED_POSES) {
        const { container } = render(<View pose={pose} />);
        expect(container.querySelector("svg")).not.toBeNull();
      }
    });

    it(`${id}: the back view shows the back of the head and no face`, () => {
      expect(has("back", '[data-part="back-of-head"]')).toBe(true);
      expect(has("back", '[data-part="face"]')).toBe(false);
    });

    it(`${id}: three-quarter and look-back views show a face`, () => {
      expect(has("turn", '[data-part="face"]')).toBe(true);
      expect(has("back_look", '[data-part="face"]')).toBe(true);
    });

    it(`${id}: only the look-back view shakes, and only when animated`, () => {
      expect(has("back_look", ".bear-butt-shake", true)).toBe(true);
      expect(has("back_look", ".bear-butt-shake", false)).toBe(false);
      expect(has("turn", ".bear-butt-shake", true)).toBe(false);
      expect(has("back", ".bear-butt-shake", true)).toBe(false);
    });
  }
});
