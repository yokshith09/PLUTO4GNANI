import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { CHARACTERS, CHARACTER_POSES } from "./Characters";

describe("characters", () => {
  it("has the approved characters", () => {
    expect(CHARACTERS.map((c) => c.name)).toEqual(["Mochi", "Pip", "Bolt", "Ghosty", "Mira", "Raja"]);
  });

  it.each(CHARACTERS.flatMap((c) => CHARACTER_POSES.map((p) => [c.name, p, c.Component] as const)))(
    "%s draws the %s pose",
    (_name, pose, Component) => {
      const { container } = render(<Component pose={pose} animated={false} />);
      expect(container.querySelector("svg")).not.toBeNull();
    },
  );
});
