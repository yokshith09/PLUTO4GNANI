import { birthdayToday, type Companion } from "@/features/companion/companion";

/**
 * Accessories and outfits for the built-in bear (owner 2026-10-26). Everything is free and open
 * from the first day: nothing is earned, locked or taken away (no-guilt rule). Worn things are
 * drawn over the bear (see Accessories.tsx); a sprite-pack character is its own picture and
 * cannot wear them. Stored in preferences under `ui.wardrobe`, so it is part of the data export.
 */
export const SLOTS = ["head", "face", "neck"] as const;
export type Slot = (typeof SLOTS)[number];

export const ITEMS = [
  { id: "party-hat", name: "Party hat", slot: "head" },
  { id: "crown", name: "Crown", slot: "head" },
  { id: "beanie", name: "Beanie", slot: "head" },
  { id: "cap", name: "Cap", slot: "head" },
  { id: "headphones", name: "Headphones", slot: "head" },
  { id: "glasses", name: "Round glasses", slot: "face" },
  { id: "sunglasses", name: "Sunglasses", slot: "face" },
  { id: "bow-tie", name: "Bow tie", slot: "neck" },
  { id: "scarf", name: "Scarf", slot: "neck" },
] as const;
export type ItemId = (typeof ITEMS)[number]["id"];

export const SLOT_LABEL: Record<Slot, string> = { head: "On the head", face: "On the face", neck: "Around the neck" };

export const OUTFITS: readonly { id: string; name: string; items: readonly ItemId[] }[] = [
  { id: "cosy", name: "Cosy", items: ["beanie", "scarf"] },
  { id: "smart", name: "Smart", items: ["glasses", "bow-tie"] },
  { id: "study", name: "Study", items: ["headphones", "glasses"] },
  { id: "cool", name: "Cool", items: ["cap", "sunglasses"] },
  { id: "royal", name: "Royal", items: ["crown", "scarf"] },
  { id: "party", name: "Party", items: ["party-hat", "bow-tie"] },
];

export interface Wardrobe {
  /** What is worn, at most one thing per slot. */
  worn: ItemId[];
  /** The outfit that was picked, while the worn things still match it. */
  outfit: string;
  /** Wear the party hat on a birthday (yours or the pet's). */
  partyOnBirthday: boolean;
}

export const emptyWardrobe: Wardrobe = { worn: [], outfit: "", partyOnBirthday: true };

const slotOf = (id: ItemId): Slot => ITEMS.find((i) => i.id === id)?.slot ?? "head";
const isItem = (v: unknown): v is ItemId => ITEMS.some((i) => i.id === v);

/** At most one item per slot; anything unknown is dropped. Never throws. */
export function normalize(ids: readonly unknown[]): ItemId[] {
  const bySlot = new Map<Slot, ItemId>();
  for (const id of ids) if (isItem(id)) bySlot.set(slotOf(id), id);
  return SLOTS.flatMap((s) => bySlot.get(s) ?? []);
}

export function parseWardrobe(raw: unknown): Wardrobe {
  if (!raw || typeof raw !== "object") return emptyWardrobe;
  const r = raw as Record<string, unknown>;
  const worn = normalize(Array.isArray(r.worn) ? r.worn : []);
  const outfit = typeof r.outfit === "string" && matches(r.outfit, worn) ? r.outfit : "";
  return { worn, outfit, partyOnBirthday: r.partyOnBirthday !== false };
}

function matches(outfitId: string, worn: readonly ItemId[]): boolean {
  const o = OUTFITS.find((x) => x.id === outfitId);
  return !!o && o.items.length === worn.length && o.items.every((i) => worn.includes(i));
}

/** Wear or take off one thing; it replaces whatever was in its slot. */
export function toggleItem(w: Wardrobe, id: ItemId): Wardrobe {
  const worn = w.worn.includes(id) ? w.worn.filter((x) => x !== id) : normalize([...w.worn, id]);
  return { ...w, worn, outfit: OUTFITS.find((o) => matches(o.id, worn))?.id ?? "" };
}

/** Put a whole outfit on, or take it off again if it is already on. */
export function toggleOutfit(w: Wardrobe, id: string): Wardrobe {
  const o = OUTFITS.find((x) => x.id === id);
  if (!o) return w;
  return w.outfit === id ? { ...w, worn: [], outfit: "" } : { ...w, worn: normalize(o.items), outfit: id };
}

/** What the pet has on right now: the saved look, with the party hat on a birthday. */
export function wornNow(w: Wardrobe, companion: Companion | undefined, now: Date): ItemId[] {
  if (!w.partyOnBirthday || !companion || !birthdayToday(companion, now)) return w.worn;
  return normalize([...w.worn, "party-hat"]);
}
