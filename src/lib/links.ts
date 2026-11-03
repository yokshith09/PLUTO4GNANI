/** Turn pasted text into a safe, openable link. Returns null if it isn't one. */
export function normalizeUrl(input: string): { url: string; label: string } | null {
  const raw = input.trim();
  if (!raw || raw.length > 2048 || [...raw].some((c) => c.charCodeAt(0) <= 32)) return null;
  const withScheme = /^[a-z][a-z0-9+.-]*:/i.test(raw) ? raw : `https://${raw}`;
  let u: URL;
  try {
    u = new URL(withScheme);
  } catch {
    return null;
  }
  if (!["http:", "https:", "mailto:"].includes(u.protocol)) return null;
  if (u.protocol !== "mailto:" && !u.hostname.includes(".")) return null;
  const label = u.protocol === "mailto:" ? u.pathname : u.hostname.replace(/^www\./, "");
  return { url: u.toString(), label };
}

/** Every link in a pasted block (one per line or space separated). */
export function parseLinks(text: string): { url: string; label: string }[] {
  const seen = new Set<string>();
  return text
    .split(/[\s,]+/)
    .map(normalizeUrl)
    .filter((x): x is { url: string; label: string } => x !== null && !seen.has(x.url) && !!seen.add(x.url));
}
