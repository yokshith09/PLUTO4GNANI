import { invoke, isTauri } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import type { AppError, PlutoEvent } from "./types";

type Handler = (cmd: string, args: Record<string, unknown> | undefined) => Promise<unknown>;
type Subscribe = (cb: (e: PlutoEvent) => void) => () => void;

type Mock = { call: Handler; subscribe: Subscribe };
// One shared promise: concurrent first calls must get the same in-memory shell, not one each.
let mock: Promise<Mock | null> | null = null;

/** In `vite dev` outside Tauri the UI runs against an in-memory fake, so the screens can be
 *  designed and screenshotted without the Rust shell. It is dead code in production builds (unless built with VITE_MOCK for design previews). */
function loadMock(): Promise<Mock | null> {
  mock ??= import.meta.env.DEV || import.meta.env.VITE_MOCK ? import("./mock").then((m) => m.createMock()) : Promise.resolve(null);
  return mock;
}

export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri()) return (await invoke<T>(cmd, args)) as T;
  const m = await loadMock();
  if (!m) throw new Error("Pluto's shell isn't available.");
  return (await m.call(cmd, args)) as T;
}

export function onEvent(cb: (e: PlutoEvent) => void): () => void {
  if (isTauri()) {
    const pending = listen<PlutoEvent>("pluto://event", (e) => cb(e.payload));
    return () => void pending.then((off) => off());
  }
  let off: (() => void) | undefined;
  let cancelled = false;
  void loadMock().then((m) => {
    if (m && !cancelled) off = m.subscribe(cb);
  });
  return () => {
    cancelled = true;
    off?.();
  };
}

export function errorMessage(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) return String((e as AppError).message);
  return typeof e === "string" ? e : "Something went wrong. Your data is safe.";
}

/**
 * Requests from the shell to the dashboard (`new-note`, `new-task`, `focus`, `tasks`, `settings`,
 * `page:<name>`): global shortcuts and the pet's right-click menu. Outside the app a
 * `pluto-command` window event stands in, so the browser preview and tests can send one too.
 */
export function onCommand(cb: (command: string) => void): () => void {
  if (!isTauri()) {
    const on = (e: Event) => cb((e as CustomEvent<string>).detail);
    window.addEventListener("pluto-command", on);
    return () => window.removeEventListener("pluto-command", on);
  }
  const pending = listen<string>("pluto://command", (e) => cb(e.payload));
  return () => void pending.then((off) => off());
}

/**
 * Typing started (`true`) or stopped (`false`) anywhere on the computer; only sent to the pet
 * window while `pet.typing` is on. Outside the app (browser preview, tests) a `pluto-typing`
 * window event with `detail: boolean` stands in for it.
 */
export function onTyping(cb: (typing: boolean) => void): () => void {
  if (isTauri()) {
    const pending = listen<boolean>("pet://typing", (e) => cb(e.payload));
    return () => void pending.then((off) => off());
  }
  const handler = (e: Event) => cb(Boolean((e as CustomEvent<boolean>).detail));
  window.addEventListener("pluto-typing", handler);
  return () => window.removeEventListener("pluto-typing", handler);
}

/**
 * Whether a text box has the focus in the app in front, while `pet.typing` is on. Sent only
 * when it changes and only where the OS can tell (never on macOS without Accessibility).
 * Outside the app a `pluto-textbox` window event with `detail: boolean` stands in for it.
 */
export function onTextBox(cb: (focused: boolean) => void): () => void {
  if (isTauri()) {
    const pending = listen<boolean>("pet://textbox", (e) => cb(e.payload));
    return () => void pending.then((off) => off());
  }
  const handler = (e: Event) => cb(Boolean((e as CustomEvent<boolean>).detail));
  window.addEventListener("pluto-textbox", handler);
  return () => window.removeEventListener("pluto-textbox", handler);
}

/** Something the dashboard asks the pet to say above its head (FR-43), e.g. the routine reminder. */
export interface PetSay { text: string; pose?: string; ms?: number; actionId?: string; actionLabel?: string }

/**
 * Dashboard → pet window message (FR-85/FR-43). In the app it is a Tauri event to every window;
 * outside it (browser preview, tests) a `pluto-pet-say` window event stands in for it.
 */
export async function petSay(say: PetSay): Promise<void> {
  if (isTauri()) return emit("pet://say", say);
  window.dispatchEvent(new CustomEvent<PetSay>("pluto-pet-say", { detail: say }));
}

export function onPetSay(cb: (say: PetSay) => void): () => void {
  if (isTauri()) {
    const pending = listen<PetSay>("pet://say", (e) => cb(e.payload));
    return () => void pending.then((off) => off());
  }
  const handler = (e: Event) => cb((e as CustomEvent<PetSay>).detail);
  window.addEventListener("pluto-pet-say", handler);
  return () => window.removeEventListener("pluto-pet-say", handler);
}

/**
 * The computer woke up (the shell noticed a long gap between its once-a-minute looks).
 * Reminders and the daily greeting re-check then (bug S1). Outside the app a `pluto-wake`
 * window event stands in for it.
 */
export function onWake(cb: () => void): () => void {
  if (isTauri()) {
    const pending = listen("pluto://wake", () => cb());
    return () => void pending.then((off) => off());
  }
  window.addEventListener("pluto-wake", cb);
  return () => window.removeEventListener("pluto-wake", cb);
}

/** Pet → dashboard: the bubble's action (e.g. Undo) was clicked. Outside the app a
 *  `pluto-pet-action` window event stands in for it. */
export async function petAction(id: string): Promise<void> {
  if (isTauri()) return emit("pet://action", id);
  window.dispatchEvent(new CustomEvent<string>("pluto-pet-action", { detail: id }));
}

export function onPetAction(cb: (id: string) => void): () => void {
  if (isTauri()) {
    const pending = listen<string>("pet://action", (e) => cb(e.payload));
    return () => void pending.then((off) => off());
  }
  const handler = (e: Event) => cb((e as CustomEvent<string>).detail);
  window.addEventListener("pluto-pet-action", handler);
  return () => window.removeEventListener("pluto-pet-action", handler);
}

/**
 * The pet's right-click menu asking the pet window itself to do something (`nap`: sleep or wake).
 * Outside the app a `pluto-pet-menu` window event stands in.
 */
export function onPetMenu(cb: (what: string) => void): () => void {
  if (!isTauri()) {
    const on = (e: Event) => cb((e as CustomEvent<string>).detail);
    window.addEventListener("pluto-pet-menu", on);
    return () => window.removeEventListener("pluto-pet-menu", on);
  }
  const pending = listen<string>("pet://menu", (e) => cb(e.payload));
  return () => void pending.then((off) => off());
}

/**
 * The shell has put the pet back in its own square (after a drag, a move to another screen, or a
 * change of size): the page lays its window out again. Outside the app a `pluto-pet-placed` window
 * event stands in for it.
 */
export function onPetPlaced(cb: () => void): () => void {
  if (isTauri()) {
    const pending = listen("pet://placed", () => cb());
    return () => void pending.then((off) => off());
  }
  const handler = () => cb();
  window.addEventListener("pluto-pet-placed", handler);
  return () => window.removeEventListener("pluto-pet-placed", handler);
}

// ---- Google Meet auto mode: messages between the shell, the pet and the page -------------------

/** A question on the pet with buttons (`options`; the first is the main one). */
export interface PetAsk { id: number; title: string; detail: string; options: string[]; /** "proposal": answers go to the voice assistant's approval, not Meet recording. */ kind?: "proposal" }
/** The recording auto mode is running. */
export interface AutoRec { session_id: string; meeting_id: string; started_at: number }

/** One Tauri event, or a window event of the same name outside the app (browser preview, tests). */
function listenBoth<T>(tauriName: string, windowName: string, cb: (payload: T) => void): () => void {
  if (isTauri()) {
    const pending = listen<T>(tauriName, (e) => cb(e.payload));
    return () => void pending.then((off) => off());
  }
  const handler = (e: Event) => cb((e as CustomEvent<T>).detail);
  window.addEventListener(windowName, handler);
  return () => window.removeEventListener(windowName, handler);
}

/** A question for the person (`ask`), and the shell taking it back (`done`, the question's id). */
export function onPetAsk(ask: (a: PetAsk) => void, done: (id: number) => void): () => void {
  const a = listenBoth<PetAsk>("pet://ask", "pluto-pet-ask", ask);
  const d = listenBoth<number>("pet://ask-done", "pluto-pet-ask-done", done);
  return () => { a(); d(); };
}

/** The recording started or ended (`null`). */
export function onAutoRecording(cb: (rec: AutoRec | null) => void): () => void {
  return listenBoth<AutoRec | null>("pet://recording", "pluto-pet-recording", cb);
}

/** The page should open (`on`) or close the microphone for a recording session. */
export function onAutoCapture(cb: (m: { session_id: string; on: boolean }) => void): () => void {
  return listenBoth<{ session_id: string; on: boolean }>("meeting://auto-capture", "pluto-auto-capture", cb);
}

/** The shell asks the page to show Settings (`voice` = the Voice & privacy tab). */
export function onOpenSettings(cb: (tab: "general" | "voice") => void): () => void {
  return listenBoth<"general" | "voice">("app://open-settings", "pluto-open-settings", cb);
}
