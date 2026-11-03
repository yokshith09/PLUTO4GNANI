/**
 * Small props shared by the sprite-only poses (FR-50): the page a character reads in `review`
 * and the tear in `sad`. Plain fills, no gradient ids, so they are safe to repeat in one atlas.
 */

/** A clipboard held in front of the belly, with written lines and one tick. Paws go on top of it. */
export function Clipboard({ y = 124, w = 52, h = 40 }: { y?: number; w?: number; h?: number }) {
  const x = 100 - w / 2;
  return (
    <g data-part="clipboard">
      <rect x={x + 2} y={y + 3} width={w} height={h} rx="5" fill="#000" opacity=".18" />
      <rect x={x} y={y} width={w} height={h} rx="5" fill="#B7793F" />
      <rect x={x + 4} y={y + 5} width={w - 8} height={h - 9} rx="2.5" fill="#FFFDF7" />
      <rect x={100 - 9} y={y - 4} width="18" height="8" rx="3" fill="#94A3B8" />
      <path d={`M${x + 10} ${y + 14} h${w - 26} M${x + 10} ${y + 21} h${w - 20} M${x + 10} ${y + 28} h${w - 30}`} stroke="#A8A29E" strokeWidth="2.2" strokeLinecap="round" />
      <path d={`M${x + w - 15} ${y + 26} l3 3 l6 -7`} stroke="#22C55E" strokeWidth="2.6" fill="none" strokeLinecap="round" strokeLinejoin="round" />
    </g>
  );
}

/** One tear on the near cheek. */
export function Tear({ x, y }: { x: number; y: number }) {
  return (
    <g data-part="tear">
      <path d={`M${x} ${y} q5 8 0 12 q-5 -4 0 -12 z`} fill="#93C5FD" stroke="#60A5FA" strokeWidth="1" />
      <ellipse cx={x - 1.2} cy={y + 7} rx="1" ry="1.8" fill="#fff" opacity=".8" />
    </g>
  );
}

/** Brows raised in the middle: the sad look. */
export function SadBrows({ l, r, y, color = "#3A2620" }: { l: number; r: number; y: number; color?: string }) {
  return (
    <g fill="none" stroke={color} strokeWidth="2.8" strokeLinecap="round" data-part="brows">
      <path d={`M${l - 9} ${y + 3} l16 -5`} />
      <path d={`M${r + 9} ${y + 3} l-16 -5`} />
    </g>
  );
}
