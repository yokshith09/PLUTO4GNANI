/**
 * Quote of the day (owner 2026-10-25). Built-in quotes are short lines from public-domain works
 * (authors who died long ago, sources named in the docs); people can add, edit and remove their
 * own. Nothing is fetched. Stored in preferences under `ui.quotes`.
 */
export interface Quote { text: string; author: string }
export interface QuoteSettings { show: boolean; builtIns: boolean; custom: Quote[] }

export const BUILT_IN_QUOTES: readonly Quote[] = [
  { text: "Well begun is half done.", author: "Aristotle" },
  { text: "The beginning is the most important part of the work.", author: "Plato" },
  { text: "A journey of a thousand miles begins with a single step.", author: "Lao Tzu" },
  { text: "It is not that we have a short time to live, but that we waste a lot of it.", author: "Seneca" },
  { text: "While we are postponing, life speeds by.", author: "Seneca" },
  { text: "Begin at once to live, and count each separate day as a separate life.", author: "Seneca" },
  { text: "First say to yourself what you would be; and then do what you have to do.", author: "Epictetus" },
  { text: "No man is free who is not master of himself.", author: "Epictetus" },
  { text: "The impediment to action advances action. What stands in the way becomes the way.", author: "Marcus Aurelius" },
  { text: "Lost time is never found again.", author: "Benjamin Franklin" },
  { text: "Well done is better than well said.", author: "Benjamin Franklin" },
  { text: "One today is worth two tomorrows.", author: "Benjamin Franklin" },
  { text: "Knowing is not enough; we must apply. Willing is not enough; we must do.", author: "Goethe" },
  { text: "Simplify, simplify.", author: "Henry David Thoreau" },
  { text: "It is not enough to be busy; so are the ants. The question is: what are we busy about?", author: "Henry David Thoreau" },
  { text: "Nothing great was ever achieved without enthusiasm.", author: "Ralph Waldo Emerson" },
  { text: "Write it on your heart that every day is the best day in the year.", author: "Ralph Waldo Emerson" },
  { text: "He who has a why to live can bear almost any how.", author: "Friedrich Nietzsche" },
  { text: "Rest is not idleness.", author: "John Lubbock" },
  { text: "The best way out is always through.", author: "Robert Frost" },
  { text: "Do what you can, with what you have, where you are.", author: "Theodore Roosevelt" },
];

export const TEXT_MAX = 300;
export const AUTHOR_MAX = 80;

export const defaultQuotes = (): QuoteSettings => ({ show: true, builtIns: true, custom: [] });

const clean = (q: unknown): Quote | null => {
  if (!q || typeof q !== "object") return null;
  const r = q as Partial<Record<keyof Quote, unknown>>;
  const text = typeof r.text === "string" ? r.text.trim().slice(0, TEXT_MAX) : "";
  const author = typeof r.author === "string" ? r.author.trim().slice(0, AUTHOR_MAX) : "";
  return text ? { text, author } : null;
};

export function parseQuotes(raw: unknown): QuoteSettings {
  const base = defaultQuotes();
  if (!raw || typeof raw !== "object") return base;
  const r = raw as Partial<Record<keyof QuoteSettings, unknown>>;
  return {
    show: r.show !== false,
    builtIns: r.builtIns !== false,
    custom: Array.isArray(r.custom) ? r.custom.map(clean).filter((q): q is Quote => q !== null).slice(0, 200) : [],
  };
}

/** Your own quotes first in the pool, then the built-in ones if they are on. */
export const pool = (s: QuoteSettings): Quote[] => [...s.custom, ...(s.builtIns ? BUILT_IN_QUOTES : [])];

/** The same quote all day, a different one the next day. */
export function quoteFor(s: QuoteSettings, day: Date): Quote | null {
  const all = pool(s);
  if (!s.show || all.length === 0) return null;
  const n = Math.floor(new Date(day.getFullYear(), day.getMonth(), day.getDate()).getTime() / 86_400_000);
  return all[((n % all.length) + all.length) % all.length] ?? null;
}
