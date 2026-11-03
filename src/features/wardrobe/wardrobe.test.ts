import { describe, expect, it } from "vitest";
import { emptyCompanion } from "@/features/companion/companion";
import { ITEMS, OUTFITS, normalize, parseWardrobe, toggleItem, toggleOutfit, wornNow, emptyWardrobe } from "./wardrobe";

describe("wardrobe", () => {
  it("every outfit is made of known items, one per slot", () => {
    for (const o of OUTFITS) {
      expect(normalize(o.items)).toHaveLength(o.items.length);
      for (const id of o.items) expect(ITEMS.some((i) => i.id === id)).toBe(true);
    }
  });

  it("wears one thing per slot: a new hat replaces the old one", () => {
    let w = toggleItem(emptyWardrobe, "beanie");
    w = toggleItem(w, "scarf");
    w = toggleItem(w, "crown");
    expect(w.worn).toEqual(["crown", "scarf"]);
    expect(toggleItem(w, "crown").worn).toEqual(["scarf"]);
  });

  it("an outfit puts its things on, replaces the look, and comes off on a second press", () => {
    let w = toggleItem(emptyWardrobe, "sunglasses");
    w = toggleOutfit(w, "cosy");
    expect(w).toMatchObject({ worn: ["beanie", "scarf"], outfit: "cosy" });
    expect(toggleItem(w, "beanie").outfit).toBe("");
    expect(toggleOutfit(w, "cosy")).toMatchObject({ worn: [], outfit: "" });
    expect(toggleOutfit(w, "nope")).toBe(w);
  });

  it("completing an outfit by hand counts as that outfit", () => {
    const w = toggleItem(toggleItem(emptyWardrobe, "glasses"), "bow-tie");
    expect(w.outfit).toBe("smart");
  });

  it("reads bad saved data without throwing", () => {
    expect(parseWardrobe(null)).toEqual(emptyWardrobe);
    expect(parseWardrobe({ worn: ["cap", "cap", "mystery", 3, "beanie"], outfit: "cool", partyOnBirthday: false }))
      .toEqual({ worn: ["beanie"], outfit: "", partyOnBirthday: false });
    expect(parseWardrobe({ worn: ["cap", "sunglasses"], outfit: "cool" }).outfit).toBe("cool");
  });

  it("wears the party hat on a birthday, not on other days, and only if allowed", () => {
    const c = { ...emptyCompanion("2025-03-04"), userBirthday: "10-26" };
    const w = { ...emptyWardrobe, worn: ["crown" as const, "scarf" as const] };
    expect(wornNow(w, c, new Date("2026-10-26T10:00:00"))).toEqual(["party-hat", "scarf"]);
    expect(wornNow(w, c, new Date("2026-10-27T10:00:00"))).toEqual(["crown", "scarf"]);
    expect(wornNow({ ...w, partyOnBirthday: false }, c, new Date("2026-10-26T10:00:00"))).toEqual(["crown", "scarf"]);
    expect(wornNow(w, undefined, new Date("2026-10-26T10:00:00"))).toEqual(["crown", "scarf"]);
  });
});
