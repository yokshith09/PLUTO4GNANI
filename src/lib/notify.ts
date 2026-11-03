import { onPetAction, petSay } from "@/ipc";

/**
 * Every notification Pluto shows goes **above the pet as a bubble** (owner 2026-10-23: nothing
 * pops up inside the dashboard). Errors stay a little longer. An action such as Undo is offered
 * in the bubble: clicking the pet while it shows runs it.
 */
export const TOAST_MS = 5000;
export const ERROR_MS = 8000;
const MAX_TEXT = 80;

type Action = { label: string; onClick: () => void };
type Options = { description?: string; action?: Action };
type Kind = "success" | "info" | "warning" | "error";

const POSE: Record<Kind, string> = { success: "cheer", info: "wave", warning: "wait", error: "worried" };

/** Pending bubble actions by id; the pet window sends the id back when clicked. */
const actions = new Map<string, () => void>();
let listening = false;
let seq = 0;

function listen() {
  if (listening) return;
  listening = true;
  onPetAction((id) => {
    const run = actions.get(id);
    actions.delete(id);
    run?.();
  });
}

/** The bubble text: the message, plus the detail when both fit. */
export function bubbleText(message: string, description?: string): string {
  const both = description ? `${message} — ${description}` : message;
  return both.length <= MAX_TEXT ? both : message;
}

// ---- when the pet is hidden, messages show in the dashboard instead (owner 2026-10-23) ------

export type Notice = { id: number; kind: Kind; text: string; action?: Action };
let petShown = true;
let notices: Notice[] = [];
const watchers = new Set<(n: Notice[]) => void>();
const publish = () => watchers.forEach((w) => w(notices));
/** The dashboard tells this whether the pet is on screen (`pet.visible`). */
export function setPetShown(shown: boolean) { petShown = shown; }
export function watchNotices(w: (n: Notice[]) => void) { watchers.add(w); w(notices); return () => void watchers.delete(w); }
export function dismissNotice(id: number) { notices = notices.filter((n) => n.id !== id); publish(); }
/** At most 3 at a time, newest last. */
export const MAX_NOTICES = 3;

function show(kind: Kind, message: string, o?: Options) {
  const ms = kind === "error" ? ERROR_MS : TOAST_MS;
  if (!petShown) {
    const n: Notice = { id: ++seq, kind, text: bubbleText(message, o?.description), ...(o?.action ? { action: o.action } : {}) };
    notices = [...notices, n].slice(-MAX_NOTICES);
    publish();
    setTimeout(() => dismissNotice(n.id), o?.action ? ms + 3000 : ms);
    return;
  }
  let actionId: string | undefined;
  if (o?.action) {
    listen();
    actionId = `n${++seq}`;
    const id = actionId;
    actions.set(id, o.action.onClick);
    // Forget it once the bubble is gone.
    setTimeout(() => actions.delete(id), ms + 1000);
  }
  void petSay({
    text: bubbleText(message, o?.description),
    pose: POSE[kind],
    ms: o?.action ? ms + 3000 : ms,
    ...(actionId && o?.action ? { actionId, actionLabel: o.action.label } : {}),
  });
}

/**
 * "Completed" feedback. With the pet on screen its own reaction ("Done! 2 left today") already says
 * so and carries the Undo, so a second bubble would only repeat it (owner: double bubble). With
 * the pet hidden, nothing else would tell you, so the dashboard shows it.
 */
export function completed(title: string, undo: () => void) {
  if (petShown) return;
  show("info", `Completed “${title}”`, { action: { label: "Undo", onClick: undo } });
}

export const notify = {
  success: (message: string, o?: Options) => show("success", message, o),
  info: (message: string, o?: Options) => show("info", message, o),
  warning: (message: string, o?: Options) => show("warning", message, o),
  error: (message: string, o?: Options) => show("error", message, o),
};
