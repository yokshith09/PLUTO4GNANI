import { useCallback, useEffect, useRef, useState } from "react";
import { playbackChannel } from "@/audio";
import { voiceErrorMessage, voiceIpc } from "@/ipc/voiceClient";
import { realAudio, type AudioDeps } from "./VoiceProvider";

/**
 * Speak a piece of text with Timbre (a test tone while Cloud voice is off), stop it, and fall
 * silent when the page that asked for it goes away. One player is kept for the page's life.
 * `error` holds the last failure in words; the text stays on screen either way.
 */
export function useSpeakText(audio: AudioDeps = realAudio) {
  const [speaking, setSpeaking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const player = useRef<ReturnType<AudioDeps["createPlayer"]> | null>(null);

  const stop = useCallback(() => {
    player.current?.stop();
    void voiceIpc.stopSpeaking().catch(() => undefined);
    setSpeaking(false);
  }, []);

  // Leaving the page silences it.
  useEffect(() => () => { player.current?.stop(); void voiceIpc.stopSpeaking().catch(() => undefined); }, []);

  const speak = useCallback(async (text: string, language: string) => {
    setError(null);
    setSpeaking(true);
    try {
      player.current ??= audio.createPlayer(() => undefined);
      const p = player.current;
      await voiceIpc.speak(text, language, playbackChannel((pcm) => p.enqueue(pcm)));
    } catch (e) {
      setError(voiceErrorMessage(e));
    } finally {
      setSpeaking(false);
    }
  }, [audio]);

  return { speaking, error, speak, stop, clearError: () => setError(null) };
}
