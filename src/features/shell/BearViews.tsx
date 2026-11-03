import { useId } from "react";
import { cn } from "@/lib/utils";

/**
 * The bear seen from other sides, drawn for the back-turn emote (FR-40). These are separate
 * drawings, not a mirrored front view: a back has no face or belly, and a three-quarter view
 * has a foreshortened face and a muzzle that sticks out past the cheek.
 * `turn` = three-quarter, facing right. `back` = seen from behind. `back_look` = seen from
 * behind, head turned back over the shoulder.
 */
export const TURNED_POSES = ["turn", "back", "back_look"] as const;
export type TurnedPose = (typeof TURNED_POSES)[number];
export const isTurnedPose = (p: string): p is TurnedPose => (TURNED_POSES as readonly string[]).includes(p);

/** Gradients and filters shared by every view of the bear. `g` makes ids unique per instance. */
export function BearDefs({ g }: { g: (name: string) => string }) {
  return (
    <defs>
      <radialGradient id={g("fur")} cx="34%" cy="24%" r="88%"><stop offset="0" stopColor="#F8DAB0" /><stop offset=".42" stopColor="#E3A465" /><stop offset=".85" stopColor="#B4702F" /><stop offset="1" stopColor="#8C5221" /></radialGradient>
      <radialGradient id={g("dark")} cx="34%" cy="24%" r="92%"><stop offset="0" stopColor="#EBB880" /><stop offset=".6" stopColor="#B87634" /><stop offset="1" stopColor="#82491B" /></radialGradient>
      {/* Form shading: clear in the middle, darker toward the far edge. */}
      <radialGradient id={g("shade")} cx="38%" cy="30%" r="75%"><stop offset=".55" stopColor="#5A2E0E" stopOpacity="0" /><stop offset="1" stopColor="#5A2E0E" stopOpacity=".38" /></radialGradient>
      {/* Rim light: warm bounce on the lower right edge. */}
      <radialGradient id={g("rim")} cx="78%" cy="82%" r="40%"><stop offset=".6" stopColor="#FFD9A0" stopOpacity="0" /><stop offset=".92" stopColor="#FFD9A0" stopOpacity=".35" /><stop offset="1" stopColor="#FFD9A0" stopOpacity="0" /></radialGradient>
      <radialGradient id={g("belly")} cx="40%" cy="28%" r="82%"><stop offset="0" stopColor="#FFF6E6" /><stop offset=".7" stopColor="#F1CE9E" /><stop offset="1" stopColor="#D9A86E" /></radialGradient>
      <radialGradient id={g("ear")} cx="40%" cy="35%" r="80%"><stop offset="0" stopColor="#FFC9BE" /><stop offset="1" stopColor="#CF7A6C" /></radialGradient>
      <radialGradient id={g("pad")} cx="40%" cy="35%" r="80%"><stop offset="0" stopColor="#F5B1A3" /><stop offset="1" stopColor="#B86A5C" /></radialGradient>
      <radialGradient id={g("nose")} cx="35%" cy="30%" r="80%"><stop offset="0" stopColor="#7A5545" /><stop offset="1" stopColor="#1E110C" /></radialGradient>
      <radialGradient id={g("eye")} cx="35%" cy="30%" r="80%"><stop offset="0" stopColor="#6A4A3E" /><stop offset="1" stopColor="#120B08" /></radialGradient>
      <radialGradient id={g("gloss")} cx="30%" cy="20%" r="60%"><stop offset="0" stopColor="#fff" stopOpacity=".8" /><stop offset="1" stopColor="#fff" stopOpacity="0" /></radialGradient>
      <linearGradient id={g("metal")} x1="0" y1="0" x2="0" y2="1"><stop offset="0" stopColor="#E5E7EB" /><stop offset="1" stopColor="#9CA3AF" /></linearGradient>
      <linearGradient id={g("gold")} x1="0" y1="0" x2="1" y2="1"><stop offset="0" stopColor="#FDE68A" /><stop offset=".6" stopColor="#F59E0B" /><stop offset="1" stopColor="#B45309" /></linearGradient>
      <filter id={g("soft")} x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="3.5" /></filter>
      <filter id={g("ao")} x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="5" /></filter>
    </defs>
  );
}

/** Head turned about 40 degrees to the right: far eye narrow, muzzle past the cheek line. */
function ThreeQuarterHead({ url, animated, dx = 0, dy = 0 }: { url: (n: string) => string; animated: boolean; dx?: number; dy?: number }) {
  return (
    <g data-part="face" transform={`translate(${dx} ${dy})`}>
      {/* Far ear is smaller and tucked behind; near ear faces us. */}
      <circle cx="64" cy="38" r="17" fill={url("dark")} />
      <circle cx="64" cy="38" r="17" fill={url("shade")} />
      <circle cx="66" cy="40" r="9" fill={url("ear")} opacity=".8" />
      <circle cx="152" cy="38" r="20" fill={url("dark")} />
      <circle cx="152" cy="38" r="20" fill={url("shade")} />
      <circle cx="150" cy="40.5" r="11" fill={url("ear")} />
      <ellipse cx="106" cy="80" rx="58" ry="52" fill={url("fur")} />
      <ellipse cx="106" cy="80" rx="58" ry="52" fill={url("shade")} />
      <ellipse cx="106" cy="80" rx="58" ry="52" fill={url("rim")} />
      <ellipse cx="82" cy="50" rx="28" ry="13" fill={url("gloss")} />
      <ellipse cx="146" cy="96" rx="10" ry="6.5" fill="#FF8F86" opacity=".42" />
      {/* Muzzle sticks out on the facing side. */}
      <ellipse cx="128" cy="102" rx="23" ry="13" fill="#7A3F12" opacity=".28" filter={url("soft")} />
      <ellipse cx="127" cy="97" rx="24" ry="18" fill={url("belly")} />
      <g className={animated ? "bear-blink" : undefined}>
        <ellipse cx="97" cy="72" rx="6" ry="9" fill={url("eye")} />
        <circle cx="95.5" cy="68.5" r="2.6" fill="#fff" />
        <ellipse cx="134" cy="72" rx="8.5" ry="9.5" fill={url("eye")} />
        <circle cx="131.2" cy="68.2" r="3.4" fill="#fff" />
        <circle cx="137" cy="76" r="1.6" fill="#fff" opacity=".8" />
      </g>
      <ellipse cx="144" cy="88" rx="8.5" ry="6" fill={url("nose")} />
      <ellipse cx="141.5" cy="86" rx="3.2" ry="1.7" fill="#fff" opacity=".6" />
      <path d="M144 93 v4 M144 97 q-6 6 -13 1" fill="none" stroke="#3A2620" strokeWidth="2.6" strokeLinecap="round" />
    </g>
  );
}

/** Back of the head: no face, ears seen from behind, a fur swirl on the crown. */
function BackHead({ url }: { url: (n: string) => string }) {
  return (
    <g data-part="back-of-head">
      {[54, 146].map((x) => (
        <g key={x}>
          <circle cx={x} cy="36" r="20" fill={url("dark")} />
          <circle cx={x} cy="36" r="20" fill={url("shade")} />
          {/* Seen from behind the inside of the ear is hidden; only a darker fur hint remains. */}
          <ellipse cx={x} cy="40" rx="8" ry="6" fill="#7A3B2E" opacity=".16" />
        </g>
      ))}
      <ellipse cx="100" cy="80" rx="60" ry="52" fill={url("fur")} />
      <ellipse cx="100" cy="80" rx="60" ry="52" fill={url("shade")} />
      <ellipse cx="100" cy="80" rx="60" ry="52" fill={url("rim")} />
      <ellipse cx="78" cy="50" rx="30" ry="14" fill={url("gloss")} opacity=".8" />
      <path d="M92 40 q10 -6 18 2 q4 8 -6 10" fill="none" stroke="#B97A3E" strokeWidth="3" strokeLinecap="round" opacity=".55" />
      <path d="M60 96 q40 18 80 0" fill="none" stroke="#8C5221" strokeWidth="3" strokeLinecap="round" opacity=".25" />
    </g>
  );
}

export function BearTurned({ pose, animated = false, className, style }: { pose: TurnedPose; animated?: boolean; className?: string | undefined; style?: React.CSSProperties | undefined }) {
  const u = useId().replace(/:/g, "");
  const g = (name: string) => `${name}-${u}`;
  const url = (name: string) => `url(#${g(name)})`;
  const anim = (cls: string) => (animated ? cls : undefined);
  const shaking = pose === "back_look";

  const label = pose === "turn" ? "three-quarter view" : pose === "back" ? "back view" : "looking back over the shoulder";

  return (
    <svg viewBox="0 0 200 200" role="img" aria-label={`Pluto the bear (${label})`} data-view={pose === "turn" ? "three_quarter" : "back"} className={cn("overflow-visible", className)} style={style}>
      <BearDefs g={g} />
      <ellipse cx="100" cy="188" rx="38" ry="3.5" fill="#000" opacity=".3" filter={url("soft")} />
      <ellipse cx="100" cy="189" rx="58" ry="7" fill="#000" opacity=".22" filter={url("soft")} />

      {pose === "turn" ? (
        <g className={anim("bear-breathe")}>
          {/* Far arm and far foot sit behind the body. */}
          <ellipse cx="56" cy="142" rx="13" ry="24" transform="rotate(10 56 142)" fill={url("dark")} opacity=".9" />
          <ellipse cx="66" cy="178" rx="18" ry="11" fill={url("dark")} />
          <ellipse cx="124" cy="178" rx="21" ry="12" fill={url("dark")} />
          <ellipse cx="124" cy="178" rx="21" ry="12" fill={url("shade")} />
          <ellipse cx="130" cy="181" rx="9" ry="5" fill={url("pad")} opacity=".85" />
          {/* Tail pokes out behind on the far side. */}
          <circle cx="46" cy="160" r="10" fill={url("dark")} />
          <ellipse cx="100" cy="140" rx="52" ry="46" fill={url("fur")} />
          <ellipse cx="100" cy="140" rx="52" ry="46" fill={url("shade")} />
          <ellipse cx="100" cy="140" rx="52" ry="46" fill={url("rim")} />
          <ellipse cx="116" cy="150" rx="27" ry="29" fill={url("belly")} />
          <ellipse cx="82" cy="134" rx="13" ry="9" fill={url("gloss")} opacity=".5" />
          {/* Near arm hangs in front of the belly edge. */}
          <ellipse cx="140" cy="142" rx="14" ry="25" transform="rotate(-10 140 142)" fill={url("dark")} />
          <ellipse cx="140" cy="142" rx="14" ry="25" transform="rotate(-10 140 142)" fill={url("shade")} />
          <ellipse cx="136" cy="131" rx="5.5" ry="10" transform="rotate(-10 140 142)" fill={url("gloss")} opacity=".5" />
          <ellipse cx="100" cy="124" rx="44" ry="10" fill="#4A2508" opacity=".35" filter={url("ao")} />
          <ThreeQuarterHead url={url} animated={animated} />
        </g>
      ) : (
        <g className={anim("bear-breathe")}>
          {/* Heels, seen from behind: no pads. */}
          {[70, 130].map((x) => (
            <g key={x}>
              <ellipse cx={x} cy="179" rx="18" ry="10" fill={url("dark")} />
              <ellipse cx={x} cy="179" rx="18" ry="10" fill={url("shade")} />
            </g>
          ))}
          {/* The hips, tail and lower back are what shakes; arms and feet stay put. */}
          <g className={shaking ? anim("bear-butt-shake") : undefined}>
            <ellipse cx="100" cy="140" rx="56" ry="46" fill={url("fur")} />
            <ellipse cx="100" cy="140" rx="56" ry="46" fill={url("shade")} />
            <ellipse cx="100" cy="140" rx="56" ry="46" fill={url("rim")} />
            <path d="M70 118 q30 12 60 0 M62 140 q38 16 76 0" fill="none" stroke="#8C5221" strokeWidth="3" strokeLinecap="round" opacity=".22" />
            <ellipse cx="82" cy="128" rx="14" ry="9" fill={url("gloss")} opacity=".45" />
            <g className={shaking ? anim("bear-tail-wag") : undefined}>
              <circle cx="100" cy="170" r="12" fill={url("dark")} />
              <circle cx="100" cy="170" r="12" fill={url("shade")} />
              <ellipse cx="96" cy="166" rx="4" ry="3" fill={url("gloss")} opacity=".6" />
            </g>
          </g>
          {[50, 150].map((cx) => (
            <g key={cx}>
              <ellipse cx={cx} cy="140" rx="14" ry="25" transform={`rotate(${cx < 100 ? 14 : -14} ${cx} 140)`} fill={url("dark")} />
              <ellipse cx={cx} cy="140" rx="14" ry="25" transform={`rotate(${cx < 100 ? 14 : -14} ${cx} 140)`} fill={url("shade")} />
              <ellipse cx={cx + (cx < 100 ? -3 : 3)} cy="129" rx="5" ry="10" transform={`rotate(${cx < 100 ? 14 : -14} ${cx} 140)`} fill={url("gloss")} opacity=".4" />
            </g>
          ))}
          <ellipse cx="100" cy="124" rx="44" ry="10" fill="#4A2508" opacity=".35" filter={url("ao")} />
          {pose === "back" ? <BackHead url={url} /> : <ThreeQuarterHead url={url} animated={animated} dx={-6} dy={2} />}
        </g>
      )}
    </svg>
  );
}
