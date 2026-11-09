import { useEffect, useRef } from "react";
import { ipc, onAutoCapture } from "@/ipc";
import { notify } from "@/lib/notify";
import { realAudio, type AudioDeps } from "@/features/voice/VoiceProvider";
import type { CaptureHandle, FrameSender } from "@/audio";

/**
 * Google Meet auto mode, page side: when the shell starts recording a call, this opens the
 * microphone for that session and streams it (the same path the Record button uses); when the
 * shell says the call ended, it stops. Renders nothing. If the microphone cannot be opened it says
 * so, and tells the shell to end the recording.
 */
export function AutoCapture({ audio = realAudio }: { audio?: AudioDeps }) {
  const open = useRef<{ session: string; handle: CaptureHandle; sender: FrameSender } | null>(null);

  useEffect(() => {
    const close = async () => {
      const m = open.current;
      open.current = null;
      if (!m) return;
      await m.handle.stop().catch(() => undefined);
      await m.sender.close().catch(() => undefined);
    };
    const off = onAutoCapture((m) => {
      if (!m.on) {
        if (open.current?.session === m.session_id) void close();
        return;
      }
      if (open.current?.session === m.session_id) return;
      void (async () => {
        await close();
        try {
          const sender = new audio.FrameSender(m.session_id, audio.pushFrame, { onError: () => undefined });
          const handle = await audio.startCapture({
            onFrame: (f) => sender.send(f),
            onEnded: () => { void close(); notify.error("The microphone stopped, so the recording of the call ended."); void ipc.autoMeetingCaptureFailed("it stopped").catch(() => undefined); },
          });
          open.current = { session: m.session_id, handle, sender };
        } catch (e) {
          const why = e instanceof Error ? e.message : "it could not be opened";
          notify.error(`Pluto could not use the microphone: ${why}`);
          void ipc.autoMeetingCaptureFailed(why).catch(() => undefined);
        }
      })();
    });
    return () => { off(); void close(); };
  }, [audio]);

  return null;
}
