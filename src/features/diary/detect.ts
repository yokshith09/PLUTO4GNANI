import { ymd } from "@/lib/dates";

/** A line of the diary that reads like something to do (FR-86). Local rules only: nothing leaves the PC. */
export interface Todo { line: number; text: string; title: string; due: string | null }

const WEEKDAYS = ["sunday", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday"];
const MONTHS = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
const TRIGGER = /(^\s*[-*]\s*\[ \])|\b(need to|needs to|have to|has to|must|should|remember to|don't forget to|dont forget to|to do|todo|submit|finish|prepare for)\b/i;
const LEAD = /^\s*(?:[-*]\s*\[ \]\s*)?(?:(?:i|we)\s+)?(?:(?:really|still|just)\s+)?(?:need to|needs to|have to|has to|must|should|remember to|don't forget to|dont forget to|to do:?)(?:\s+|$)/i;
const TIME = /\b(?:by|on|before|until|this|next)?\s*(today|tomorrow|next week|(?:next\s+)?(?:sun|mon|tues?|wednes?|thurs?|fri|satur?)day|\d{1,2}(?:st|nd|rd|th)?\s+(?:jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec)[a-z]*|(?:jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec)[a-z]*\s+\d{1,2}(?:st|nd|rd|th)?)\b/i;

const addDays = (d: Date, n: number) => { const x = new Date(d.getFullYear(), d.getMonth(), d.getDate() + n); return x; };

/** "by Friday", "tomorrow", "next week", "23 Oct" → a day (`YYYY-MM-DD`) on or after `today`, or null. */
export function parseDue(text: string, today: Date): string | null {
  const m = TIME.exec(text);
  const w = m?.[1]?.toLowerCase();
  if (!w) return null;
  if (w === "today") return ymd(today);
  if (w === "tomorrow") return ymd(addDays(today, 1));
  if (w === "next week") return ymd(addDays(today, 7));
  const day = WEEKDAYS.findIndex((n) => n.startsWith(w.replace(/^next\s+/, "").slice(0, 3)));
  if (day >= 0 && /day$/.test(w)) {
    let ahead = (day - today.getDay() + 7) % 7;
    if (ahead === 0 || w.startsWith("next")) ahead += w.startsWith("next") && ahead !== 0 ? 7 : ahead === 0 ? 7 : 0;
    return ymd(addDays(today, ahead));
  }
  const dm = /(\d{1,2})(?:st|nd|rd|th)?\s+([a-z]+)/.exec(w) ?? /([a-z]+)\s+(\d{1,2})/.exec(w);
  if (!dm) return null;
  const [dayStr, monStr] = /^\d/.test(dm[1] ?? "") ? [dm[1], dm[2]] : [dm[2], dm[1]];
  const month = MONTHS.indexOf((monStr ?? "").slice(0, 3));
  const dom = Number(dayStr);
  if (month < 0 || !(dom >= 1 && dom <= 31)) return null;
  let d = new Date(today.getFullYear(), month, dom);
  if (d.getMonth() !== month) return null;
  if (ymd(d) < ymd(today)) d = new Date(today.getFullYear() + 1, month, dom);
  return d.getMonth() === month ? ymd(d) : null;
}

/** The task title from a line: the to-do words and the date phrase come off, the first letter is a capital. */
export function titleOf(line: string): string {
  let t = line.replace(LEAD, "").replace(/^\s*[-*]\s*\[ \]\s*/, "");
  const m = TIME.exec(t);
  if (m && m[0]) t = (t.slice(0, m.index) + t.slice(m.index + m[0].length)).replace(/\s{2,}/g, " ");
  t = t.replace(/[\s,;.!]+$/g, "").replace(/^\s+/, "");
  const title = t.charAt(0).toUpperCase() + t.slice(1);
  return title.slice(0, 300);
}

export function detectTodos(text: string, today: Date): Todo[] {
  const found: Todo[] = [];
  text.split("\n").forEach((raw, line) => {
    if (!TRIGGER.test(raw)) return;
    const title = titleOf(raw);
    if (title.length < 3) return;
    found.push({ line, text: raw, title, due: parseDue(raw, today) });
  });
  return found;
}
