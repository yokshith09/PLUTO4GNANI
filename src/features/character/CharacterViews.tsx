import type { ReactNode } from "react";
import { cn } from "@/lib/utils";
import { TURNED_POSES, type TurnedPose } from "@/features/shell/BearViews";
import { AO, Defs, Shadow, Spec, Vol, useIds, type Url } from "./Characters";
import { MIRA, RAJA, Solid, type PersonLook } from "./People";

/**
 * Turned views for the built-in characters (FR-40 back-turn shake), as drawn views and never a
 * mirrored front pose: a back has no face, a three-quarter view foreshortens it.
 * `turn` = three-quarter, facing right. `back` = from behind. `back_look` = from behind, head
 * turned over the shoulder, and the hips shake. Built-in characters only: personal-photo
 * characters have no back and never get this emote (owner decision 2026-10-22).
 */
export { TURNED_POSES };
export interface TurnedProps { pose: TurnedPose; animated?: boolean; className?: string | undefined }

const LABELS: Record<TurnedPose, string> = { turn: "three-quarter view", back: "back view", back_look: "looking back over the shoulder" };

interface FrameProps extends TurnedProps { name: string; shadowW?: number; shadowY?: number; children: (url: Url) => ReactNode; defs: { a: string; b: string; c: string; rim: string } }

/** Shared svg shell: gradients, floor shadow, a gentle breath, the accessible label. */
function Frame({ pose, animated = false, className, name, shadowW, shadowY, defs, children }: FrameProps) {
  const { id, url } = useIds();
  return (
    <svg viewBox="0 0 200 200" role="img" aria-label={`${name} (${LABELS[pose]})`} data-view={pose === "turn" ? "three_quarter" : "back"} className={cn("overflow-visible", className)}>
      <Defs id={id} {...defs} />
      <Shadow url={url} {...(shadowW ? { w: shadowW } : {})} {...(shadowY ? { y: shadowY } : {})} />
      <g className={animated ? "bear-breathe" : undefined}>{children(url)}</g>
    </svg>
  );
}

/** The hips (and tail) rock on look-back; everything else stays still. */
const Hips = ({ on, animated, children }: { on: boolean; animated: boolean; children: ReactNode }) => (
  <g className={on && animated ? "bear-butt-shake" : undefined}>{children}</g>
);

/** A small narrow eye for the far side of a turned face, and a rounder one for the near side. */
function SideEyes({ url, far, near, y, size = 7 }: { url: Url; far: number; near: number; y: number; size?: number }) {
  return (
    <g>
      <ellipse cx={far} cy={y} rx={size * 0.62} ry={size + 1} fill={url("eye")} />
      <circle cx={far - 1} cy={y - 3} r={size / 3} fill="#fff" />
      <ellipse cx={near} cy={y} rx={size} ry={size + 1} fill={url("eye")} />
      <circle cx={near - 2} cy={y - 3} r={size / 2.4} fill="#fff" />
      <circle cx={near + 2.5} cy={y + 3} r={size / 6} fill="#fff" opacity=".85" />
    </g>
  );
}

/** One blush on the near cheek of a turned face. */
const NearCheek = ({ url, x, y, w }: { url: Url; x: number; y: number; w: number }) => <ellipse cx={x} cy={y} rx={w} ry={w * 0.6} fill={url("blush")} />;

// ---------------------------------------------------------------- Mochi

const MOCHI = { a: "#FFFBF5", b: "#F3D2AE", c: "#B9875A", rim: "#FFE2C4" };

function MochiHead3Q({ url, dx = 0, dy = 0 }: { url: Url; dx?: number; dy?: number }) {
  return (
    <g data-part="face" transform={`translate(${dx} ${dy})`}>
      <Vol url={url} shape={(f) => <path d="M62 50 Q62 26 70 20 Q80 28 92 44 Z" fill={f} />} />
      <Vol url={url} shape={(f) => <path d="M150 54 Q152 22 142 14 Q130 22 116 44 Z" fill={f} />} />
      <path d="M142 46 Q144 30 141 24 Q135 30 128 42 Z" fill="#F7A9B4" />
      <Vol url={url} shape={(f) => <ellipse cx="106" cy="78" rx="52" ry="44" fill={f} />} />
      <path d="M92 40 q6 8 0 16 M106 36 v18 M120 40 q-6 8 0 16" stroke="#D99A5E" strokeWidth="4" strokeLinecap="round" fill="none" opacity=".8" />
      <Spec url={url} x={84} y={50} rx={20} ry={9} />
      <ellipse cx="136" cy="94" rx="22" ry="14" fill="#FFFDF9" opacity=".7" />
      <SideEyes url={url} far={92} near={126} y={76} />
      <path d="M143 88 q5 -2 9 1 l-4 5 z" fill="#F08A9A" />
      <path d="M148 94 q-4 6 -11 3" stroke="#3A2620" strokeWidth="2.4" fill="none" strokeLinecap="round" />
      <g stroke="#A07858" strokeWidth="1.4" strokeLinecap="round" opacity=".7"><path d="M154 92 h16 M154 97 l14 4" /></g>
      <NearCheek url={url} x={144} y={90} w={8} />
    </g>
  );
}

function MochiBackHead({ url }: { url: Url }) {
  return (
    <g data-part="back-of-head">
      <Vol url={url} shape={(f) => <path d="M56 54 Q58 22 66 16 Q76 24 90 44 Z M144 54 Q142 22 134 16 Q124 24 110 44 Z" fill={f} />} />
      <path d="M66 44 Q68 30 70 27 Q76 33 82 42 Z M134 44 Q132 30 130 27 Q124 33 118 42 Z" fill="#B9875A" opacity=".5" />
      <Vol url={url} shape={(f) => <ellipse cx="100" cy="78" rx="52" ry="44" fill={f} />} />
      <path d="M86 40 q6 8 0 16 M100 36 v18 M114 40 q-6 8 0 16" stroke="#D99A5E" strokeWidth="4" strokeLinecap="round" fill="none" opacity=".8" />
      <Spec url={url} x={78} y={50} rx={20} ry={9} />
    </g>
  );
}

export function MochiTurned(p: TurnedProps) {
  const animated = p.animated ?? false;
  return (
    <Frame {...p} name="Mochi the cat" defs={MOCHI}>
      {(url) => p.pose === "turn" ? (
        <g>
          <Vol url={url} shape={(f) => <path d="M60 172 q-44 -4 -38 -52 q4 -14 14 -6 q-4 28 28 40 z" fill={f} />} />
          <Vol url={url} shape={(f) => <ellipse cx="96" cy="146" rx="48" ry="40" fill={f} />} />
          <ellipse cx="116" cy="154" rx="24" ry="22" fill="#FFFDF9" opacity=".85" />
          <AO url={url} x={100} y={114} rx={40} ry={10} />
          {[82, 124].map((x) => <Vol key={x} url={url} shape={(f) => <ellipse cx={x} cy="176" rx="13" ry="9" fill={f} />} />)}
          <MochiHead3Q url={url} />
        </g>
      ) : (
        <g>
          {[72, 128].map((x) => <Vol key={x} url={url} shape={(f) => <ellipse cx={x} cy="176" rx="13" ry="9" fill={f} />} />)}
          <Hips on={p.pose === "back_look"} animated={animated}>
            <Vol url={url} shape={(f) => <ellipse cx="100" cy="146" rx="52" ry="40" fill={f} />} />
            <path d="M70 130 q30 12 60 0" stroke="#B9875A" strokeWidth="3" strokeLinecap="round" fill="none" opacity=".3" />
            <g className={p.pose === "back_look" && animated ? "bear-tail-wag" : undefined}>
              <Vol url={url} shape={(f) => <path d="M96 176 Q82 140 100 118 Q112 100 106 84 L118 86 Q126 108 110 128 Q100 146 112 176 Z" fill={f} />} />
            </g>
          </Hips>
          <AO url={url} x={100} y={114} rx={42} ry={10} />
          {p.pose === "back" ? <MochiBackHead url={url} /> : <MochiHead3Q url={url} dx={-6} dy={2} />}
        </g>
      )}
    </Frame>
  );
}

// ---------------------------------------------------------------- Pip

const PIP = { a: "#6E80A8", b: "#2D3A55", c: "#0E1422", rim: "#9CC8FF" };

function PipFeet({ url, xs }: { url: Url; xs: number[] }) {
  return <g>{xs.map((x) => <ellipse key={x} cx={x} cy="184" rx="14" ry="6" fill="#F59E0B" stroke="#C2700A" strokeWidth="1" />)}<AO url={url} x={100} y={184} rx={40} ry={4} /></g>;
}

function PipHead3Q({ url }: { url: Url }) {
  return (
    <g data-part="face">
      <ellipse cx="116" cy="82" rx="26" ry="26" fill="#FBFBF7" />
      <ellipse cx="116" cy="82" rx="26" ry="26" fill={url("core")} opacity=".6" />
      <SideEyes url={url} far={106} near={130} y={76} size={5.5} />
      <path d="M134 86 q14 -6 24 2 q-12 12 -26 6 z" fill="#F59E0B" />
      <path d="M138 87 q9 -4 15 -1" stroke="#FDE68A" strokeWidth="1.6" fill="none" strokeLinecap="round" />
    </g>
  );
}

export function PipTurned(p: TurnedProps) {
  const animated = p.animated ?? false;
  return (
    <Frame {...p} name="Pip the penguin" defs={PIP} shadowW={46}>
      {(url) => p.pose === "turn" ? (
        <g>
          <PipFeet url={url} xs={[84, 120]} />
          <Vol url={url} shape={(f) => <ellipse cx="100" cy="112" rx="54" ry="74" fill={f} />} />
          <ellipse cx="118" cy="128" rx="30" ry="52" fill="#FBFBF7" />
          <ellipse cx="118" cy="128" rx="30" ry="52" fill={url("core")} />
          <Vol url={url} transform="rotate(10 62 100)" shape={(f) => <ellipse cx="60" cy="126" rx="12" ry="34" fill={f} />} />
          <PipHead3Q url={url} />
          <Spec url={url} x={80} y={54} rx={18} ry={9} />
        </g>
      ) : (
        <g>
          <PipFeet url={url} xs={[82, 118]} />
          <Hips on={p.pose === "back_look"} animated={animated}>
            <Vol url={url} shape={(f) => <ellipse cx="100" cy="112" rx="56" ry="74" fill={f} />} />
            <g className={p.pose === "back_look" && animated ? "bear-tail-wag" : undefined}>
              <Vol url={url} shape={(f) => <path d="M86 176 Q100 200 114 176 Z" fill={f} />} />
            </g>
          </Hips>
          <path d="M72 100 q28 14 56 0" stroke="#9CC8FF" strokeWidth="2" fill="none" opacity=".25" strokeLinecap="round" />
          {[{ x: 46, r: 12 }, { x: 154, r: -12 }].map((fl) => (
            <Vol key={fl.x} url={url} transform={`rotate(${fl.r} ${fl.x} 100)`} shape={(f) => <ellipse cx={fl.x} cy="128" rx="12" ry="34" fill={f} />} />
          ))}
          {p.pose === "back" ? (
            <g data-part="back-of-head"><Spec url={url} x={76} y={54} rx={18} ry={9} /></g>
          ) : <PipHead3Q url={url} />}
        </g>
      )}
    </Frame>
  );
}

// ---------------------------------------------------------------- Bolt

const BOLT = { a: "#F8FAFC", b: "#A3B1C6", c: "#4A5568", rim: "#A5F3FC" };

function BoltHead3Q({ url, animated }: { url: Url; animated: boolean }) {
  return (
    <g data-part="face">
      <rect x="98" y="22" width="4" height="14" rx="2" fill="#64748B" />
      <circle cx="100" cy="18" r="7" fill="#F43F5E" className={animated ? "pet-glow" : undefined} />
      {/* The head's side panel shows on the left; the screen is foreshortened on the right. */}
      <Vol url={url} shape={(f) => <rect x="48" y="34" width="104" height="80" rx="24" fill={f} />} />
      <rect x="56" y="46" width="14" height="56" rx="6" fill="#64748B" opacity=".45" />
      <rect x="76" y="45" width="70" height="58" rx="13" fill="#2A3445" />
      <rect x="77" y="46" width="68" height="56" rx="12" fill="#0B1220" />
      <g fill="#4ADE80"><rect x="90" y="64" width="9" height="16" rx="4" /><rect x="120" y="64" width="12" height="16" rx="4" /><path d="M100 90 q10 7 22 0" fill="none" stroke="#4ADE80" strokeWidth="3" strokeLinecap="round" /></g>
      <Spec url={url} x={86} y={40} rx={18} ry={5} r={-6} />
    </g>
  );
}

export function BoltTurned(p: TurnedProps) {
  const animated = p.animated ?? false;
  return (
    <Frame {...p} name="Bolt the robot" defs={BOLT} shadowW={50}>
      {(url) => p.pose === "turn" ? (
        <g>
          {[76, 112].map((x) => <Vol key={x} url={url} shape={(f) => <rect x={x} y="168" width="20" height="16" rx="5" fill={f} />} />)}
          <Vol url={url} shape={(f) => <rect x="62" y="112" width="80" height="64" rx="20" fill={f} />} />
          <circle cx="112" cy="142" r="8" fill="#38BDF8" className={animated ? "pet-glow" : undefined} />
          <Vol url={url} shape={(f) => <rect x="46" y="120" width="14" height="40" rx="7" fill={f} />} />
          <Vol url={url} shape={(f) => <rect x="138" y="120" width="14" height="40" rx="7" fill={f} />} />
          <AO url={url} x={100} y={114} rx={40} ry={8} />
          <BoltHead3Q url={url} animated={animated} />
        </g>
      ) : (
        <g>
          {[70, 110].map((x) => <Vol key={x} url={url} shape={(f) => <rect x={x} y="168" width="20" height="16" rx="5" fill={f} />} />)}
          <Hips on={p.pose === "back_look"} animated={animated}>
            <Vol url={url} shape={(f) => <rect x="56" y="112" width="88" height="64" rx="20" fill={f} />} />
            {/* Back panel: battery pack with a vent and two bolts. */}
            <rect x="72" y="124" width="56" height="40" rx="8" fill="#4A5568" opacity=".55" />
            <path d="M82 134 h36 M82 142 h36 M82 150 h36" stroke="#CBD5E1" strokeWidth="2.4" strokeLinecap="round" opacity=".7" />
            <circle cx="78" cy="160" r="2" fill="#CBD5E1" /><circle cx="122" cy="160" r="2" fill="#CBD5E1" />
            <g className={p.pose === "back_look" && animated ? "bear-tail-wag" : undefined}><rect x="94" y="164" width="12" height="8" rx="3" fill="#64748B" /></g>
          </Hips>
          {[44, 156].map((x) => <Vol key={x} url={url} shape={(f) => <rect x={x - 8} y="120" width="16" height="40" rx="8" fill={f} />} />)}
          <AO url={url} x={100} y={114} rx={40} ry={8} />
          {p.pose === "back" ? (
            <g data-part="back-of-head">
              <rect x="98" y="22" width="4" height="14" rx="2" fill="#64748B" />
              <circle cx="100" cy="18" r="7" fill="#F43F5E" className={animated ? "pet-glow" : undefined} />
              <Vol url={url} shape={(f) => <rect x="46" y="34" width="108" height="80" rx="24" fill={f} />} />
              <path d="M70 58 h60 M70 68 h60 M70 78 h60 M70 88 h60" stroke="#64748B" strokeWidth="3" strokeLinecap="round" opacity=".5" />
              <circle cx="62" cy="48" r="2.4" fill="#64748B" /><circle cx="138" cy="48" r="2.4" fill="#64748B" /><circle cx="62" cy="100" r="2.4" fill="#64748B" /><circle cx="138" cy="100" r="2.4" fill="#64748B" />
              <Spec url={url} x={74} y={40} rx={20} ry={5} r={-6} />
            </g>
          ) : <g transform="translate(-4 2)"><BoltHead3Q url={url} animated={animated} /></g>}
        </g>
      )}
    </Frame>
  );
}

// ---------------------------------------------------------------- Ghosty

const GHOSTY = { a: "#FFFFFF", b: "#F1F0FF", c: "#B7B3EC", rim: "#C4F1F9" };
const SHEET = "M100 30 C138 30 156 62 156 100 C156 126 160 146 170 162 C160 170 148 167 140 160 C131 172 116 173 107 163 C97 173 81 173 73 162 C63 170 49 167 42 158 C46 142 44 124 44 100 C44 62 62 30 100 30 Z";
/** The sheet seen turned: narrower, leaning right, with a longer trailing hem on the left. */
const SHEET_3Q = "M108 30 C142 30 154 62 152 100 C150 126 154 146 164 160 C152 168 142 164 134 158 C126 172 110 172 102 162 C90 174 70 170 56 164 C54 146 52 124 56 100 C58 62 76 30 108 30 Z";

export function GhostyTurned(p: TurnedProps) {
  const animated = p.animated ?? false;
  return (
    <Frame {...p} name="Ghosty" defs={GHOSTY} shadowW={34} shadowY={190}>
      {(url) => (
        <g className={animated ? "pet-float" : undefined}>
          <defs>
            <radialGradient id={url("aura").slice(5, -1)} cx="50%" cy="50%"><stop offset=".5" stopColor="#C7D2FE" stopOpacity=".45" /><stop offset="1" stopColor="#C7D2FE" stopOpacity="0" /></radialGradient>
          </defs>
          <ellipse cx="100" cy="104" rx="74" ry="80" fill={url("aura")} opacity=".55" />
          {p.pose === "turn" ? (
            <g>
              <Vol url={url} transform="rotate(-20 150 120)" shape={(f) => <ellipse cx="150" cy="124" rx="11" ry="15" fill={f} />} />
              <Vol url={url} shape={(f) => <path d={SHEET_3Q} fill={f} />} />
              <Spec url={url} x={84} y={52} rx={18} ry={10} />
              <g data-part="face">
                <SideEyes url={url} far={106} near={132} y={92} size={7.5} />
                <path d="M122 108 q5 5 10 0" stroke="#2E2A5B" strokeWidth="2.6" fill="none" strokeLinecap="round" />
                <NearCheek url={url} x={142} y={106} w={8} />
              </g>
            </g>
          ) : (
            <g>
              <Hips on={p.pose === "back_look"} animated={animated}>
                <Vol url={url} shape={(f) => <path d={SHEET} fill={f} />} />
                {/* A seam and a fold down the back stand in for the face. */}
                <path d="M100 56 C92 80 108 104 100 140" stroke="#B7B3EC" strokeWidth="3" fill="none" strokeLinecap="round" opacity=".5" />
                <path d="M72 120 q28 12 56 0" stroke="#B7B3EC" strokeWidth="2.6" fill="none" strokeLinecap="round" opacity=".4" />
              </Hips>
              <Vol url={url} transform="rotate(28 46 112)" shape={(f) => <ellipse cx="46" cy="122" rx="11" ry="15" fill={f} />} />
              <Vol url={url} transform="rotate(-28 154 112)" shape={(f) => <ellipse cx="154" cy="122" rx="11" ry="15" fill={f} />} />
              <Spec url={url} x={76} y={52} rx={20} ry={10} />
              {p.pose === "back" ? (
                <g data-part="back-of-head" />
              ) : (
                <g data-part="face">
                  <SideEyes url={url} far={122} near={142} y={88} size={6} />
                  <NearCheek url={url} x={146} y={102} w={7} />
                  <path d="M136 104 q4 4 8 0" stroke="#2E2A5B" strokeWidth="2.4" fill="none" strokeLinecap="round" />
                </g>
              )}
            </g>
          )}
        </g>
      )}
    </Frame>
  );
}

// ---------------------------------------------------------------- Mira and Raja

function PersonTurned({ look, kind, ...p }: TurnedProps & { look: PersonLook; kind: "mira" | "raja" }) {
  const animated = p.animated ?? false;
  const mira = kind === "mira";
  const back = p.pose !== "turn";
  return (
    <Frame {...p} name={look.name} defs={look.skin} shadowW={44}>
      {(url) => (
        <g>
          {[84, 116].map((x) => <ellipse key={x} cx={x} cy="183" rx="13" ry="6" fill={look.shoe} />)}
          <Hips on={p.pose === "back_look"} animated={animated}>
            <Solid url={url} color={look.outfit} shape={(f) => <path d={mira ? "M66 130 Q62 178 78 180 L122 180 Q138 178 134 130 Z" : "M62 128 Q56 180 70 182 L130 182 Q144 180 138 128 Z"} fill={f} />} />
            <Solid url={url} color={look.outfit} shape={(f) => <ellipse cx="100" cy="142" rx={back ? (mira ? 34 : 38) : 28} ry="34" fill={f} />} />
            {back && !mira && <path d="M100 112 L100 180" stroke="#0F1013" strokeWidth="2" opacity=".6" />}
            {back && mira && <path d="M70 124 Q100 146 130 122" stroke="#F2B632" strokeWidth="8" fill="none" strokeLinecap="round" />}
          </Hips>
          {[{ x: back ? 58 : 76, r: 8 }, { x: back ? 142 : 124, r: -8 }].map((a) => (
            <g key={a.x} transform={`rotate(${a.r} ${a.x} 118)`}>
              <Solid url={url} color={look.outfit} shape={(f) => <ellipse cx={a.x} cy="140" rx="10" ry="25" fill={f} />} />
              <Vol url={url} shape={(f) => <ellipse cx={a.x} cy="163" rx="8" ry="7.5" fill={f} />} />
            </g>
          ))}
          <AO url={url} x={100} y={114} rx={34} ry={8} />
          <Vol url={url} shape={(f) => <ellipse cx="100" cy="112" rx="14" ry="10" fill={f} />} />
          {back ? (
            <g>
              {mira && [0, 1, 2, 3, 4].map((i) => <Solid key={i} url={url} color={look.hair} shape={(f) => <ellipse cx={100 + (i % 2 ? 3 : -3)} cy={100 + i * 14} rx={9 - i * 0.7} ry="8" fill={f} />} />)}
              <Solid url={url} color={look.hair} shape={(f) => <ellipse cx="100" cy="74" rx="47" ry="43" fill={f} />} />
              {!mira && <path d="M70 52 Q100 34 132 54" stroke={look.hairLight} strokeWidth="3.4" fill="none" strokeLinecap="round" opacity=".7" />}
              <Spec url={url} x={78} y={50} rx={16} ry={8} />
              {p.pose === "back" ? <g data-part="back-of-head" /> : (
                <g data-part="face" transform="translate(30 6)">
                  <ellipse cx="86" cy="80" rx="20" ry="22" fill={look.skin.b} />
                  <ellipse cx="94" cy="80" rx="5" ry="6.5" fill={look.eye} />
                  <circle cx="92.5" cy="77.5" r="1.8" fill="#fff" />
                  <path d="M86 98 q5 4 10 0" stroke="#9A3A3A" strokeWidth="2.2" fill="none" strokeLinecap="round" />
                </g>
              )}
            </g>
          ) : (
            <g>
              <Vol url={url} shape={(f) => <ellipse cx="104" cy="76" rx="42" ry="42" fill={f} />} />
              <Solid url={url} color={look.hair} shape={(f) => <path d={mira ? "M58 76 Q52 32 104 30 Q146 34 142 70 Q132 52 112 52 Q88 60 70 54 Q60 62 58 76 Z" : "M58 74 Q52 26 106 22 Q148 26 144 72 Q142 52 126 42 Q104 28 82 40 Q62 48 58 74 Z"} fill={f} />} />
              <Spec url={url} x={82} y={52} rx={16} ry={8} />
              <g data-part="face">
                <SideEyes url={url} far={100} near={126} y={78} size={mira ? 7.5 : 6.5} />
                {!mira && <rect x="116" y="68" width="24" height="18" rx="7" fill="#0F1013" />}
                <path d="M116 100 q8 6 16 0" stroke="#9A3A3A" strokeWidth="2.4" fill="none" strokeLinecap="round" />
                <NearCheek url={url} x={136} y={92} w={7} />
              </g>
              {mira && <path d="M58 76 Q50 96 56 118" stroke={look.hair} strokeWidth="9" fill="none" strokeLinecap="round" />}
            </g>
          )}
        </g>
      )}
    </Frame>
  );
}
export const MiraTurned = (p: TurnedProps) => <PersonTurned {...p} look={MIRA} kind="mira" />;
export const RajaTurned = (p: TurnedProps) => <PersonTurned {...p} look={RAJA} kind="raja" />;

export const TURNED_VIEWS = {
  mochi: MochiTurned,
  pip: PipTurned,
  bolt: BoltTurned,
  ghosty: GhostyTurned,
  mira: MiraTurned,
  raja: RajaTurned,
} as const;
