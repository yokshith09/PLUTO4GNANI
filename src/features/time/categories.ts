/** Data-viz colours for time categories. Semantic colours only; the copper accent stays for UI. */
const STYLES: Record<string, { tile: string; bar: string; dot: string }> = {
  Coding: { tile: "bg-sky-500/15 text-sky-400", bar: "bg-sky-400", dot: "bg-sky-400" },
  Design: { tile: "bg-fuchsia-500/15 text-fuchsia-400", bar: "bg-fuchsia-400", dot: "bg-fuchsia-400" },
  Research: { tile: "bg-amber-500/15 text-amber-400", bar: "bg-amber-400", dot: "bg-amber-400" },
  Communication: { tile: "bg-emerald-500/15 text-emerald-400", bar: "bg-emerald-400", dot: "bg-emerald-400" },
  Notes: { tile: "bg-violet-500/15 text-violet-400", bar: "bg-violet-400", dot: "bg-violet-400" },
  Entertainment: { tile: "bg-rose-500/15 text-rose-400", bar: "bg-rose-400", dot: "bg-rose-400" },
};
const FALLBACK = { tile: "bg-foreground/10 text-muted-foreground", bar: "bg-foreground/35", dot: "bg-foreground/35" };

export const categoryStyle = (category: string) => STYLES[category] ?? FALLBACK;
export const CATEGORIES = [...Object.keys(STYLES), "Other"] as const;
