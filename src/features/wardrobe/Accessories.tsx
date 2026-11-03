import type { ReactNode } from "react";
import type { ItemId } from "./wardrobe";

/**
 * The drawings for each accessory, in the bear's 200x200 space (head ellipse centred at 100,80;
 * eyes at x 77 and 123, y 72). Plain fills with no ids, so they are safe to repeat on one page.
 * `head` and `face` things go inside the bear's head group so they move with it; `neck` things
 * go on top of the head and body.
 */
const ART: Record<ItemId, ReactNode> = {
  "party-hat": (
    <g transform="rotate(-8 100 36)">
      <path d="M78 38 L100 8 L122 38 Z" fill="#F472B6" stroke="#BE185D" strokeWidth="1.5" strokeLinejoin="round" />
      <path d="M84 30 L116 26 M90 20 L110 17" stroke="#FDE68A" strokeWidth="3.5" strokeLinecap="round" />
      <circle cx="100" cy="7" r="5.5" fill="#FBBF24" stroke="#D97706" strokeWidth="1.2" />
    </g>
  ),
  crown: (
    <g>
      <path d="M72 38 L70 10 L86 24 L100 4 L114 24 L130 10 L128 38 Z" fill="#FBBF24" stroke="#B45309" strokeWidth="2" strokeLinejoin="round" />
      <rect x="72" y="32" width="56" height="7" rx="2" fill="#F59E0B" stroke="#B45309" strokeWidth="1.5" />
      <circle cx="100" cy="21" r="3.4" fill="#EF4444" /><circle cx="84" cy="30" r="2.4" fill="#3B82F6" /><circle cx="116" cy="30" r="2.4" fill="#22C55E" />
    </g>
  ),
  beanie: (
    <g>
      <path d="M60 48 Q58 10 100 10 Q142 10 140 48 Z" fill="#60A5FA" stroke="#1D4ED8" strokeWidth="1.8" strokeLinejoin="round" />
      <rect x="56" y="38" width="88" height="14" rx="7" fill="#3B82F6" stroke="#1D4ED8" strokeWidth="1.8" />
      <path d="M66 45 H134" stroke="#BFDBFE" strokeWidth="2" strokeDasharray="3 5" strokeLinecap="round" />
      <circle cx="100" cy="9" r="6.5" fill="#F8FAFC" stroke="#CBD5E1" strokeWidth="1.5" />
    </g>
  ),
  cap: (
    <g>
      <path d="M64 46 Q64 10 100 10 Q136 10 136 46 Z" fill="#EF4444" stroke="#991B1B" strokeWidth="1.8" strokeLinejoin="round" />
      <path d="M62 46 Q100 58 138 46 Q138 41 133 41 H67 Q62 41 62 46 Z" fill="#B91C1C" stroke="#7F1D1D" strokeWidth="1.5" strokeLinejoin="round" />
      <circle cx="100" cy="9" r="3.5" fill="#B91C1C" />
    </g>
  ),
  headphones: (
    <g>
      <path d="M43 78 C43 18 157 18 157 78" fill="none" stroke="#334155" strokeWidth="7" strokeLinecap="round" />
      <path d="M43 78 C43 20 157 20 157 78" fill="none" stroke="#64748B" strokeWidth="1.6" strokeLinecap="round" opacity=".7" />
      {[41, 159].map((x) => (
        <g key={x}>
          <ellipse cx={x} cy="80" rx="10" ry="16" fill="#1E293B" stroke="#0F172A" strokeWidth="1.5" />
          <ellipse cx={x} cy="80" rx="5" ry="10" fill="#475569" />
        </g>
      ))}
    </g>
  ),
  glasses: (
    <g fill="#E0F2FE" fillOpacity=".25" stroke="#1F2937" strokeWidth="3.2">
      <circle cx="77" cy="72" r="16" /><circle cx="123" cy="72" r="16" />
      <path d="M93 70 Q100 65 107 70" fill="none" />
      <path d="M61 70 L48 66 M139 70 L152 66" fill="none" strokeLinecap="round" />
    </g>
  ),
  sunglasses: (
    <g>
      <path d="M58 62 H96 V76 Q96 90 78 90 Q58 90 58 76 Z" fill="#111827" stroke="#030712" strokeWidth="2" strokeLinejoin="round" />
      <path d="M104 62 H142 V76 Q142 90 122 90 Q104 90 104 76 Z" fill="#111827" stroke="#030712" strokeWidth="2" strokeLinejoin="round" />
      <path d="M96 66 Q100 63 104 66" fill="none" stroke="#111827" strokeWidth="3.5" />
      <path d="M58 65 L47 62 M142 65 L153 62" stroke="#111827" strokeWidth="3" strokeLinecap="round" />
      <path d="M64 67 L72 67 M110 67 L118 67" stroke="#fff" strokeOpacity=".4" strokeWidth="3" strokeLinecap="round" />
    </g>
  ),
  "bow-tie": (
    <g stroke="#7F1D1D" strokeWidth="1.5" strokeLinejoin="round">
      <path d="M100 134 L74 120 Q70 132 74 146 Z" fill="#EF4444" />
      <path d="M100 134 L126 120 Q130 132 126 146 Z" fill="#EF4444" />
      <rect x="94" y="127" width="12" height="14" rx="4" fill="#DC2626" />
    </g>
  ),
  scarf: (
    <g stroke="#B45309" strokeWidth="1.6" strokeLinejoin="round">
      <path d="M52 124 Q100 152 148 124 L152 140 Q100 168 48 140 Z" fill="#F59E0B" />
      <path d="M116 146 L128 178 L110 182 L102 152 Z" fill="#FBBF24" />
      <path d="M66 134 L70 148 M84 141 L87 155 M114 141 L112 155 M132 134 L129 148" stroke="#FEF3C7" strokeWidth="2.4" strokeLinecap="round" fill="none" />
    </g>
  ),
};

const draw = (ids: readonly ItemId[], which: readonly ItemId[]) =>
  which.filter((id) => ids.includes(id)).map((id) => <g key={id} data-accessory={id}>{ART[id]}</g>);

const ON_HEAD = ["party-hat", "crown", "beanie", "cap", "headphones", "glasses", "sunglasses"] as const;
const AT_NECK = ["bow-tie", "scarf"] as const;

/** Drawn inside the bear's head group, so it moves with the head. */
export const headGear = (ids: readonly ItemId[]) => draw(ids, ON_HEAD);
/** Drawn over the head and body. */
export const neckGear = (ids: readonly ItemId[]) => draw(ids, AT_NECK);

/** The part of the 200x200 drawing space each accessory occupies, so its icon is cropped to it. */
const ICON_BOX: Record<ItemId, string> = {
  "party-hat": "70 2 60 42",
  crown: "64 0 72 44",
  beanie: "50 2 100 54",
  cap: "56 6 88 54",
  headphones: "28 28 144 74",
  glasses: "44 52 112 40",
  sunglasses: "42 56 116 38",
  "bow-tie": "66 114 68 38",
  scarf: "44 118 112 68",
};

/** A small picture of an accessory, for the wardrobe list. */
export function AccessoryIcon({ id, className }: { id: ItemId; className?: string }) {
  return (
    <svg viewBox={ICON_BOX[id]} aria-hidden="true" className={className} data-icon={id}>
      {ART[id]}
    </svg>
  );
}
