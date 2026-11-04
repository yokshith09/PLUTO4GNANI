// Typed client for the voice commands (API_CONTRACTS §4.2). New commands fail with `VoiceError`,
// not the base's `AppError`: use `voiceErrorMessage` for them. `audio_push` lives in `src/audio`.
import { Channel } from "@tauri-apps/api/core";
import { call } from "./transport";
import type {
  PrivacyLogEntry, ProposalResponse, ToolResult, UsageCounters, VoiceError, VoiceSettings, VoiceSettingsPatch, VoiceStatus,
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
  speak: (text: string, language: string | null, playback: Channel<ArrayBuffer>) =>
    call<null>("voice_speak", { text, language, playback }),
  stopSpeaking: () => call<null>("voice_stop_speaking"),
  usage: () => call<UsageCounters>("voice_usage"),
  privacyLogList: (limit = 100) => call<PrivacyLogEntry[]>("privacy_log_list", { limit }),
  privacyLogClear: () => call<null>("privacy_log_clear"),
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
