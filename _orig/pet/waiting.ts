import type { WaitingChat } from "@/ipc";

/** A chat stays on the list for half an hour without a word (the shell drops it too). */
export const WAIT_EXPIRES_MS = 30 * 60 * 1000;

/** What is still waiting: nothing older than half an hour, longest-waiting first. */
export function stillWaiting(chats: readonly WaitingChat[], now: number): WaitingChat[] {
  return chats.filter((c) => now - c.since < WAIT_EXPIRES_MS).sort((a, b) => a.since - b.since);
}

/** "now", "4m", "1h" for how long a chat has waited. */
export function waited(since: number, now: number): string {
  const m = Math.max(0, Math.floor((now - since) / 60_000));
  return m < 1 ? "now" : m < 60 ? `${m}m` : `${Math.floor(m / 60)}h`;
}

/** The badge's words for a screen reader: "2 chats waiting for you, 1 asks permission". */
export function badgeLabel(chats: readonly WaitingChat[]): string {
  const perm = chats.filter((c) => c.kind === "permission").length;
  const base = `${chats.length} ${chats.length === 1 ? "chat" : "chats"} waiting for you`;
  return perm ? `${base}, ${perm} ${perm === 1 ? "asks" : "ask"} permission` : base;
}
