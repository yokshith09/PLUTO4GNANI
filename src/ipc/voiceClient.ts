// Typed client for the voice commands (API_CONTRACTS §4.2). New commands fail with `VoiceError`,
// not the base's `AppError`: use `voiceErrorMessage` for them. `audio_push` lives in `src/audio`.
import { Channel } from "@tauri-apps/api/core";
import { call } from "./transport";
import type { PlaybackMessage } from "@/audio/ipc";
import type {
  AssistantTurn, PrivacyLogEntry, ProposalResponse, ToolResult, UsageCounters, VoiceError, VoiceSettings, VoiceSettingsPatch, VoiceStatus,
  WakeClip, WakeStatus,
} from "./generated";

export type KeyAccount = "gnani" | "gemini" | "groq";

export const voiceIpc = {
  settingsGet: () => call<VoiceSettings>("voice_settings_get"),
  settingsSet: (patch: VoiceSettingsPatch) => call<VoiceSettings>("voice_settings_set", { patch }),
  /** The secret goes straight to the OS keychain and is never returned. */
  keySet: (account: KeyAccount, secret: string) => call<null>("voice_key_set", { account, secret }),
  keyClear: (account: KeyAccount) => call<null>("voice_key_clear", { account }),
  status: () => call<VoiceStatus>("voice_status"),
  /** Returns the session id; mic frames then go to `audio_push` with it. */
  pttStart: () => call<string>("voice_ptt_start"),
  pttStop: () => call<null>("voice_ptt_stop"),
  cancel: () => call<null>("voice_cancel"),
  proposalRespond: (proposalId: string, response: ProposalResponse) =>
    call<ToolResult>("voice_proposal_respond", { proposalId, response }),
  /** PCM16 24 kHz chunks arrive on `playback`; `src/audio` plays them. */
  speak: (text: string, language: string | null, playback: Channel<PlaybackMessage>) =>
    call<null>("voice_speak", { text, language, playback }),
  stopSpeaking: () => call<null>("voice_stop_speaking"),
  usage: () => call<UsageCounters>("voice_usage"),
  /** The assistant's turns, newest first. */
  history: (limit = 50) => call<AssistantTurn[]>("voice_history", { limit }),
  privacyLogList: (limit = 100) => call<PrivacyLogEntry[]>("privacy_log_list", { limit }),
  privacyLogClear: () => call<null>("privacy_log_clear"),
  // Always-on listening (T-705). Teaching: begin → (clipStart, frames, clipStop) × n → finish.
  wakeStatus: () => call<WakeStatus>("voice_wake_status"),
  wakeTeachBegin: () => call<WakeStatus>("voice_wake_teach_begin"),
  /** Returns the session; the microphone's frames go to `audio_push` with it. */
  wakeClipStart: () => call<string>("voice_wake_clip_start"),
  wakeClipStop: (sessionId: string) => call<WakeClip>("voice_wake_clip_stop", { sessionId }),
  wakeTeachFinish: () => call<WakeStatus>("voice_wake_teach_finish"),
  wakeForget: () => call<WakeStatus>("voice_wake_forget"),
} as const;

export function isVoiceError(e: unknown): e is VoiceError {
  return !!e && typeof e === "object" && "code" in e && "retryable" in e && "message" in e;
}

/** Text for a toast or banner. Never includes transcript content. */
export function voiceErrorMessage(e: unknown): string {
  if (isVoiceError(e)) return e.message;
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  return "Something went wrong. Your data is safe.";
}
