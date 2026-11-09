import { VOICE_SHORTCUTS, showCombo } from "@/features/settings/shortcuts";
import { useSetting } from "@/hooks/useTheme";
import { getWakeWord } from "./WakeSetup";
import { useEffect } from "react";
import { Mic, MicOff, Square, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { VoiceState } from "@/ipc/generated";
import { ProposalCard } from "./ProposalCard";
import { useVoice } from "./VoiceProvider";

export const STATE_LABEL: Record<VoiceState, string> = {
  disabled: "Cloud voice is off", idle: "Ready", listening: "Listening…", thinking: "Thinking…",
  awaiting_approval: "Waiting for your answer", executing: "Working…", speaking: "Speaking…", recording: "Recording a meeting", error: "Something went wrong",
};

/** Dot colour per state (spec S3): blue listening, amber thinking, green speaking, red recording / error. */
const DOT: Record<VoiceState, string> = {
  disabled: "bg-muted-foreground/40", idle: "bg-muted-foreground/60", listening: "bg-blue-500 animate-pulse", thinking: "bg-amber-500",
  awaiting_approval: "bg-amber-500", executing: "bg-amber-500", speaking: "bg-emerald-500", recording: "bg-red-500", error: "bg-red-500",
};

/**
 * The slim voice bar at the bottom of the window (spec S3): state, the last words heard and said,
 * and the mic button. The approval card rises above it when the assistant wants to change something.
 */
export function VoiceBar({ onOpenSettings }: { onOpenSettings: () => void }) {
  const v = useVoice();
  const state: VoiceState = v.error ? "error" : v.talking ? "listening" : (v.status?.state ?? "idle");
  const off = v.status?.state === "disabled";
  const recording = v.status?.state === "recording";
  // Always-on (T-705): when idle, say that Pluto is waiting for its name.
  const [dictationKey] = useSetting("shortcuts.voice_dictation");
  const dictationLabel = v.dictating ? `Dictating: what you say is typed where your cursor is. Press ${showCombo(dictationKey ?? VOICE_SHORTCUTS[1].fallback)} to stop.` : null;
  const idleLabel = state === "idle" && v.wake?.session_id ? `Say “${getWakeWord()}” to start` : STATE_LABEL[state];
  const label = v.error ?? dictationLabel ?? (v.status?.detail && state !== "listening" ? v.status.detail : idleLabel);

  // Something can be called off while Pluto listens, thinks or speaks. Escape does it too.
  const cancellable = state === "listening" || state === "thinking" || state === "speaking" || v.dictating;
  const { cancel } = v;
  useEffect(() => {
    if (!cancellable) return;
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") void cancel(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [cancellable, cancel]);

  return (
    <div className="relative shrink-0 border-t bg-[var(--sidebar)]">
      {v.proposal && (
        <div className="absolute bottom-full left-0 right-0 z-20 flex justify-center px-4 pb-3">
          <ProposalCard key={v.proposal.id} proposal={v.proposal} onRespond={(r) => void v.respond(r)} onExpire={v.expireProposal} />
        </div>
      )}
      <div role="region" aria-label="Voice" className="mx-auto flex max-w-[1280px] items-center gap-3 px-4 py-2 sm:px-6">
        <span aria-hidden className={cn("size-2.5 shrink-0 rounded-full", DOT[state])} />
        <div className="min-w-0 flex-1 text-[13px]" aria-live="polite">
          <p className={cn("truncate font-medium", state === "error" && "text-destructive")} role={v.error ? "alert" : "status"}>{label}</p>
          {(v.heard || v.reply) && !v.error && (
            <p className="truncate text-xs text-muted-foreground">
              {v.heard && <span>You: “{v.heard}”</span>}
              {v.heard && v.reply && " · "}
              {v.reply && <span>Pluto: {v.reply}</span>}
            </p>
          )}
        </div>
        {v.error && <Button size="icon" variant="ghost" aria-label="Dismiss" onClick={v.clearError}><X /></Button>}
        {cancellable && <Button size="sm" variant="outline" aria-label="Cancel" title="Cancel (Esc)" onClick={() => void v.cancel()}><X />Cancel</Button>}
        {off ? (
          <Button size="sm" variant="outline" onClick={onOpenSettings}><MicOff />Turn on in Settings</Button>
        ) : (
          <Button
            size="sm" variant={v.talking ? "destructive" : "default"} disabled={recording || v.status === null}
            aria-label={v.talking ? "Stop and send" : "Push to talk"} aria-pressed={v.talking}
            onClick={() => void v.toggleTalk()}
          >
            {v.talking ? <Square /> : <Mic />}{v.talking ? "Send" : "Talk"}
          </Button>
        )}
      </div>
    </div>
  );
}
