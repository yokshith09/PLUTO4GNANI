import { describe, expect, it } from "vitest";
import { normalizeUrl, parseLinks } from "./links";

describe("normalizeUrl", () => {
  it("adds https and derives a label", () => {
    expect(normalizeUrl("github.com/pluto")).toEqual({ url: "https://github.com/pluto", label: "github.com" });
    expect(normalizeUrl("https://www.figma.com/file/1")?.label).toBe("figma.com");
  });
  it("refuses anything that is not a web or mail link", () => {
    for (const bad of ["javascript:alert(1)", "file:///etc/passwd", "data:text/html,hi", "localhost", "two words", ""]) {
      expect(normalizeUrl(bad)).toBeNull();
    }
    expect(normalizeUrl("mailto:me@example.com")?.label).toBe("me@example.com");
  });
});

describe("parseLinks", () => {
  it("splits a pasted block and drops duplicates and junk", () => {
    const found = parseLinks("https://a.com\nb.org  https://a.com nonsense");
    expect(found.map((f) => f.label)).toEqual(["a.com", "b.org"]);
  });
});
