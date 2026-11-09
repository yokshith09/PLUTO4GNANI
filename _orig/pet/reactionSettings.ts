import { BEAR_POSES, type BearPose } from "@/features/shell/Bear";
import type { PlutoEvent } from "@/ipc";
import { reactionFor, type Reaction, type ReactionContext } from "./reactions";

/**
 * What the pet does when something happens: Pluto's defaults, which the user can change (owner
 * 2026-10-26, J3). Each moment can be turned off, play another pose, or say other words. The
 * words may use `{title}` (the task or reminder) and `{left}` (tasks left today).
 * Stored in preferences under `ui.reactions`, so it is part of the data export.
 */
export const REACTION_KEYS = ["task-added", "task-started", "task-done", "all-done", "task-deleted", "reminder", "focus-done"] as const;
export type ReactionKey = (typeof REACTION_KEYS)[number];

export interface ReactionSetting {
  enabled: boolean;
  /** Empty: the default words. */
  text: string;
  /** Empty: the default pose. */
  pose: BearPose | "";
}
export type ReactionSettings = Partial<Record<ReactionKey, ReactionSetting>>;

export const REACTION_INFO: Record<ReactionKey, { label: string; hint: string; text: string; tokens: string[] }> = {
  "task-added": { label: "A task is added", hint: "Right after you add one.", text: "Got it: {title}", tokens: ["title"] },
  "task-started": { label: "A task is started", hint: "When you press Start.", text: "On it: {title}", tokens: ["title"] },
  "task-done": { label: "A task is done", hint: "When you complete one and more are left.", text: "Done! {left} left today", tokens: ["left"] },
  "all-done": { label: "Everything is done", hint: "When the last task for today is done.", text: "All done for today! 🎉", tokens: [] },
  "task-deleted": { label: "A task is deleted", hint: "When a task goes to the Bin.", text: "Tossed it", tokens: [] },
  reminder: { label: "A reminder is due", hint: "Your own reminders only.", text: "Reminder: {title}", tokens: ["title"] },
  "focus-done": { label: "Focus time ends", hint: "When a focus session finishes. The break offer stays.", text: "", tokens: [] },
};

const MAX_TEXT = 80;

export function parseReactionSettings(raw: unknown): ReactionSettings {
  const out: ReactionSettings = {};
  if (!raw || typeof raw !== "object") return out;
  for (const key of REACTION_KEYS) {
    const v = (raw as Record<string, unknown>)[key];
    if (!v || typeof v !== "object") continue;
    const r = v as Record<string, unknown>;
    out[key] = {
      enabled: r.enabled !== false,
      text: typeof r.text === "string" ? r.text.trim().slice(0, MAX_TEXT) : "",
      pose: typeof r.pose === "string" && (BEAR_POSES as readonly string[]).includes(r.pose) ? (r.pose as BearPose) : "",
    };
  }
  return out;
}

/** Fill `{title}` and `{left}` in; a token with no value disappears. */
export function fillText(template: string, vars: Record<string, string>): string {
  return template.replace(/\{(\w+)\}/g, (_, name: string) => vars[name] ?? "").replace(/\s+/g, " ").trim();
}

const short = (s: string, n = 26) => (s.length > n ? `${s.slice(0, n - 1).trimEnd()}…` : s);

/** Which moment an event is, and the values its words can use. */
export function momentOf(e: PlutoEvent, ctx: ReactionContext): { key: ReactionKey; vars: Record<string, string> } | null {
  const ev = e as unknown as Record<string, unknown>;
  switch (e.type) {
    case "TaskCreated": return { key: "task-added", vars: { title: `“${short(String((ev.task as { title?: string } | undefined)?.title ?? ""))}”` } };
    case "TaskDeleted": return { key: "task-deleted", vars: {} };
    case "ReminderDue": return { key: "reminder", vars: { title: `“${short(String((ev.reminder as { title?: string } | undefined)?.title ?? ""))}”` } };
    case "FocusEnded": return { key: "focus-done", vars: {} };
    case "TaskStatusChanged": {
      const id = String(ev.id ?? "");
      if (ev.to === "COMPLETED") {
        const left = ctx.openToday.filter((r) => r.task.id !== id).length;
        return left === 0 ? { key: "all-done", vars: {} } : { key: "task-done", vars: { left: String(left) } };
      }
      if (ev.to === "IN_PROGRESS") {
        const row = ctx.openToday.find((r) => r.task.id === id);
        return { key: "task-started", vars: { title: row ? `“${short(row.task.title)}”` : "" } };
      }
      return null;
    }
    default: return null;
  }
}

/**
 * The reaction for an event, in three layers: Pluto's default, then the pet's own pose for that
 * moment (`pluto.json`), then the user's own choice on the Character page. Null when turned off.
 */
export function reactionWithSettings(e: PlutoEvent, ctx: ReactionContext, settings: ReactionSettings, packPoses: Partial<Record<string, string>> = {}): Reaction | null {
  const base = reactionFor(e, ctx);
  if (!base) return null;
  const moment = momentOf(e, ctx);
  if (!moment) return base;
  const s = settings[moment.key];
  if (s && !s.enabled) return null;
  const own = packPoses[moment.key];
  const packPose = own && (BEAR_POSES as readonly string[]).includes(own) ? (own as BearPose) : "";
  const pose = s?.pose || packPose;
  const text = s?.text ? fillText(s.text, moment.vars) : "";
  return { ...base, ...(pose ? { pose } : {}), ...(text ? { bubble: text } : {}) };
}
