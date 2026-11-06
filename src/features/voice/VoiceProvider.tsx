import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { FrameSender, PcmPlayer, playbackChannel, pushFrame, startCapture, type CaptureHandle } from "@/audio";
import { onNamed } from "@/ipc/namedEvents";
import { voiceErrorMessage, voiceIpc } from "@/ipc/voiceClient";
import type { Proposal, ProposalResponse, ToolResult, VoiceStatus } from "@/ipc/generated";

/** The audio pieces, injectable so tests don't need a microphone or speakers. */
export interface AudioDeps {
  startCapture: typeof startCapture;
  FrameSender: typeof FrameSender;
  pushFrame: typeof pushFrame;
  createPlayer: (onIdle: () => void) => Pick<PcmPlayer, "enqueue" | "stop">;
}

export const realAudio: AudioDeps = { startCapture, FrameSender, pushFrame, createPlayer: (onIdle) => new PcmPlayer({ onIdle }) };

export interface VoiceApi {
  status: VoiceStatus | null;
  /** The last thing heard, as text. */
  heard: string | null;
  /** The last thing the assistant said, as text. */
  reply: string | null;
  proposal: Proposal | null;
  error: string | null;
  /** The microphone is open for a push-to-talk turn. */
  talking: boolean;
  toggleTalk: () => Promise<void>;
  respond: (response: ProposalResponse) => Promise<ToolResult | null>;
  /** The proposal ran out of time (its card closes). */
  expireProposal: () => void;
  clearError: () => void;
}

const VoiceContext = createContext<VoiceApi | null>(null);

export function useVoice(): VoiceApi {
  const v = useContext(VoiceContext);
  if (!v) throw new Error("useVoice needs a VoiceProvider");
  return v;
}

/**
 * Voice state for the whole app (T-304): runtime status, what was heard and said, the pending
 * proposal, and the push-to-talk button. One set of event listeners, one microphone.
 */
export function VoiceProvider({ children, audio = realAudio }: { children: ReactNode; audio?: AudioDeps }) {
  const [status, setStatus] = useState<VoiceStatus | null>(null);
  const [heard, setHeard] = useState<string | null>(null);
  const [reply, setReply] = useState<string | null>(null);
  const [proposal, setProposal] = useState<Proposal | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [talking, setTalking] = useState(false);
  const mic = useRef<{ handle: CaptureHandle; sender: FrameSender } | null>(null);
  const player = useRef<ReturnType<AudioDeps["createPlayer"]> | null>(null);
  const spoken = useRef(new Set<string>());

  const speak = useCallback(
    (text: string, language: string | null, key: string) => {
      if (spoken.current.has(key)) return;
      spoken.current.add(key);
      try {
        player.current ??= audio.createPlayer(() => undefined);
        const p = player.current;
        void voiceIpc.speak(text, language, playbackChannel((pcm) => p.enqueue(pcm))).catch((e: unknown) => setError(voiceErrorMessage(e)));
      } catch {
        // No audio output here; the reply is still shown as text.
      }
    },
    [audio],
  );

  useEffect(() => {
    let live = true;
    void voiceIpc.status().then((s) => live && setStatus(s)).catch(() => undefined);
    const offs = [
      onNamed("voice://status", setStatus),
      onNamed("voice://transcript", (seg) => setHeard(seg.text)),
      onNamed("voice://turn", (turn) => {
        setReply(turn.reply_text);
        if (turn.reply_text && !turn.result) speak(turn.reply_text, turn.language, turn.interaction_id);
      }),
      onNamed("voice://proposal", (p) => setProposal(p)),
      onNamed("voice://error", (e) => setError(e.message)),
    ];
    return () => {
      live = false;
      offs.forEach((off) => off());
    };
  }, [speak]);

  const closeMic = useCallback(async () => {
    const m = mic.current;
    mic.current = null;
    if (!m) return;
    await m.handle.stop().catch(() => undefined);
    await m.sender.close().catch(() => undefined);
  }, []);

  // Leaving the app releases the microphone.
  useEffect(() => () => void closeMic(), [closeMic]);

  const toggleTalk = useCallback(async () => {
    setError(null);
    if (mic.current || talking) {
      setTalking(false);
      await closeMic();
      try {
        await voiceIpc.pttStop();
      } catch (e) {
        setError(voiceErrorMessage(e));
      }
      return;
    }
    // Barge-in: talking over a reply stops it.
    player.current?.stop();
    void voiceIpc.stopSpeaking().catch(() => undefined);
    let session: string;
    try {
      session = await voiceIpc.pttStart();
    } catch (e) {
      setError(voiceErrorMessage(e));
      return;
    }
    try {
      const sender = new audio.FrameSender(session, audio.pushFrame, { onError: () => setError("Some audio couldn't be sent. Try again.") });
      const handle = await audio.startCapture({
        onFrame: (f) => sender.send(f),
        onEnded: () => {
          setTalking(false);
          setError("The microphone stopped. Check it is plugged in.");
          void closeMic();
          void voiceIpc.cancel().catch(() => undefined);
        },
      });
      mic.current = { handle, sender };
      setTalking(true);
    } catch (e) {
      setError(e instanceof Error && e.name === "CaptureError" ? e.message : "Pluto couldn't use the microphone.");
      void voiceIpc.cancel().catch(() => undefined);
    }
  }, [audio, closeMic, talking]);

  const respond = useCallback(async (response: ProposalResponse) => {
    const p = proposal;
    if (!p) return null;
    setProposal(null);
    try {
      return await voiceIpc.proposalRespond(p.id, response);
    } catch (e) {
      setError(voiceErrorMessage(e));
      return null;
    }
  }, [proposal]);

  const api = useMemo<VoiceApi>(
    () => ({ status, heard, reply, proposal, error, talking, toggleTalk, respond, expireProposal: () => setProposal(null), clearError: () => setError(null) }),
    [status, heard, reply, proposal, error, talking, toggleTalk, respond],
  );
  return <VoiceContext.Provider value={api}>{children}</VoiceContext.Provider>;
}
