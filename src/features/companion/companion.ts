import { ymd } from "@/lib/dates";

/**
 * The pet that ages and remembers (FR-93, FR-94): its birthday is the day you met, it knows your
 * name and birthday if you tell it, and says hello once a day. Kind words only (no-guilt rule):
 * it never gets sick, sad or older faster because of anything you did or didn't do.
 * Stored in preferences under `ui.companion` (included in the data export).
 */
export interface Companion {
  /** What you call the pet. Empty: it answers to "Pluto". */
  petName: string;
  /** The pet's birthday: the day it was first met (YYYY-MM-DD). */
  born: string;
  userName: string;
  /** "MM-DD"; the year is never asked. */
  userBirthday: string | null;
  greet: boolean;
  /** The day the once-a-day greeting was last given. */
  greetedOn: string | null;
}

export const emptyCompanion = (today: string): Companion => ({ born: today, petName: "", userName: "", userBirthday: null, greet: true, greetedOn: null });

const isDay = (v: unknown): v is string => typeof v === "string" && /^\d{4}-\d{2}-\d{2}$/.test(v) && !Number.isNaN(Date.parse(v));
const isMonthDay = (v: unknown): v is string => {
  if (typeof v !== "string" || !/^\d{2}-\d{2}$/.test(v)) return false;
  const [m, d] = v.split("-").map(Number);
  return (m ?? 0) >= 1 && (m ?? 0) <= 12 && (d ?? 0) >= 1 && (d ?? 0) <= 31;
};

export function parseCompanion(raw: unknown, today: string): Companion {
  const base = emptyCompanion(today);
  if (!raw || typeof raw !== "object") return base;
  const r = raw as Partial<Record<keyof Companion, unknown>>;
  return {
    born: isDay(r.born) ? r.born : base.born,
    petName: typeof r.petName === "string" ? r.petName.trim().slice(0, 30) : "",
    userName: typeof r.userName === "string" ? r.userName.trim().slice(0, 40) : "",
    userBirthday: isMonthDay(r.userBirthday) ? r.userBirthday : null,
    greet: r.greet !== false,
    greetedOn: isDay(r.greetedOn) ? r.greetedOn : null,
  };
}

const noon = (d: string) => new Date(`${d}T12:00:00`);

export function ageDays(c: Companion, now: Date): number {
  return Math.max(0, Math.round((noon(ymd(now)).getTime() - noon(c.born).getTime()) / 86_400_000));
}

export function ageLabel(c: Companion, now: Date): string {
  const days = ageDays(c, now);
  const years = Math.floor(days / 365);
  if (years >= 1) return `${years} ${years === 1 ? "year" : "years"} old`;
  if (days === 0) return "born today";
  return `${days} ${days === 1 ? "day" : "days"} old`;
}

/** Whose birthday is it today, if anyone's. */
export function birthdayToday(c: Companion, now: Date): "user" | "pet" | null {
  const today = ymd(now);
  const md = today.slice(5);
  if (c.userBirthday === md) return "user";
  if (c.born.slice(5) === md && c.born !== today) return "pet";
  return null;
}

export function greetingFor(c: Companion, now: Date): string {
  const h = now.getHours();
  const part = h < 12 ? "Good morning" : h < 17 ? "Good afternoon" : "Good evening";
  return c.userName ? `${part}, ${c.userName}!` : `${part}!`;
}

/** What the pet can tell you today besides hello. */
export interface HelloFacts {
  /** Open tasks for today (overdue ones included). */
  open: number;
  overdue: number;
  /** The title of the most overdue task, if any. */
  firstOverdue?: string;
  quote?: { text: string; author: string } | null;
}

const clip = (s: string, n: number) => (s.length > n ? `${s.slice(0, n - 1).trimEnd()}…` : s);

/**
 * The hello as one text bubble (owner 2026-10-26, J3): who it is, then the most useful thing:
 * what is overdue, else what is waiting today, else the day's quote. Kind words only.
 */
export function helloLine(c: Companion, now: Date, facts?: HelloFacts): string {
  const intro = `${greetingFor(c, now)} I'm ${c.petName || "Pluto"}, your assistant.`;
  if (!facts) return greetingFor(c, now);
  if (facts.overdue > 0) {
    const first = facts.firstOverdue ? `, starting with “${clip(facts.firstOverdue, 28)}”` : "";
    return `${intro} ${facts.overdue} ${facts.overdue === 1 ? "task is" : "tasks are"} overdue${first}.`;
  }
  if (facts.open > 0) return `${intro} ${facts.open} ${facts.open === 1 ? "thing" : "things"} to do today.`;
  if (facts.quote?.text) return `${intro} “${clip(facts.quote.text, 90)}”${facts.quote.author ? ` — ${clip(facts.quote.author, 24)}` : ""}`;
  return `${intro} Nothing planned yet. A calm day.`;
}

export interface Greeting { kind: "birthday-user" | "birthday-pet" | "greeting"; text: string; pose: string }

/** The one greeting for today, or null if already given or turned off. Birthdays come first. */
export function todaysGreeting(c: Companion, now: Date, facts?: HelloFacts): Greeting | null {
  if (!c.greet || c.greetedOn === ymd(now)) return null;
  const bday = birthdayToday(c, now);
  if (bday === "user") return { kind: "birthday-user", text: c.userName ? `Happy birthday, ${c.userName}! 🎂` : "Happy birthday! 🎂", pose: "party" };
  if (bday === "pet") {
    const years = Math.floor(ageDays(c, now) / 365);
    return { kind: "birthday-pet", text: `I'm ${years} today! Thanks for having me 🎂`, pose: "party" };
  }
  return { kind: "greeting", text: helloLine(c, now, facts), pose: "wave" };
}
