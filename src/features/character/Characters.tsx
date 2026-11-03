import { useId, type ReactNode } from "react";
import { cn } from "@/lib/utils";
import { BackLaptop, Pocket, isTypingPose } from "./Laptop";
import { Mira, Raja } from "./People";
import { Clipboard, SadBrows, Tear } from "./SpriteProps";

/**
 * The new characters (Phase 7, pulled forward by the owner): Mochi, Pip, Bolt, Ghosty.
 * Drawn as lit 3D volumes: a key light from the top left, a core shadow, a cool rim light
 * on the far edge, a sharp specular highlight, and contact shadows where parts meet.
 * Same core poses as Pluto the bear. `type` is the typing-along pose: a laptop opens and they tap with you.
 */
export const CHARACTER_POSES = ["idle", "type", "wait", "cheer", "sleep"] as const;
/**
 * Poses drawn for the sprite packs (FR-50) and not used by the app's reactions yet:
 * `jump` (arms up, mid-air), `sad` (the "failed" row: brows up, frown, a tear),
 * `review` (holding a clipboard, eyes down, reading).
 */
export const SPRITE_POSES = ["jump", "sad", "review"] as const;
export type CharacterPose = (typeof CHARACTER_POSES)[number] | (typeof SPRITE_POSES)[number];
export interface CharacterProps { pose?: CharacterPose; animated?: boolean; className?: string }

/** Poses where the default arms are replaced (laptop or clipboard in the paws). */
export const handsBusy = (pose: CharacterPose) => isTypingPose(pose) || pose === "review";

export type Url = (n: string) => string;

export function useIds() {
  const u = useId().replace(/:/g, "");
  return { id: (n: string) => `${n}-${u}`, url: (n: string) => `url(#${n}-${u})` };
}

/** Gradients and filters every character uses. `a` light, `b` mid, `c` dark; `rim` is the bounce light. */
export function Defs({ id, a, b, c, rim = "#BFE3FF" }: { id: (n: string) => string; a: string; b: string; c: string; rim?: string }) {
  return (
    <defs>
      <radialGradient id={id("body")} cx="36%" cy="28%" r="80%" fx="30%" fy="22%">
        <stop offset="0" stopColor={a} /><stop offset=".5" stopColor={b} /><stop offset="1" stopColor={c} />
      </radialGradient>
      <radialGradient id={id("core")} cx="34%" cy="26%" r="82%">
        <stop offset=".58" stopColor="#000" stopOpacity="0" /><stop offset=".88" stopColor="#000" stopOpacity=".22" /><stop offset="1" stopColor="#000" stopOpacity=".1" />
      </radialGradient>
      <radialGradient id={id("rim")} cx="74%" cy="84%" r="40%">
        <stop offset=".55" stopColor={rim} stopOpacity="0" /><stop offset=".95" stopColor={rim} stopOpacity=".55" /><stop offset="1" stopColor={rim} stopOpacity="0" />
      </radialGradient>
      <radialGradient id={id("spec")}><stop offset="0" stopColor="#fff" stopOpacity=".95" /><stop offset="1" stopColor="#fff" stopOpacity="0" /></radialGradient>
      <radialGradient id={id("eye")} cx="40%" cy="30%" r="75%"><stop offset="0" stopColor="#4B3A35" /><stop offset=".6" stopColor="#1A1210" /><stop offset="1" stopColor="#000" /></radialGradient>
      <radialGradient id={id("blush")}><stop offset="0" stopColor="#FF8FA3" stopOpacity=".6" /><stop offset="1" stopColor="#FF8FA3" stopOpacity="0" /></radialGradient>
      <linearGradient id={id("lid")} x1="0" y1="0" x2="0" y2="1"><stop offset="0" stopColor="#4B5563" /><stop offset="1" stopColor="#1F2937" /></linearGradient>
      <linearGradient id={id("deck")} x1="0" y1="0" x2="0" y2="1"><stop offset="0" stopColor="#D1D5DB" /><stop offset="1" stopColor="#9CA3AF" /></linearGradient>
      <radialGradient id={id("ao")}><stop offset="0" stopColor="#000" stopOpacity=".35" /><stop offset="1" stopColor="#000" stopOpacity="0" /></radialGradient>
      <filter id={id("soft")} x="-40%" y="-40%" width="180%" height="180%"><feGaussianBlur stdDeviation="4" /></filter>
      <filter id={id("blur1")} x="-40%" y="-40%" width="180%" height="180%"><feGaussianBlur stdDeviation="1.4" /></filter>
    </defs>
  );
}

/** One lit volume: the same shape drawn with base colour, core shadow and rim light. */
export function Vol({ url, shape, className, transform }: { url: Url; shape: (fill: string) => ReactNode; className?: string | undefined; transform?: string | undefined }) {
  return (
    <g className={className} transform={transform}>
      {shape(url("body"))}
      {shape(url("core"))}
      {shape(url("rim"))}
    </g>
  );
}

/** A soft white glint where the key light hits. */
export function Spec({ url, x, y, rx, ry, r = -20 }: { url: Url; x: number; y: number; rx: number; ry: number; r?: number }) {
  return <ellipse cx={x} cy={y} rx={rx} ry={ry} fill={url("spec")} transform={`rotate(${r} ${x} ${y})`} />;
}

/** Contact shadow under a part where it meets another (ambient occlusion). */
export function AO({ url, x, y, rx, ry }: { url: Url; x: number; y: number; rx: number; ry: number }) {
  return <ellipse cx={x} cy={y} rx={rx} ry={ry} fill={url("ao")} />;
}

/** Two-layer floor shadow: a tight dark core and a wide soft spread. */
export function Shadow({ url, w = 56, y = 189 }: { url: Url; w?: number; y?: number }) {
  return (
    <g>
      <ellipse cx="100" cy={y} rx={w} ry="8" fill="#000" opacity=".16" filter={url("soft")} />
      <ellipse cx="100" cy={y - 1} rx={w * 0.62} ry="4" fill="#000" opacity=".3" filter={url("blur1")} />
    </g>
  );
}

/** Glossy eyes with two highlights; happy arcs when cheering; closed lids when asleep. */
export function Eyes({ url, pose, l, r, y, size = 8, color = "#1a1210" }: { url: Url; pose: CharacterPose; l: number; r: number; y: number; size?: number; color?: string }) {
  if (pose === "cheer") return <g fill="none" stroke={color} strokeWidth="3.6" strokeLinecap="round"><path d={`M${l - 9} ${y + 3} q9 -11 18 0`} /><path d={`M${r - 9} ${y + 3} q9 -11 18 0`} /></g>;
  if (pose === "sleep") return <g fill="none" stroke={color} strokeWidth="3.2" strokeLinecap="round"><path d={`M${l - 9} ${y} q9 8 18 0`} /><path d={`M${r - 9} ${y} q9 8 18 0`} /></g>;
  if (pose === "review") return (
    <g>
      {[l, r].map((x) => (
        <g key={x}>
          <ellipse cx={x} cy={y + 5} rx={size - 0.5} ry={size * 0.8} fill={url("eye")} />
          <circle cx={x - size / 3} cy={y + 3} r={size / 3.2} fill="#fff" />
          <circle cx={x + size / 3} cy={y + 8} r={size / 6} fill="#fff" opacity=".85" />
        </g>
      ))}
    </g>
  );
  if (pose === "sad") return (
    <g>
      {[l, r].map((x) => (
        <g key={x}>
          <ellipse cx={x} cy={y + 3} rx={size - 0.5} ry={size} fill={url("eye")} />
          <circle cx={x - size / 3} cy={y + 5} r={size / 3} fill="#fff" />
          <circle cx={x + size / 3} cy={y + 7} r={size / 6} fill="#fff" opacity=".85" />
        </g>
      ))}
    </g>
  );
  const big = pose === "jump" ? 1.6 : 0;
  const dy = pose === "type" ? 3 : pose === "jump" ? -2 : 0;
  size += big;
  return (
    <g>
      {[l, r].map((x) => (
        <g key={x}>
          <ellipse cx={x} cy={y + dy + 1.5} rx={size + 1} ry={size + 2} fill="#000" opacity=".12" />
          <ellipse cx={x} cy={y + dy} rx={size} ry={size + 1} fill={url("eye")} />
          <circle cx={x - size / 3} cy={y + dy - size / 2.5} r={size / 2.5} fill="#fff" />
          <circle cx={x + size / 2.6} cy={y + dy + size / 2.4} r={size / 6} fill="#fff" opacity=".85" />
        </g>
      ))}
    </g>
  );
}

export function Cheeks({ url, l, r, y, w = 10 }: { url: Url; l: number; r: number; y: number; w?: number }) {
  return <g><ellipse cx={l} cy={y} rx={w} ry={w * 0.6} fill={url("blush")} /><ellipse cx={r} cy={y} rx={w} ry={w * 0.6} fill={url("blush")} /></g>;
}

export function Zzz({ on }: { on: boolean }) {
  if (!on) return null;
  return <g fill="#94A3B8" fontWeight="700" fontFamily="system-ui, sans-serif"><text x="150" y="40" fontSize="16" className="bear-zzz">z</text><text x="162" y="24" fontSize="12" className="bear-zzz bear-zzz-late">z</text></g>;
}

export const bodyAnim = (pose: CharacterPose, animated: boolean) =>
  !animated ? undefined : pose === "cheer" || pose === "jump" ? "bear-jump" : pose === "sleep" || pose === "sad" ? "bear-breathe-slow" : "bear-breathe";

/** Review: both paws hold a clipboard in front of the belly, arms reaching from the shoulders. */
export function ReviewRig({ url, pose, y, shoulder, arm = 11 }: { url: Url; pose: CharacterPose; y: number; shoulder: [number, number]; arm?: number }) {
  if (pose !== "review") return null;
  return (
    <g>
      <Clipboard y={y} />
      {(["l", "r"] as const).map((side) => {
        const sx = side === "l" ? shoulder[0] : 200 - shoulder[0];
        const [px, py] = [side === "l" ? 76 : 124, y + 22];
        const [dx, dy] = [px - sx, py - shoulder[1]];
        const len = Math.hypot(dx, dy) / 2 + 5;
        const angle = (Math.atan2(-dx, dy) * 180) / Math.PI;
        const [cx, cy] = [(sx + px) / 2, (shoulder[1] + py) / 2];
        return (
          <Vol key={side} url={url} shape={(f) => (
            <g>
              <ellipse cx={cx} cy={cy} rx={arm} ry={len} transform={`rotate(${angle} ${cx} ${cy})`} fill={f} />
              <ellipse cx={px} cy={py} rx="9" ry="8" fill={f} />
            </g>
          )} />
        );
      })}
    </g>
  );
}

/**
 * Typing along: the pocket, the arms reaching from the shoulders over the lid to the keys (paws
 * tap while typing, rest while waiting), and the laptop itself, nearest to you.
 */
export function TypingRig({ url, pose, animated, top, pocketY, shoulder, arm = 11, pocket = "#000" }: {
  url: Url; pose: CharacterPose; animated: boolean; top: number; pocketY: number; shoulder: [number, number]; arm?: number; pocket?: string;
}) {
  if (!isTypingPose(pose)) return null;
  const phase = pose === "type" ? "type" : "wait";
  return (
    <g>
      <Pocket y={pocketY} color={pocket} />
      {(["l", "r"] as const).map((side) => {
        const sx = side === "l" ? shoulder[0] : 200 - shoulder[0];
        const [px, py] = [side === "l" ? 86 : 114, top - 2];
        const [dx, dy] = [px - sx, py - shoulder[1]];
        const len = Math.hypot(dx, dy) / 2 + 5;
        const angle = (Math.atan2(-dx, dy) * 180) / Math.PI;
        const [cx, cy] = [(sx + px) / 2, (shoulder[1] + py) / 2];
        const tap = animated && phase === "type" ? (side === "l" ? "pet-tap-l" : "pet-tap-r") : undefined;
        return (
          <Vol key={side} url={url} className={tap} shape={(f) => (
            <g>
              <ellipse cx={cx} cy={cy} rx={arm} ry={len} transform={`rotate(${angle} ${cx} ${cy})`} fill={f} />
              <ellipse cx={px} cy={py} rx="9.5" ry="7.5" fill={f} />
            </g>
          )} />
        );
      })}
      <BackLaptop phase={phase} animated={animated} top={top} pocketY={pocketY + 4} />
    </g>
  );
}

/** Mochi the cat: paws at the keys; curls up asleep. */
export function Mochi({ pose = "idle", animated = false, className }: CharacterProps) {
  const { id, url } = useIds();
  const up = pose === "cheer" || pose === "jump";
  return (
    <svg viewBox="0 0 200 200" role="img" aria-label={`Mochi the cat (${pose})`} className={cn("overflow-visible", className)}>
      <Defs id={id} a="#FFFBF5" b="#F3D2AE" c="#B9875A" rim="#FFE2C4" />
      <Shadow url={url} />
      <g className={bodyAnim(pose, animated)}>
        <Vol url={url} shape={(f) => <path d="M146 172 q44 -8 34 -54 q-4 -16 -14 -8 q8 32 -24 46 z" fill={f} />} />
        <Vol url={url} shape={(f) => <ellipse cx="100" cy="146" rx="50" ry="40" fill={f} />} />
        <ellipse cx="100" cy="154" rx="27" ry="23" fill="#FFFDF9" opacity=".85" />
        <AO url={url} x={100} y={114} rx={42} ry={10} />
        {!handsBusy(pose) && [72, 128].map((x) => (
          <Vol key={x} url={url} shape={(f) => <ellipse cx={x} cy={up ? 96 : 174} rx="13" ry="10" fill={f} />} />
        ))}
        <Vol url={url} shape={(f) => <path d="M56 54 Q58 22 66 16 Q76 24 90 44 Z M144 54 Q142 22 134 16 Q124 24 110 44 Z" fill={f} />} />
        <path d="M64 46 Q66 30 69 26 Q75 32 81 42 Z M136 46 Q134 30 131 26 Q125 32 119 42 Z" fill="#F7A9B4" />
        <Vol url={url} shape={(f) => <ellipse cx="100" cy="78" rx="52" ry="44" fill={f} />} />
        <path d="M86 40 q6 8 0 16 M100 36 v18 M114 40 q-6 8 0 16" stroke="#D99A5E" strokeWidth="4" strokeLinecap="round" fill="none" opacity=".8" />
        <Spec url={url} x={78} y={50} rx={20} ry={9} />
        <Eyes url={url} pose={pose} l={80} r={120} y={76} color="#2B3A1E" />
        <path d="M95.5 91 q4.5 -2 9 0 l-4.5 5 z" fill="#F08A9A" />
        {pose === "jump" ? (
          <g><path d="M90 97 q10 15 20 0 z" fill="#7A2E24" stroke="#3A2620" strokeWidth="2" strokeLinejoin="round" /><ellipse cx="100" cy="104" rx="4.5" ry="2.2" fill="#FF8F86" /></g>
        ) : pose === "sad" ? (
          <path d="M100 96 v3 M90 105 q10 -8 20 0" stroke="#3A2620" strokeWidth="2.4" fill="none" strokeLinecap="round" />
        ) : (
          <path d="M100 96 q-6 6 -12 2 M100 96 q6 6 12 2" stroke="#3A2620" strokeWidth="2.4" fill="none" strokeLinecap="round" />
        )}
        {pose === "sad" && <><SadBrows l={80} r={120} y={58} color="#8A5A2B" /><Tear x={70} y={84} /></>}
        <g stroke="#A07858" strokeWidth="1.4" strokeLinecap="round" opacity=".7"><path d="M70 94 h-18 M70 99 l-16 4 M130 94 h18 M130 99 l16 4" /></g>
        <Cheeks url={url} l={66} r={134} y={92} />
        <TypingRig url={url} pose={pose} animated={animated} top={128} pocketY={162} shoulder={[60, 138]} pocket="#8A5A2B" />
        <ReviewRig url={url} pose={pose} y={122} shoulder={[60, 138]} />
      </g>
      <Zzz on={pose === "sleep"} />
    </svg>
  );
}

/** Pip the penguin: flippers slap the keys. */
export function Pip({ pose = "idle", animated = false, className }: CharacterProps) {
  const { id, url } = useIds();
  const up = pose === "cheer" || pose === "jump";
  return (
    <svg viewBox="0 0 200 200" role="img" aria-label={`Pip the penguin (${pose})`} className={cn("overflow-visible", className)}>
      <Defs id={id} a="#6E80A8" b="#2D3A55" c="#0E1422" rim="#9CC8FF" />
      <Shadow url={url} w={46} />
      <g className={bodyAnim(pose, animated)}>
        {[82, 118].map((x) => <ellipse key={x} cx={x} cy="184" rx="14" ry="6" fill="#F59E0B" stroke="#C2700A" strokeWidth="1" />)}
        <Vol url={url} shape={(f) => <ellipse cx="100" cy="112" rx="56" ry="74" fill={f} />} />
        <ellipse cx="100" cy="128" rx="38" ry="54" fill="#FBFBF7" />
        <ellipse cx="100" cy="128" rx="38" ry="54" fill={url("core")} />
        <ellipse cx="100" cy="80" rx="34" ry="28" fill="#FBFBF7" />
        <ellipse cx="100" cy="80" rx="34" ry="28" fill={url("core")} opacity=".6" />
        <Spec url={url} x={76} y={54} rx={18} ry={9} />
        {!handsBusy(pose) && [{ x: 46, r: up ? 130 : 12 }, { x: 154, r: up ? -130 : -12 }].map((f) => (
          <Vol key={f.x} url={url} transform={`rotate(${f.r} ${f.x} 100)`} shape={(fill) => <ellipse cx={f.x} cy="128" rx="12" ry="34" fill={fill} />} />
        ))}
        <Eyes url={url} pose={pose} l={86} r={114} y={76} size={6} />
        <path d="M91 90 q9 -7 18 0 q-9 11 -18 0 z" fill="#F59E0B" />
        <path d="M93 89.5 q7 -4 12 -1" stroke="#FDE68A" strokeWidth="1.6" fill="none" strokeLinecap="round" />
        <Cheeks url={url} l={74} r={126} y={92} w={8} />
        {pose === "jump" && <path d="M95 92 q5 6 10 0" fill="#B45309" />}
        {pose === "sad" && <><SadBrows l={86} r={114} y={62} color="#1E293B" /><Tear x={78} y={84} /></>}
        <TypingRig url={url} pose={pose} animated={animated} top={132} pocketY={162} shoulder={[52, 112]} arm={11} pocket="#334155" />
        <ReviewRig url={url} pose={pose} y={124} shoulder={[52, 112]} />
      </g>
      <Zzz on={pose === "sleep"} />
    </svg>
  );
}

/** Bolt the robot: its screen-face scrolls code while you type. */
export function Bolt({ pose = "idle", animated = false, className }: CharacterProps) {
  const { id, url } = useIds();
  const up = pose === "cheer" || pose === "jump";
  const face = pose === "sleep" ? "#334155" : "#0B1220";
  return (
    <svg viewBox="0 0 200 200" role="img" aria-label={`Bolt the robot (${pose})`} className={cn("overflow-visible", className)}>
      <Defs id={id} a="#F8FAFC" b="#A3B1C6" c="#4A5568" rim="#A5F3FC" />
      <Shadow url={url} w={50} />
      <g className={bodyAnim(pose, animated)}>
        {[70, 110].map((x) => <Vol key={x} url={url} shape={(f) => <rect x={x} y="168" width="20" height="16" rx="5" fill={f} />} />)}
        <Vol url={url} shape={(f) => <rect x="56" y="112" width="88" height="64" rx="20" fill={f} />} />
        <circle cx="100" cy="142" r="11" fill="#0B1220" opacity=".5" />
        <circle cx="100" cy="142" r="8" fill="#38BDF8" className={animated ? "pet-glow" : undefined} />
        <circle cx="97" cy="139" r="2.5" fill="#fff" opacity=".8" />
        {!handsBusy(pose) && [44, 156].map((x) => (
          <Vol key={x} url={url} shape={(f) => <rect x={x - 8} y={up ? 70 : 120} width="16" height="40" rx="8" fill={f} />} />
        ))}
        <AO url={url} x={100} y={114} rx={40} ry={8} />
        <rect x="98" y="22" width="4" height="14" rx="2" fill="#64748B" />
        <circle cx="100" cy="18" r="7" fill="#F43F5E" className={animated ? "pet-glow" : undefined} />
        <circle cx="98" cy="16" r="2.2" fill="#fff" opacity=".8" />
        <Vol url={url} shape={(f) => <rect x="46" y="34" width="108" height="80" rx="24" fill={f} />} />
        <rect x="57" y="45" width="86" height="58" rx="13" fill="#2A3445" />
        <rect x="58" y="46" width="84" height="56" rx="12" fill={face} />
        <path d="M62 50 h40 l-22 48 h-16 a4 4 0 0 1 -4 -4 z" fill="#fff" opacity=".06" />
        <Spec url={url} x={74} y={40} rx={20} ry={5} r={-6} />
        {pose === "type" ? (
          <g className={animated ? "pet-code" : undefined} stroke="#4ADE80" strokeWidth="3" strokeLinecap="round">
            <path d="M66 58 h20 M90 58 h14 M66 68 h12 M82 68 h26 M66 78 h30 M66 88 h16 M86 88 h22" />
          </g>
        ) : pose === "sleep" ? (
          <g stroke="#94A3B8" strokeWidth="3" strokeLinecap="round"><path d="M74 74 h16 M110 74 h16" /></g>
        ) : pose === "jump" ? (
          <g fill="none" stroke="#4ADE80" strokeWidth="3.4" strokeLinecap="round">
            <circle cx="80" cy="70" r="7" /><circle cx="120" cy="70" r="7" />
            <path d="M92 86 h16 v6 q-8 6 -16 0 z" fill="#4ADE80" />
          </g>
        ) : pose === "sad" ? (
          <g>
            <g fill="#60A5FA"><path d="M72 70 l16 -5 v15 h-16 z" /><path d="M128 70 l-16 -5 v15 h16 z" /></g>
            <path d="M88 96 q12 -8 24 0" fill="none" stroke="#60A5FA" strokeWidth="3" strokeLinecap="round" />
            <rect x="78" y="84" width="4" height="6" rx="1" fill="#93C5FD" />
          </g>
        ) : pose === "review" ? (
          <g fill="#4ADE80">
            <rect x="74" y="74" width="12" height="7" rx="3" /><rect x="114" y="74" width="12" height="7" rx="3" />
            <path d="M90 92 h20" stroke="#4ADE80" strokeWidth="3" strokeLinecap="round" />
            <rect x="60" y="62" width="80" height="2" fill="#4ADE80" opacity=".35" className={animated ? "pet-code" : undefined} />
          </g>
        ) : (
          <g fill="#4ADE80">
            {up ? <g fill="none" stroke="#4ADE80" strokeWidth="4" strokeLinecap="round"><path d="M72 76 q8 -10 16 0 M112 76 q8 -10 16 0" /></g>
              : <><rect x="74" y="64" width="12" height="16" rx="4" /><rect x="114" y="64" width="12" height="16" rx="4" /></>}
            <path d="M88 90 q12 8 24 0" fill="none" stroke="#4ADE80" strokeWidth="3" strokeLinecap="round" />
          </g>
        )}
        <TypingRig url={url} pose={pose} animated={animated} top={128} pocketY={160} shoulder={[50, 126]} arm={9} pocket="#334155" />
        <ReviewRig url={url} pose={pose} y={124} shoulder={[50, 126]} arm={8} />
      </g>
      <Zzz on={pose === "sleep"} />
    </svg>
  );
}

/** Ghosty: a soft gumdrop ghost with a curling tail. Floats, glows while you type, fades a little asleep. */
export function Ghosty({ pose = "idle", animated = false, className }: CharacterProps) {
  const { id, url } = useIds();
  const up = pose === "cheer" || pose === "jump";
  const sheet = "M100 30 C138 30 156 62 156 100 C156 126 160 146 170 162 C160 170 148 167 140 160 C131 172 116 173 107 163 C97 173 81 173 73 162 C63 170 49 167 42 158 C46 142 44 124 44 100 C44 62 62 30 100 30 Z";
  return (
    <svg viewBox="0 0 200 200" role="img" aria-label={`Ghosty (${pose})`} className={cn("overflow-visible", className)} style={{ opacity: pose === "sleep" ? 0.8 : 1 }}>
      <Defs id={id} a="#FFFFFF" b="#F1F0FF" c="#B7B3EC" rim="#C4F1F9" />
      <defs>
        <radialGradient id={id("inner")} cx="50%" cy="45%" r="50%"><stop offset="0" stopColor="#fff" stopOpacity=".9" /><stop offset="1" stopColor="#fff" stopOpacity="0" /></radialGradient>
        <radialGradient id={id("aura")}><stop offset=".5" stopColor="#C7D2FE" stopOpacity=".45" /><stop offset="1" stopColor="#C7D2FE" stopOpacity="0" /></radialGradient>
      </defs>
      <Shadow url={url} w={34} y={190} />
      <g className={animated && pose !== "cheer" ? "pet-float" : bodyAnim(pose, animated)}>
        <ellipse cx="100" cy="104" rx="74" ry="80" fill={url("aura")} opacity={pose === "type" ? 1 : 0.55} className={animated && pose === "type" ? "pet-glow" : undefined} />
        {!handsBusy(pose) && [{ x: 46, r: up ? -60 : 28 }, { x: 154, r: up ? 60 : -28 }].map((a) => (
          <Vol key={a.x} url={url} transform={`rotate(${a.r} ${a.x} 112)`} shape={(f) => <ellipse cx={a.x} cy={up ? 100 : 122} rx="11" ry="15" fill={f} />} />
        ))}
        <Vol url={url} shape={(f) => <path d={sheet} fill={f} />} />
        <ellipse cx="100" cy="98" rx="38" ry="40" fill={url("inner")} opacity=".7" />
        <Spec url={url} x={76} y={52} rx={20} ry={10} />
        <ellipse cx="128" cy="44" rx="5" ry="2.5" fill="#fff" opacity=".7" transform="rotate(30 128 44)" />
        <Eyes url={url} pose={pose} l={83} r={117} y={92} size={8.5} color="#2E2A5B" />
        {up ? (
          <g><path d="M91 108 q9 14 18 0 z" fill="#3B2F6B" /><ellipse cx="100" cy="114" rx="4" ry="2" fill="#FB7185" /></g>
        ) : pose === "sleep" ? (
          <ellipse cx="100" cy="110" rx="3" ry="2.4" fill="#3B2F6B" opacity=".8" />
        ) : pose === "sad" ? (
          <g><path d="M92 114 q8 -7 16 0" stroke="#2E2A5B" strokeWidth="2.6" fill="none" strokeLinecap="round" /><SadBrows l={83} r={117} y={74} color="#2E2A5B" /><Tear x={72} y={100} /></g>
        ) : (
          <path d="M95 108 q5 5 10 0" stroke="#2E2A5B" strokeWidth="2.6" fill="none" strokeLinecap="round" />
        )}
        <Cheeks url={url} l={66} r={134} y={106} w={10} />
        <TypingRig url={url} pose={pose} animated={animated} top={124} pocketY={152} shoulder={[50, 116]} arm={10} pocket="#6366F1" />
        <ReviewRig url={url} pose={pose} y={120} shoulder={[50, 116]} arm={9} />
      </g>
      <Zzz on={pose === "sleep"} />
    </svg>
  );
}

// People.tsx imports this file, so these wrappers keep the cycle harmless (nothing runs at load).
function MiraFront(p: CharacterProps) { return <Mira {...p} />; }
function RajaFront(p: CharacterProps) { return <Raja {...p} />; }

export const CHARACTERS = [
  { key: "mochi", name: "Mochi", what: "the cat", Component: Mochi },
  { key: "pip", name: "Pip", what: "the penguin", Component: Pip },
  { key: "bolt", name: "Bolt", what: "the robot", Component: Bolt },
  { key: "ghosty", name: "Ghosty", what: "the ghost", Component: Ghosty },
  { key: "mira", name: "Mira", what: "the cheerful heroine", Component: MiraFront },
  { key: "raja", name: "Raja", what: "the cool hero", Component: RajaFront },
] as const;
