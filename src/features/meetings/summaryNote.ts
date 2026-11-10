// The meeting's summary lives in its notes, as the block under a "## Summary" heading (auto mode
// writes it the same way). Writing or regenerating a recap rewrites only that block, so the
// person's own notes around it are never touched.

const HEADING = /^(?:#{1,6}\s*)?summary\s*$/i;

/** Where the summary block is: the heading line, and the paragraph right under it. */
function locate(lines: string[]): { start: number; end: number } | null {
  const start = lines.findIndex((l) => HEADING.test(l.trim()));
  if (start < 0) return null;
  let end = start + 1;
  while (end < lines.length && (lines[end] ?? "").trim() !== "") end += 1;
  return { start, end };
}

/** The summary text in `notes`, or null when there is none. */
export function extractSummary(notes: string): string | null {
  const lines = notes.split("\n");
  const at = locate(lines);
  if (!at) return null;
  const text = lines.slice(at.start + 1, at.end).join("\n").trim();
  return text || null;
}

/** `notes` with the summary block replaced by `text`, or added at the top when there was none. */
export function upsertSummary(notes: string, text: string): string {
  const clean = text.trim().replace(/\n{2,}/g, "\n");
  const block = `## Summary\n${clean}`;
  const lines = notes.split("\n");
  const at = locate(lines);
  if (!at) return notes.trim() ? `${block}\n\n${notes.trimStart()}` : block;
  return [...lines.slice(0, at.start), ...block.split("\n"), ...lines.slice(at.end)].join("\n");
}
