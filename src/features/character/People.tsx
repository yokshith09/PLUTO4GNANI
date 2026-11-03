import type { ReactNode } from "react";
import { cn } from "@/lib/utils";
import { AO, Cheeks, Defs, Eyes, ReviewRig, Shadow, Spec, TypingRig, Vol, Zzz, bodyAnim, handsBusy, useIds, type CharacterProps, type Url } from "./Characters";
import { SadBrows, Tear } from "./SpriteProps";

/**
 * Mira and Raja: two original human characters (owner brief 2026-10-28). Not based on any real
 * person: a cheerful heroine in a red and gold saree with a braid and gold jhumkas, and a cool hero in a long jacket
 * with sunglasses. Same lit-volume style and the same poses as the other characters.
 */
export interface PersonLook {
  name: string;
  skin: { a: string; b: string; c: string; rim: string };
  /** Flat colours for the parts that are not skin. */
  hair: string; hairLight: string; outfit: string; trim: string; shoe: string; eye: string;
}

/** A flat-coloured volume: the same core shadow and rim light as `Vol`, on any colour. */
export function Solid({ url, color, shape, transform }: { url: Url; color: string; shape: (fill: string) => ReactNode; transform?: string }) {
  return (
    <g transform={transform}>
      {shape(color)}
      {shape(url("core"))}
      {shape(url("rim"))}
    </g>
  );
}

export const MIRA: PersonLook = {
  name: "Mira",
  skin: { a: "#F8D9BC", b: "#E8B48A", c: "#B7794F", rim: "#FFE3C9" },
  hair: "#2A1A16", hairLight: "#5A3A2E", outfit: "#B3202A", trim: "#E8B23A", shoe: "#8A3A1E", eye: "#2A1A16",
};
export const RAJA: PersonLook = {
  name: "Raja",
  skin: { a: "#F3CFAE", b: "#DDA075", c: "#A56A44", rim: "#FFDDBF" },
  hair: "#15110F", hairLight: "#43342C", outfit: "#23262E", trim: "#C7CBD4", shoe: "#0F1013", eye: "#15110F",
};

function Mouth({ pose, y = 98 }: { pose: string; y?: number }) {
  if (pose === "cheer" || pose === "jump") return <g><path d={`M90 ${y} q10 15 20 0 z`} fill="#8A2E2E" /><path d={`M93 ${y + 8} q7 5 14 0`} stroke="#E8707A" strokeWidth="2" fill="none" strokeLinecap="round" /></g>;
  if (pose === "sleep") return <ellipse cx="100" cy={y + 2} rx="3" ry="2.4" fill="#8A2E2E" opacity=".75" />;
  if (pose === "sad") return <path d={`M92 ${y + 6} q8 -7 16 0`} stroke="#7A2E2E" strokeWidth="2.6" fill="none" strokeLinecap="round" />;
  return <path d={`M92 ${y} q8 8 16 0`} stroke="#9A3A3A" strokeWidth="2.6" fill="none" strokeLinecap="round" />;
}

function MiraHair({ url, look, front }: { url: Url; look: PersonLook; front: boolean }) {
  const c = look.hair;
  return (
    <g>
      {!front ? null : (
        <g>
          <Solid url={url} color={c} shape={(f) => <path d="M54 80 Q50 36 100 32 Q150 36 146 80 Q140 56 124 52 Q104 62 76 52 Q60 58 54 80 Z" fill={f} />} />
          <path d="M64 62 Q86 44 118 50" stroke={look.hairLight} strokeWidth="3" fill="none" strokeLinecap="round" opacity=".6" />
        </g>
      )}
    </g>
  );
}

/** The braid over the right shoulder with a gold tip. */
function Braid({ url, look, swing = 0 }: { url: Url; look: PersonLook; swing?: number }) {
  return (
    <g transform={`rotate(${swing} 138 104)`}>
      {[0, 1, 2, 3, 4].map((i) => (
        <Solid key={i} url={url} color={look.hair} shape={(f) => <ellipse cx={142 + (i % 2 ? 2 : -2)} cy={108 + i * 11} rx={8 - i * 0.6} ry="7" fill={f} />} />
      ))}
      <ellipse cx="143" cy="168" rx="4" ry="5" fill={look.trim} />
    </g>
  );
}

function Person({ look, kind, pose = "idle", animated = false, className }: CharacterProps & { look: PersonLook; kind: "mira" | "raja" }) {
  const { id, url } = useIds();
  const up = pose === "cheer" || pose === "jump";
  const mira = kind === "mira";
  const glassesUp = pose === "cheer" || pose === "sad" || pose === "sleep";
  return (
    <svg viewBox="0 0 200 200" role="img" aria-label={`${look.name} (${pose})`} className={cn("overflow-visible", className)}>
      <Defs id={id} {...look.skin} />
      <Shadow url={url} w={44} />
      <g className={bodyAnim(pose, animated)}>
        {/* shoes */}
        {[82, 118].map((x) => <ellipse key={x} cx={x} cy="183" rx="14" ry="6.5" fill={look.shoe} />)}
        {/* long jacket tails or tunic hem behind the body */}
        <Solid url={url} color={look.outfit} shape={(f) => <path d={mira ? "M64 130 Q60 178 76 180 L124 180 Q140 178 136 130 Z" : "M62 128 Q56 180 70 182 L130 182 Q144 180 138 128 Z"} fill={f} />} />
        {/* torso */}
        <Solid url={url} color={look.outfit} shape={(f) => <ellipse cx="100" cy="142" rx={mira ? 34 : 38} ry="34" fill={f} />} />
        {mira ? (
          <g>
            {/* scarf: a gold-edged sash across the chest */}
            <path d="M68 118 Q100 140 134 112 L140 124 Q104 156 64 132 Z" fill="#F2B632" opacity=".95" />
            <path d="M68 118 Q100 140 134 112" stroke="#C98A12" strokeWidth="1.6" fill="none" />
            <path d="M120 140 Q132 160 124 176" stroke="#F2B632" strokeWidth="9" fill="none" strokeLinecap="round" />
          </g>
        ) : (
          <g>
            <path d="M84 108 L100 150 L116 108 Z" fill="#E9E6E0" />
            <path d="M100 150 L100 180" stroke="#0F1013" strokeWidth="2" />
            <path d="M80 108 L90 134 L72 124 Z M120 108 L110 134 L128 124 Z" fill="#2E323C" />
            <path d="M74 150 q26 8 52 0" stroke="#0F1013" strokeWidth="2.4" fill="none" opacity=".5" />
          </g>
        )}
        <AO url={url} x={100} y={114} rx={36} ry={9} />
        {/* arms */}
        {!handsBusy(pose) && [{ x: 58, r: up ? 150 : 8 }, { x: 142, r: up ? -150 : -8 }].map((a) => (
          <g key={a.x} transform={`rotate(${a.r} ${a.x} 118)`}>
            <Solid url={url} color={look.outfit} shape={(f) => <ellipse cx={a.x} cy="140" rx="11" ry="26" fill={f} />} />
            <Vol url={url} shape={(f) => <ellipse cx={a.x} cy="164" rx="8.5" ry="8" fill={f} />} />
          </g>
        ))}
        {mira && <Braid url={url} look={look} swing={animated && pose === "idle" ? 4 : 0} />}
        {/* neck and head */}
        <Vol url={url} shape={(f) => <ellipse cx="100" cy="112" rx="14" ry="10" fill={f} />} />
        {!mira && <Solid url={url} color={look.hair} shape={(f) => <path d="M56 78 Q50 30 100 26 Q152 30 144 78 Q148 52 128 40 Q110 30 86 38 Q62 46 56 78 Z" fill={f} />} />}
        <Vol url={url} shape={(f) => <ellipse cx="100" cy="76" rx="46" ry="42" fill={f} />} />
        <Spec url={url} x={78} y={52} rx={18} ry={9} />
        {mira ? (
          <g>
            <MiraHair url={url} look={look} front />
            <circle cx="100" cy="58" r="3.2" fill="#C41E2A" />
            <path d="M100 34 L100 54" stroke={look.skin.b} strokeWidth="2.6" strokeLinecap="round" />
            {/* a gold jhumka on each ear */}
            {[52, 148].map((x) => <g key={x}><circle cx={x} cy="94" r="3.4" fill={look.trim} /><path d={`M${x - 4} 97 q4 12 8 0 z`} fill={look.trim} stroke="#B07F12" strokeWidth=".8" /></g>)}
          </g>
        ) : (
          <g>
            {/* the swept-back quiff */}
            <Solid url={url} color={look.hair} shape={(f) => <path d="M54 76 Q48 28 98 20 Q150 22 146 76 Q146 54 130 44 Q110 28 80 40 Q60 48 54 76 Z M70 30 Q96 6 128 28 Q108 20 70 30 Z" fill={f} />} />
            <path d="M66 44 Q92 26 126 40" stroke={look.hairLight} strokeWidth="3" fill="none" strokeLinecap="round" opacity=".7" />
          </g>
        )}
        {/* ears */}
        {[52, 148].map((x) => <ellipse key={x} cx={x} cy="80" rx="6" ry="9" fill={look.skin.b} />)}
        {/* brows and eyes */}
        {pose !== "sad" && (
          <g stroke={look.hair} strokeWidth={mira ? 2.6 : 3.4} strokeLinecap="round" fill="none">
            <path d={`M70 ${mira ? 64 : 66} q10 -${mira ? 5 : 3} 20 0`} /><path d={`M110 ${mira ? 64 : 66} q10 -${mira ? 5 : 3} 20 0`} />
          </g>
        )}
        <Eyes url={url} pose={pose} l={80} r={120} y={78} size={mira ? 8.5 : 7} color={look.eye} />
        {mira && pose !== "sleep" && pose !== "cheer" && <g stroke={look.hair} strokeWidth="2.4" strokeLinecap="round"><path d="M70 74 l-4 -3 M130 74 l4 -3" /></g>}
        {!mira && (
          glassesUp
            ? <g><rect x="70" y="42" width="24" height="12" rx="5" fill="#0F1013" /><rect x="106" y="42" width="24" height="12" rx="5" fill="#0F1013" /><path d="M94 47 h12" stroke="#0F1013" strokeWidth="2.6" /></g>
            : <g>
                <rect x="64" y="68" width="30" height="20" rx="8" fill="#0F1013" /><rect x="106" y="68" width="30" height="20" rx="8" fill="#0F1013" />
                <path d="M94 76 h12" stroke="#0F1013" strokeWidth="3" />
                <path d="M70 72 l10 -2 M112 72 l10 -2" stroke="#fff" strokeWidth="2.4" strokeLinecap="round" opacity=".55" />
              </g>
        )}
        <path d="M101 84 q-2 4 0 6 q2 -.5 3 -1.5" stroke="#B7794F" strokeWidth="1.6" fill="none" strokeLinecap="round" opacity=".45" />
        <Mouth pose={pose} y={mira ? 98 : 100} />
        <Cheeks url={url} l={68} r={132} y={92} w={9} />
        {pose === "sad" && <><SadBrows l={80} r={120} y={64} color={look.hair} /><Tear x={72} y={92} /></>}
        <TypingRig url={url} pose={pose} animated={animated} top={132} pocketY={166} shoulder={[58, 128]} arm={11} pocket={look.outfit} />
        <ReviewRig url={url} pose={pose} y={124} shoulder={[58, 128]} />
      </g>
      <Zzz on={pose === "sleep"} />
    </svg>
  );
}

export const Mira = (p: CharacterProps) => <Person {...p} look={MIRA} kind="mira" />;
export const Raja = (p: CharacterProps) => <Person {...p} look={RAJA} kind="raja" />;
