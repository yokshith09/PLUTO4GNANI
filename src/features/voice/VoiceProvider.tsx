import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { FrameSender, PcmPlayer, playbackChannel, pushFrame, startCapture, type CaptureHandle } from "@/audio";
import { onNamed } from "@/ipc/namedEvents";
import { createEndpointer, type Endpointer } from "./endpoint";
import { voiceErrorMessage, voiceIpc } from "@/ipc/voiceClient";
import type { Proposal, ProposalResponse, ToolResult, VoiceStatus, WakeClip, WakeStatus } from "@/ipc/generated";

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
  /** Dictation is on: what you say is typed into the app that has the focus. */
  dictating: boolean;
  toggleTalk: () => Promise<void>;
  respond: (response: ProposalResponse) => Promise<ToolResult | null>;
  /** The proposal ran out of time (its card closes). */
  expireProposal: () => void;
  clearError: () => void;
  /** Always-on listening (T-705): taught or not, and listening for "Pluto" or not. */
  wake: WakeStatus | null;
  /** Records one "Pluto" for teaching, for `ms` milliseconds, and returns the shell's verdict. */
  recordClip: (ms: number) => Promise<WakeClip>;
}

const VoiceContext = createContext<VoiceApi | null>(null);

/** The voice API, or null outside a VoiceProvider (a settings test, for example). */
export function useOptionalVoice(): VoiceApi | null {
  return useContext(VoiceContext);
}

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
  const [dictating, setDictating] = useState(false);
  const mic = useRef<{ handle: CaptureHandle; sender: FrameSender; session: string } | null>(null);
  // Set while a session is being opened here, so its own Listening status isn't taken for a
  // hotkey press.
  const opening = useRef(false);
  const player = useRef<ReturnType<AudioDeps["createPlayer"]> | null>(null);
  const spoken = useRef(new Set<string>());
  const lastLanguage = useRef<string | null>(null);
  const [wake, setWake] = useState<WakeStatus | null>(null);
  // Set while a Talk-button turn runs: it ends by itself after a pause (no Send press needed).
  const button = useRef<Endpointer | null>(null);
  const stopByPause = useRef<() => void>(() => undefined);
  const watch = useCallback((f: ArrayBuffer) => {
    if (button.current?.feed(f) === "end") {
      button.current = null;
      stopByPause.current();
    }
  }, []);
  // The always-on microphone (T-705). While it is open it is the only one: Rust sends its audio
  // to whichever turn is listening (wake word, hotkey or button).
  const wakeMic = useRef<{ handle: CaptureHandle; sender: FrameSender; session: string } | null>(null);

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
        lastLanguage.current = turn.language;
        // Speak every reply: proposals, answers that ran at once (lists, "Opening Meetings") and
        // "Done" after a spoken "haan". Skip only the proposal's own turn coming back with its
        // result after approval: the approval says "Done" itself (by voice, or in `respond`).
        if (turn.reply_text && !(turn.proposal && turn.result)) speak(turn.reply_text, turn.language, turn.interaction_id);
      }),
      onNamed("voice://proposal", (p) => setProposal(p)),
      onNamed("voice://dictation", setDictating),
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

  const closeWakeMic = useCallback(async () => {
    const m = wakeMic.current;
    wakeMic.current = null;
    if (!m) return;
    await m.handle.stop().catch(() => undefined);
    await m.sender.close().catch(() => undefined);
  }, []);

  // Always-on listening: the shell says when to listen and for which session (voice://wake).
  useEffect(() => {
    let live = true;
    const apply = (w: WakeStatus) => {
      if (!live) return;
      setWake(w);
      const want = w.session_id;
      if (wakeMic.current?.session === want) return;
      void (async () => {
        await closeWakeMic();
        if (!want) return;
        await closeMic(); // one microphone
        try {
          const sender = new audio.FrameSender(want, audio.pushFrame);
          const handle = await audio.startCapture({
            onFrame: (f) => { sender.send(f); watch(f); },
            onEnded: () => {
              setError("The microphone stopped, so Pluto can't hear its name. Check it is plugged in.");
              void closeWakeMic();
            },
          });
          if (!live) {
            await handle.stop().catch(() => undefined);
            return;
          }
          wakeMic.current = { handle, sender, session: want };
        } catch {
          setError("Pluto couldn't use the microphone for always-on listening.");
        }
      })();
    };
    void voiceIpc.wakeStatus().then(apply).catch(() => undefined);
    const off = onNamed("voice://wake", apply);
    return () => {
      live = false;
      off();
      void closeWakeMic();
    };
  }, [audio, closeMic, closeWakeMic, watch]);

  const recordClip = useCallback(
    async (ms: number): Promise<WakeClip> => {
      const session = await voiceIpc.wakeClipStart();
      const sender = new audio.FrameSender(session, audio.pushFrame);
      const handle = await audio.startCapture({ onFrame: (f) => sender.send(f) });
      await new Promise((r) => setTimeout(r, ms));
      await handle.stop().catch(() => undefined);
      await sender.close().catch(() => undefined);
      return voiceIpc.wakeClipStop(session);
    },
    [audio],
  );

  /** Opens the microphone and sends its frames for `session`. False if it couldn't. */
  const openMic = useCallback(
    async (session: string): Promise<boolean> => {
      try {
        const sender = new audio.FrameSender(session, audio.pushFrame, { onError: () => setError("Some audio couldn't be sent. Try again.") });
        const handle = await audio.startCapture({
          onFrame: (f) => { sender.send(f); watch(f); },
          onEnded: () => {
            setTalking(false);
            setError("The microphone stopped. Check it is plugged in.");
            void closeMic();
            void voiceIpc.cancel().catch(() => undefined);
          },
        });
        mic.current = { handle, sender, session };
        setTalking(true);
        return true;
      } catch (e) {
        setError(e instanceof Error && e.name === "CaptureError" ? e.message : "Pluto couldn't use the microphone.");
        void voiceIpc.cancel().catch(() => undefined);
        return false;
      }
    },
    [audio, closeMic, watch],
  );

  const toggleTalk = useCallback(async () => {
    setError(null);
    if (mic.current || talking) {
      button.current = null;
      setTalking(false);
      await closeMic(); // nothing to close while the always-on microphone serves the turn
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
    opening.current = true;
    try {
      let session: string;
      try {
        session = await voiceIpc.pttStart();
      } catch (e) {
        setError(voiceErrorMessage(e));
        return;
      }
      button.current = createEndpointer();
      if (wakeMic.current) setTalking(true); // the always-on microphone already sends to it
      else if (!(await openMic(session))) button.current = null;
    } finally {
      opening.current = false;
    }
  }, [closeMic, openMic, talking]);

  stopByPause.current = () => void toggleTalk();

  // The global push-to-talk hotkey (T-301) opens Listening in Rust: open the microphone for that
  // session, and close it when Listening ends (key released, the 60 s auto-commit, an error).
  useEffect(
    () =>
      onNamed("voice://status", (s) => {
        if (wakeMic.current) {
          // The always-on microphone serves every turn; just show whether one is listening,
          // and stop a reply that "Pluto" talked over (Rust already stopped sending it).
          if (s.state === "listening") player.current?.stop();
          setTalking(s.state === "listening");
          return;
        }
        if (s.state === "listening" && s.session_id) {
          if (opening.current || mic.current?.session === s.session_id) return;
          player.current?.stop(); // barge-in: Rust already stopped sending the reply
          opening.current = true;
          void (async () => {
            await closeMic();
            await openMic(s.session_id as string);
          })().finally(() => {
            opening.current = false;
          });
        } else if (s.state !== "listening" && mic.current && !opening.current) {
          setTalking(false);
          void closeMic();
        }
      }),
    [closeMic, openMic],
  );

  const respond = useCallback(async (response: ProposalResponse) => {
    const p = proposal;
    if (!p) return null;
    setProposal(null);
    try {
      const result = await voiceIpc.proposalRespond(p.id, response);
      // A click approval has no spoken turn of its own: say what happened.
      if (result?.message) speak(result.message, lastLanguage.current, `${p.id}:result`);
      return result;
    } catch (e) {
      setError(voiceErrorMessage(e));
      return null;
    }
  }, [proposal, speak]);

  const api = useMemo<VoiceApi>(
    () => ({ status, heard, reply, proposal, error, talking, dictating, toggleTalk, respond, expireProposal: () => setProposal(null), clearError: () => setError(null), wake, recordClip }),
    [status, heard, reply, proposal, error, talking, dictating, toggleTalk, respond, wake, recordClip],
  );
  return <VoiceContext.Provider value={api}>{children}</VoiceContext.Provider>;
}
