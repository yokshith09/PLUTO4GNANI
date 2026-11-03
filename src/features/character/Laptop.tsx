import { useId } from "react";

/** Where the typing-along sequence is: typing, stopped and looking at you, putting it away. */
export type LaptopPhase = "type" | "wait" | "stow";

/**
 * The laptop every character types on, seen the way you'd really see it: the character faces
 * you, so you see the back of the lid (with a glowing logo) and the screen's light on its face.
 * The paws reach over the top of the lid to the keys (draw them before this, so the lid hides
 * their tips). It comes out of the pocket at `pocketY`, the lid opens, and on `stow` it goes back.
 */
export function BackLaptop({ phase, animated, top = 128, pocketY = 166 }: { phase: LaptopPhase; animated: boolean; top?: number; pocketY?: number }) {
  const u = useId().replace(/:/g, "");
  const id = (n: string) => `${n}-${u}`;
  const url = (n: string) => `url(#${n}-${u})`;
  const y = top;
  const out = animated ? (phase === "stow" ? "pet-laptop-away" : "pet-laptop-out") : undefined;
  return (
    <g className={out} style={{ transformOrigin: `100px ${pocketY}px` }}>
      <defs>
        <linearGradient id={id("alu")} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#6B7385" /><stop offset=".45" stopColor="#3B4252" /><stop offset="1" stopColor="#1C212B" />
        </linearGradient>
        <linearGradient id={id("edge")} x1="0" y1="0" x2="0" y2="1"><stop offset="0" stopColor="#4A5262" /><stop offset="1" stopColor="#181C24" /></linearGradient>
        <radialGradient id={id("logo")}><stop offset="0" stopColor="#FFFFFF" /><stop offset=".45" stopColor="#99F6E4" /><stop offset="1" stopColor="#2DD4BF" stopOpacity="0" /></radialGradient>
        <radialGradient id={id("spill")}><stop offset="0" stopColor="#93C5FD" stopOpacity=".7" /><stop offset="1" stopColor="#93C5FD" stopOpacity="0" /></radialGradient>
      </defs>
      {/* Screen light on the character's face and chest, above the lid. */}
      <ellipse cx="100" cy={y - 8} rx="44" ry="22" fill={url("spill")} opacity={phase === "type" ? 0.9 : 0.45} className={animated && phase === "type" ? "bear-screen" : undefined} />
      {/* Shadow the laptop casts on the body. */}
      <ellipse cx="100" cy={y + 47} rx="44" ry="5" fill="#000" opacity=".25" />
      {/* Base: its front edge, a little wider than the lid. */}
      <path d={`M58 ${y + 41} H142 Q145 ${y + 41} 144 ${y + 44} L142 ${y + 47} H58 L56 ${y + 44} Q55 ${y + 41} 58 ${y + 41} Z`} fill={url("edge")} />
      <path d={`M60 ${y + 41.6} H140`} stroke="#fff" strokeOpacity=".22" strokeWidth="1" />
      <ellipse cx="100" cy={y + 45} rx="7" ry="1.2" fill="#374151" opacity=".6" />
      {/* Lid (back side), hinged at the base; it swings up when the laptop comes out. */}
      <g className={animated && phase !== "stow" ? "pet-lid-open pet-lid-late" : undefined}>
        <path d={`M66 ${y + 42} L69 ${y + 4} Q69.5 ${y} 74 ${y} H126 Q130.5 ${y} 131 ${y + 4} L134 ${y + 42} Z`} fill={url("alu")} />
        <path d={`M131 ${y + 4} L134 ${y + 42} H129 L127 ${y + 3} Z`} fill="#000" opacity=".1" />
        <path d={`M72 ${y + 1.2} H128`} stroke="#fff" strokeOpacity=".4" strokeWidth="1.2" strokeLinecap="round" />
        <ellipse cx="84" cy={y + 12} rx="12" ry="6" fill="#fff" opacity=".12" transform={`rotate(-12 84 ${y + 12})`} />
        <circle cx="100" cy={y + 21} r="9" fill={url("logo")} opacity={phase === "type" ? 1 : 0.6} className={animated && phase === "type" ? "pet-glow" : undefined} />
        <path d={`M96 ${y + 23} q0 -6 4 -6 q4 0 4 6 z`} fill="#fff" opacity=".95" />
      </g>
    </g>
  );
}

/** The front pocket the laptop comes out of. */
export function Pocket({ y = 160, color = "#000", opacity = 0.18 }: { y?: number; color?: string; opacity?: number }) {
  return (
    <g>
      <path d={`M80 ${y - 6} H120 V${y + 6} Q100 ${y + 18} 80 ${y + 6} Z`} fill={color} opacity={opacity} />
      <path d={`M80 ${y - 6} H120`} stroke={color} strokeOpacity={opacity * 1.8} strokeWidth="2" strokeLinecap="round" />
      <path d={`M83 ${y - 3} H117 V${y + 5} Q100 ${y + 15} 83 ${y + 5} Z`} fill="none" stroke="#fff" strokeOpacity=".35" strokeWidth="1" strokeDasharray="2.5 2" />
    </g>
  );
}

export const isTypingPose = (pose: string): pose is LaptopPhase => pose === "type" || pose === "wait" || pose === "stow";
