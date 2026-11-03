import { describe, expect, it } from "vitest";
import { supportedBrowsers } from "./SitesEmpty";

describe("supportedBrowsers", () => {
  it("never promises Firefox on a Mac, where it can't be read", () => {
    expect(supportedBrowsers(true)).not.toContain("Firefox");
    expect(supportedBrowsers(true)).toContain("Vivaldi");
  });
  it("lists Firefox on Windows", () => {
    expect(supportedBrowsers(false)).toContain("Firefox");
  });
});
