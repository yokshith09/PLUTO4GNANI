import { useId } from "react";
import { cn } from "@/lib/utils";
import { BackLaptop, Pocket } from "@/features/character/Laptop";
import { Clipboard, Tear } from "@/features/character/SpriteProps";
import { BearDefs, BearTurned, TURNED_POSES, isTurnedPose } from "./BearViews";
import { headGear, neckGear } from "@/features/wardrobe/Accessories";
import type { ItemId } from "@/features/wardrobe/wardrobe";

/** Everything the bear can do. Reactions (src/pet/reactions.ts) pick one of these. */
const FRONT_POSES = ["idle", "wave", "sleep", "work", "type", "wait", "stow", "cheer", "party", "catch", "worried", "ring", "toss", "jump", "sad", "review"] as const;
/** Front poses are what reactions use; the turned views (BearViews.tsx) exist for emotes such as the back-turn shake. */
export const BEAR_POSES = [...FRONT_POSES, ...TURNED_POSES] as const;
export type BearPose = (typeof BEAR_POSES)[number];
type FrontPose = (typeof FRONT_POSES)[number];

type Eyes = "open" | "happy" | "closed" | "down";
type Mouth = "smile" | "open" | "worried";
type Arms = "down" | "wave" | "up" | "hold" | "throw" | "reach";
type Prop = "laptop" | "card" | "bell" | "ball" | "confetti" | "zzz" | "tear" | "clipboard" | null;

interface Look { eyes: Eyes; mouth: Mouth; arms: Arms; prop: Prop; body?: string; brows?: boolean }

/** Face, arms and prop per pose. The body class is the whole-body motion (CSS, transforms only). */
const LOOKS: Record<FrontPose, Look> = {
  idle: { eyes: "open", mouth: "smile", arms: "down", prop: null, body: "bear-breathe" },
  wave: { eyes: "happy", mouth: "open", arms: "wave", prop: null, body: "bear-breathe" },
  sleep: { eyes: "closed", mouth: "smile", arms: "down", prop: "zzz", body: "bear-breathe-slow" },
  work: { eyes: "down", mouth: "smile", arms: "reach", prop: "laptop", body: "bear-breathe" },
  // Typing along (pet.typing): out of the pocket, lid up, paws on the keys while you type;
  // when you stop it looks up at you and waits; then it puts the laptop away.
  type: { eyes: "down", mouth: "smile", arms: "reach", prop: "laptop", body: "bear-breathe" },
  wait: { eyes: "open", mouth: "smile", arms: "reach", prop: "laptop", body: "bear-breathe" },
  stow: { eyes: "happy", mouth: "smile", arms: "reach", prop: "laptop", body: "bear-breathe" },
  cheer: { eyes: "happy", mouth: "open", arms: "up", prop: null, body: "bear-jump" },
  party: { eyes: "happy", mouth: "open", arms: "up", prop: "confetti", body: "bear-jump bear-jump-twice" },
  catch: { eyes: "open", mouth: "open", arms: "hold", prop: "card", body: "bear-catch" },
  worried: { eyes: "open", mouth: "worried", arms: "down", prop: null, body: "bear-shake", brows: true },
  ring: { eyes: "happy", mouth: "open", arms: "hold", prop: "bell", body: "bear-breathe" },
  toss: { eyes: "happy", mouth: "smile", arms: "throw", prop: "ball", body: "bear-breathe" },
  // Sprite-pack poses (FR-50): mid-air jump, the "failed" row, and reading over a clipboard.
  jump: { eyes: "open", mouth: "open", arms: "up", prop: null, body: "bear-jump" },
  sad: { eyes: "down", mouth: "worried", arms: "down", prop: "tear", body: "bear-breathe-slow", brows: true },
  review: { eyes: "down", mouth: "smile", arms: "hold", prop: "clipboard", body: "bear-breathe" },
};

const CONFETTI = [
  { x: 40, y: 40, dx: -38, dy: -30, c: "#F59E0B", r: 20 }, { x: 160, y: 40, dx: 36, dy: -34, c: "#22C55E", r: -30 },
  { x: 60, y: 20, dx: -20, dy: -48, c: "#3B82F6", r: 45 }, { x: 140, y: 18, dx: 22, dy: -50, c: "#EC4899", r: -15 },
  { x: 100, y: 10, dx: 2, dy: -56, c: "#F97316", r: 60 }, { x: 30, y: 80, dx: -46, dy: -8, c: "#A855F7", r: 10 },
  { x: 170, y: 80, dx: 46, dy: -10, c: "#14B8A6", r: -50 },
];

/**
 * Pluto the bear, drawn "soft 3D": one light from the top left, form shading toward the edges,
 * a warm rim light on the shadow side, contact shadows where parts meet, and a blurred ground
 * shadow. `animated` turns on the motion (CSS transforms only; reduced motion stops it).
 */
export function Bear({ pose = "idle", animated = false, className, style, wear = [] }: { pose?: BearPose; animated?: boolean; className?: string; style?: React.CSSProperties; wear?: readonly ItemId[] }) {
  const u = useId().replace(/:/g, "");
  const g = (name: string) => `${name}-${u}`;
  const url = (name: string) => `url(#${g(name)})`;
  if (isTurnedPose(pose)) return <BearTurned pose={pose} animated={animated} className={className} style={style} />;
  const look = LOOKS[pose];
  const anim = (cls: string | undefined) => (animated ? cls : undefined);
  const arm = (side: "l" | "r") => {
    const cx = side === "l" ? 50 : 150;
    const tilt = side === "l" ? 14 : -14;
    const shoulder = side === "l" ? "58px 122px" : "142px 122px";
    const sign = side === "l" ? 1 : -1;
    const angle = (() => {
      switch (look.arms) {
        case "up": return 120 * sign;
        case "hold": return -38 * sign;
        case "wave": return side === "r" ? -130 : 0;
        case "throw": return side === "r" ? -110 : 0;
        default: return 0;
      }
    })();
    const cls = look.arms === "wave" && side === "r" ? "bear-wave"
      : look.arms === "up" ? (side === "l" ? "bear-arm-up-l" : "bear-arm-up-r")
      : look.arms === "throw" && side === "r" ? "bear-throw" : undefined;
    return (
      <g style={angle ? { transformOrigin: shoulder, transform: `rotate(${angle}deg)` } : { transformOrigin: shoulder }} className={anim(cls)}>
        <ellipse cx={cx} cy="140" rx="14" ry="25" transform={`rotate(${tilt} ${cx} 140)`} fill={url("dark")} />
        <ellipse cx={cx} cy="140" rx="14" ry="25" transform={`rotate(${tilt} ${cx} 140)`} fill={url("shade")} />
        <ellipse cx={cx - 4} cy="129" rx="5.5" ry="10" transform={`rotate(${tilt} ${cx} 140)`} fill={url("gloss")} opacity=".55" />
        <ellipse cx={cx + sign * -2} cy="160" rx="8" ry="6" fill={url("pad")} opacity=".9" transform={`rotate(${tilt} ${cx} 140)`} />
      </g>
    );
  };

  return (
    <svg viewBox="0 0 200 200" role="img" aria-label={`Pluto the bear (${pose})`} className={cn("overflow-visible", className)} style={style}>
      <BearDefs g={g} />

      {/* Ground shadow: shrinks while the bear is in the air. */}
      <ellipse cx="100" cy="188" rx="38" ry="3.5" fill="#000" opacity=".3" filter={url("soft")} />
      <ellipse cx="100" cy="189" rx="58" ry="7" fill="#000" opacity=".22" filter={url("soft")} className={anim(look.body?.includes("jump") ? `bear-shadow-jump${look.body.includes("twice") ? " bear-jump-twice" : ""}` : undefined)} />

      <g className={anim(look.body)}>
        {/* Feet */}
        {[70, 130].map((x) => (
          <g key={x}>
            <ellipse cx={x} cy="178" rx="19" ry="12" fill={url("dark")} />
            <ellipse cx={x} cy="178" rx="19" ry="12" fill={url("shade")} />
            <ellipse cx={x} cy="181" rx="9" ry="5.5" fill={url("pad")} opacity=".85" />
          </g>
        ))}

        {/* Body */}
        <ellipse cx="100" cy="140" rx="56" ry="46" fill={url("fur")} />
        <ellipse cx="100" cy="140" rx="56" ry="46" fill={url("shade")} />
        <ellipse cx="100" cy="140" rx="56" ry="46" fill={url("rim")} />
        <ellipse cx="100" cy="149" rx="33" ry="30" fill={url("belly")} />
        <ellipse cx="86" cy="134" rx="14" ry="9" fill={url("gloss")} opacity=".5" />

        {/* Props held in front of the belly sit under the paws. */}
        {look.prop === "card" && (
          <g className={anim("bear-card-in")}>
            <rect x="76" y="126" width="48" height="34" rx="5" fill="#FFFDF7" stroke="#E5D3B3" />
            <path d="M84 137 h32 M84 145 h24 M84 152 h18" stroke="#C9B48F" strokeWidth="2.4" strokeLinecap="round" />
            <circle cx="117" cy="152" r="5" fill="#22C55E" />
          </g>
        )}
        {look.prop === "clipboard" && <Clipboard y={124} />}
        {look.prop === "bell" && (
          <g className={anim("bear-bell")} style={{ transformOrigin: "100px 120px" }}>
            <path d="M100 120 c-14 0 -18 12 -18 24 l-5 8 h46 l-5 -8 c0 -12 -4 -24 -18 -24 z" fill={url("gold")} />
            <ellipse cx="94" cy="132" rx="4" ry="7" fill="#fff" opacity=".45" />
            <circle cx="100" cy="155" r="4.5" fill="#92400E" />
            <circle cx="100" cy="119" r="3" fill="#B45309" />
          </g>
        )}

        {look.prop === "laptop" && <Pocket y={162} color="#6B3A16" opacity={0.22} />}
        {look.arms !== "up" && look.arms !== "reach" && arm("l")}
        {look.arms !== "up" && look.arms !== "reach" && arm("r")}

        {look.prop === "ball" && (
          <g className={anim("bear-ball")}>
            <circle cx="166" cy="74" r="10" fill="#F5F5F4" stroke="#D6D3D1" />
            <path d="M159 70 q6 4 13 -1 M160 79 q6 -3 12 2" stroke="#A8A29E" strokeWidth="1.5" fill="none" />
          </g>
        )}

        {/* Contact shadow where the head meets the body. */}
        <ellipse cx="100" cy="124" rx="44" ry="10" fill="#4A2508" opacity=".35" filter={url("ao")} />

        {/* Head */}
        <g className={anim(pose === "idle" ? "bear-head-tilt" : pose === "wait" ? "pet-look" : undefined)}>
          {[54, 146].map((x) => (
            <g key={x}>
              <circle cx={x} cy="36" r="20" fill={url("dark")} />
              <circle cx={x} cy="36" r="20" fill={url("shade")} />
              <circle cx={x} cy="38.5" r="11" fill={url("ear")} />
              <ellipse cx={x} cy="44" rx="8" ry="3" fill="#7A3B2E" opacity=".25" />
            </g>
          ))}
          <ellipse cx="100" cy="80" rx="60" ry="52" fill={url("fur")} />
          <ellipse cx="100" cy="80" rx="60" ry="52" fill={url("shade")} />
          <ellipse cx="100" cy="80" rx="60" ry="52" fill={url("rim")} />
          <path d="M78 36 q6 -5 10 1 M95 31 q6 -5 10 0 M112 36 q6 -5 10 1" fill="none" stroke="#B97A3E" strokeWidth="3" strokeLinecap="round" opacity=".5" />
          <ellipse cx="76" cy="50" rx="30" ry="14" fill={url("gloss")} />
          <ellipse cx="70" cy="46" rx="9" ry="4.5" fill="#fff" opacity=".55" transform="rotate(-24 70 46)" filter={url("soft")} />
          <ellipse cx="64" cy="95" rx="10" ry="6.5" fill="#FF8F86" opacity=".42" />
          <ellipse cx="136" cy="95" rx="10" ry="6.5" fill="#FF8F86" opacity=".42" />

          {/* Muzzle with a soft shadow under it. */}
          <ellipse cx="100" cy="101" rx="25" ry="14" fill="#7A3F12" opacity=".28" filter={url("soft")} />
          <ellipse cx="100" cy="96" rx="26" ry="19" fill={url("belly")} />

          {/* Eyes */}
          {look.eyes === "open" && (
            <g className={anim("bear-blink")}>
              {[77, 123].map((x) => (
                <g key={x}>
                  <ellipse cx={x} cy="72" rx="8.5" ry="9.5" fill={url("eye")} />
                  <circle cx={x - 2.8} cy="68.2" r="3.4" fill="#fff" />
                  <circle cx={x + 3} cy="76" r="1.6" fill="#fff" opacity=".8" />
                </g>
              ))}
            </g>
          )}
          {look.eyes === "down" && (
            <g className={anim("bear-blink")}>
              {[77, 123].map((x) => (
                <g key={x}>
                  <ellipse cx={x} cy="75" rx="8" ry="8.5" fill={url("eye")} />
                  <circle cx={x - 2.4} cy="77" r="2.8" fill="#fff" />
                </g>
              ))}
            </g>
          )}
          {look.eyes === "happy" && (
            <g fill="none" stroke="#2A1812" strokeWidth="3.6" strokeLinecap="round">
              <path d="M68 75 q9 -11 18 0" /><path d="M114 75 q9 -11 18 0" />
            </g>
          )}
          {look.eyes === "closed" && (
            <g fill="none" stroke="#2A1812" strokeWidth="3.4" strokeLinecap="round">
              <path d="M68 73 q9 8 18 0" /><path d="M114 73 q9 8 18 0" />
            </g>
          )}
          {look.brows && (
            <g fill="none" stroke="#6B3A16" strokeWidth="3" strokeLinecap="round">
              <path d="M68 58 l15 -5" /><path d="M132 58 l-15 -5" />
            </g>
          )}

          {/* Nose and mouth */}
          <ellipse cx="100" cy="87" rx="9.5" ry="6.5" fill={url("nose")} />
          <ellipse cx="97" cy="85" rx="3.6" ry="1.8" fill="#fff" opacity=".6" />
          {look.mouth === "smile" && (
            <path d="M100 93 v5 M100 98 q-7 7 -15 1 M100 98 q7 7 15 1" fill="none" stroke="#3A2620" strokeWidth="2.6" strokeLinecap="round" />
          )}
          {look.mouth === "open" && (
            <g>
              <path d="M100 93 v3" stroke="#3A2620" strokeWidth="2.6" strokeLinecap="round" />
              <path d="M88 98 q12 16 24 0 z" fill="#7A2E24" stroke="#3A2620" strokeWidth="2.2" strokeLinejoin="round" />
              <ellipse cx="100" cy="105" rx="5" ry="2.6" fill="#FF8F86" />
            </g>
          )}
          {look.mouth === "worried" && (
            <path d="M100 93 v4 M90 104 q5 -5 10 0 q5 5 10 0" fill="none" stroke="#3A2620" strokeWidth="2.6" strokeLinecap="round" />
          )}
          {look.prop === "tear" && <Tear x={66} y={82} />}
          {pose === "worried" && (
            <path d="M150 52 q6 10 0 14 q-6 -4 0 -14 z" fill="#93C5FD" stroke="#60A5FA" strokeWidth="1" className={anim("bear-sweat")} />
          )}
          {headGear(wear)}
        </g>
        {neckGear(wear)}

        {/* Typing: the arms reach over the lid to the keys; the lid hides the paw tips. */}
        {look.arms === "reach" && (["l", "r"] as const).map((side) => {
          const s = side === "l" ? 1 : -1;
          const ax = side === "l" ? 68 : 132;
          const px = side === "l" ? 86 : 114;
          return (
            <g key={side} className={anim(pose === "type" || pose === "work" ? (side === "l" ? "pet-tap-l" : "pet-tap-r") : undefined)}>
              <ellipse cx={ax} cy="134" rx="13" ry="19" transform={`rotate(${71 * s} ${ax} 134)`} fill={url("dark")} />
              <ellipse cx={ax} cy="134" rx="13" ry="19" transform={`rotate(${71 * s} ${ax} 134)`} fill={url("shade")} />
              <ellipse cx={ax - 2 * s} cy="129" rx="9" ry="4" transform={`rotate(${-18 * s} ${ax} 129)`} fill={url("gloss")} opacity=".5" />
              <ellipse cx={px} cy="128" rx="10" ry="8" fill={url("dark")} />
              <ellipse cx={px} cy="128" rx="10" ry="8" fill={url("shade")} />
            </g>
          );
        })}
        {look.prop === "laptop" && <BackLaptop phase={pose === "wait" ? "wait" : pose === "stow" ? "stow" : "type"} animated={animated} top={130} pocketY={164} />}

        {/* Raised arms are in front of the head. */}
        {look.arms === "up" && arm("l")}
        {look.arms === "up" && arm("r")}
      </g>

      {look.prop === "zzz" && (
        <g fill="#94A3B8" fontWeight="700" fontFamily="system-ui, sans-serif">
          <text x="150" y="40" fontSize="16" className={anim("bear-zzz")}>z</text>
          <text x="162" y="24" fontSize="12" className={anim("bear-zzz bear-zzz-late")}>z</text>
        </g>
      )}
      {look.prop === "confetti" && (
        <g>
          {CONFETTI.map((p, i) => (
            <rect
              key={i} x={p.x} y={p.y} width="7" height="4" rx="1" fill={p.c}
              className={anim("bear-confetti")}
              style={{ "--dx": `${p.dx}px`, "--dy": `${p.dy}px`, "--r": `${p.r}deg`, animationDelay: `${i * 40}ms` } as React.CSSProperties}
            />
          ))}
        </g>
      )}
    </svg>
  );
}
