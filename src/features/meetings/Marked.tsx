/** `text` with the words that were searched for marked. A search word matches the start of a word. */
export function Marked({ text, query }: { text: string; query: string }) {
  const words = query.toLowerCase().split(/\s+/).map((w) => w.replace(/[^\p{L}\p{N}]/gu, "")).filter(Boolean);
  if (!words.length) return <>{text}</>;
  const re = new RegExp(`(?<![\\p{L}\\p{N}])(${words.map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|")})`, "giu");
  return <>{text.split(re).map((part, i) => (i % 2 === 1 ? <mark key={i} className="rounded bg-primary/20 px-0.5 text-foreground">{part}</mark> : part))}</>;
}

