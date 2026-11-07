// The `voice://…` and `meeting://…` events (API_CONTRACTS §5). The base's events keep using
// `onEvent`; these have their own names. Outside the app a window event stands in, so the
// browser preview, the mock and the tests can send them too.
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AssistantTurn, ExtractedItem, Proposal, TranscriptionJob, TranscriptSegment, VoiceError, VoiceStatus, WakeStatus } from "./generated";

export interface NamedEvents {
  "voice://status": VoiceStatus;
  "voice://transcript": TranscriptSegment;
  "voice://turn": AssistantTurn;
  "voice://proposal": Proposal;
  "voice://error": VoiceError;
  /** Always-on listening changed: open the always-on microphone for `session_id`, or close it. */
  "voice://wake": WakeStatus;
  "meeting://caption": TranscriptSegment;
  "meeting://ticker": ExtractedItem;
  "meeting://job": TranscriptionJob;
}
export type NamedEvent = keyof NamedEvents;

const DOM_PREFIX = "pluto-named:";

export function onNamed<K extends NamedEvent>(name: K, cb: (payload: NamedEvents[K]) => void): () => void {
  if (isTauri()) {
    const pending = listen<NamedEvents[K]>(name, (e) => cb(e.payload));
    return () => void pending.then((off) => off());
  }
  const handler = (e: Event) => cb((e as CustomEvent<NamedEvents[K]>).detail);
  window.addEventListener(DOM_PREFIX + name, handler);
  return () => window.removeEventListener(DOM_PREFIX + name, handler);
}

/** Used by the mock and tests only: the real shell emits these from Rust. */
export function emitNamed<K extends NamedEvent>(name: K, payload: NamedEvents[K]): void {
  // A mock timer (a meeting job step) can fire after a test has finished and its window is gone.
  if (typeof window === "undefined") return;
  window.dispatchEvent(new CustomEvent(DOM_PREFIX + name, { detail: payload }));
}
