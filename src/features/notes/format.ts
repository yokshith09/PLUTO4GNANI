/**
 * The formatting buttons in the note editor (owner 2026-10-25: numbering, bold, font). Notes are
 * Markdown, so each button writes the Markdown for it; Preview shows the result. Pure functions:
 * text and selection in, text and new selection out.
 */
export interface Edit { text: string; start: number; end: number }

/** Bold (`**`) or italic (`_`) around the selection; again on a wrapped selection removes it. */
export function wrap(e: Edit, mark: string): Edit {
  const { text, start, end } = e;
  const before = text.slice(start - mark.length, start);
  const after = text.slice(end, end + mark.length);
  if (before === mark && after === mark) {
    return { text: text.slice(0, start - mark.length) + text.slice(start, end) + text.slice(end + mark.length), start: start - mark.length, end: end - mark.length };
  }
  return { text: text.slice(0, start) + mark + text.slice(start, end) + mark + text.slice(end), start: start + mark.length, end: end + mark.length };
}

export type LineKind = "number" | "bullet" | "check" | "heading";

const PREFIX = /^(\d+\.\s|[-*]\s\[[ xX]\]\s|[-*]\s|#{1,6}\s)/;

/** Turn every line the selection touches into a numbered, bulleted, checklist or heading line. Again removes it. */
export function lines(e: Edit, kind: LineKind): Edit {
  const { text, start, end } = e;
  const from = text.lastIndexOf("\n", start - 1) + 1;
  const nl = text.indexOf("\n", end);
  const to = nl === -1 ? text.length : nl;
  const block = text.slice(from, to).split("\n");
  const want = (i: number) => (kind === "number" ? `${i + 1}. ` : kind === "bullet" ? "- " : kind === "check" ? "- [ ] " : "## ");
  const has = (l: string) =>
    kind === "number" ? /^\d+\.\s/.test(l) : kind === "check" ? /^[-*]\s\[[ xX]\]\s/.test(l) : kind === "bullet" ? /^[-*]\s(?!\[[ xX]\]\s)/.test(l) : /^#{1,6}\s/.test(l);
  const filled = block.filter((l) => l !== "");
  // An empty line alone is not "already a list": the button must give it a marker, not take one away.
  const allHave = filled.length > 0 && filled.every(has);
  let n = 0;
  const out = block
    .map((l) => (allHave ? l.replace(PREFIX, "") : l === "" && filled.length > 0 ? l : want(n++) + l.replace(PREFIX, "")))
    .join("\n");
  const next = text.slice(0, from) + out + text.slice(to);
  // With nothing selected, the caret goes to the end of the line so the next key adds to it; leaving
  // the new marker selected made the first letter typed replace it (owner 2026-10-31: "just square
  // brackets"). A selection of several lines stays selected, so the same button can undo it.
  const caret = start === end ? from + out.length : undefined;
  return { text: next, start: caret ?? from, end: caret ?? from + out.length };
}

const TASK_LINE = /^(\s*[-*]\s)\[([ xX])\](?=\s|$)/;

/** Tick or untick the checklist item on `line` (1-based, as the Markdown parser counts). Anything else is left alone. */
export function toggleCheck(text: string, line: number): string {
  const all = text.split("\n");
  const l = all[line - 1];
  const m = l === undefined ? null : TASK_LINE.exec(l);
  if (!m || l === undefined) return text;
  all[line - 1] = `${m[1]}[${m[2] === " " ? "x" : " "}]${l.slice(m[0].length)}`;
  return all.join("\n");
}

const LIST_LINE = /^(\s*)(?:(\d+)\.|([-*]))\s(?:\[([ xX])\]\s)?/;

/**
 * Enter at the end of a list line starts the next item (a new unticked box for a checklist, the next
 * number for a numbered list); Enter on an empty item ends the list. `null` = not a list line, so
 * Enter does what it normally does.
 */
export function continueList(e: Edit): Edit | null {
  const { text, start, end } = e;
  if (start !== end) return null;
  const from = text.lastIndexOf("\n", start - 1) + 1;
  const nl = text.indexOf("\n", start);
  if ((nl === -1 ? text.length : nl) !== start) return null; // the caret is mid-line
  const line = text.slice(from, start);
  const m = LIST_LINE.exec(line);
  if (!m) return null;
  if (line.length === m[0].length) {
    return { text: text.slice(0, from) + text.slice(start), start: from, end: from }; // empty item: end the list
  }
  const [, indent = "", num, , box] = m;
  const prefix = `${indent}${num ? `${Number(num) + 1}. ` : `${line.trimStart()[0]} `}${box === undefined ? "" : "[ ] "}`;
  const next = text.slice(0, start) + "\n" + prefix + text.slice(start);
  return { text: next, start: start + 1 + prefix.length, end: start + 1 + prefix.length };
}

/** Plain-text view of a note excerpt for cards: checklist markers drawn as boxes. */
export function withBoxes(s: string): string {
  return s.replace(/(^|\n)(\s*)[-*]\s\[([ xX])\](?=\s|$)/g, (_m, nl: string, ws: string, c: string) => `${nl}${ws}${c === " " ? "☐" : "☑"}`);
}

export type NoteFont = "sans" | "serif" | "mono";
export const NOTE_FONTS: { value: NoteFont; label: string }[] = [
  { value: "sans", label: "Sans" },
  { value: "serif", label: "Serif" },
  { value: "mono", label: "Mono" },
];
export const fontClass = (f: NoteFont | null | undefined) =>
  f === "serif" ? "font-serif" : f === "mono" ? "font-mono" : "";
