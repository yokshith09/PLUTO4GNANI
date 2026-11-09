import { useCallback, useEffect, useRef, useState } from "react";
import { Circle, Mic, Pause, Play, Square } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import type { CaptureHandle, FrameSender } from "@/audio";
import { realAudio, type AudioDeps } from "@/features/voice/VoiceProvider";
import type { RecordingSession } from "@/ipc/generated";
import { meetingAi } from "@/ipc/meetingAi";
import { voiceErrorMessage } from "@/ipc/voiceClient";

type Phase = "form" | "starting" | "recording" | "paused" | "stopping";

export const clock = (ms: number) => {
  const t = Math.floor(ms / 1000);
  return `${Math.floor(t / 60)}:${String(t % 60).padStart(2, "0")}`;
};

/**
 * Record a meeting with the microphone (T-207). Consent is required before anything starts. While it
 * runs: the time recorded, Pause / Resume (a resume starts a new part), Stop. Stopping saves the
 * audio and queues the transcription; the new meeting opens.
 */
export function RecordDialog({ open, onClose, onRecorded, audio = realAudio }: {
  open: boolean;
  onClose: () => void;
  onRecorded: (meetingId: string) => void;
  audio?: AudioDeps;
}) {
  const [title, setTitle] = useState("");
  const [people, setPeople] = useState("");
  const [consent, setConsent] = useState(false);
  const [phase, setPhase] = useState<Phase>("form");
  const [elapsed, setElapsed] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const session = useRef<RecordingSession | null>(null);
  const mic = useRef<{ handle: CaptureHandle; sender: FrameSender } | null>(null);
  const lastTick = useRef(0);
  const live = phase === "recording";

  // Time recorded counts only while recording, not while paused.
  useEffect(() => {
    if (!live) return;
    lastTick.current = Date.now();
    const t = setInterval(() => {
      const now = Date.now();
      setElapsed((e) => e + (now - lastTick.current));
      lastTick.current = now;
    }, 250);
    return () => clearInterval(t);
  }, [live]);

  const closeMic = useCallback(async () => {
    const m = mic.current;
    mic.current = null;
    if (!m) return;
    await m.handle.stop().catch(() => undefined);
    await m.sender.close().catch(() => undefined);
  }, []);
  useEffect(() => () => void closeMic(), [closeMic]);

  const openMic = useCallback(async (sessionId: string) => {
    const sender = new audio.FrameSender(sessionId, audio.pushFrame, { onError: () => setError("Some audio couldn't be saved. Check the recording is still running.") });
    const handle = await audio.startCapture({
      onFrame: (f) => sender.send(f),
      // A lost microphone pauses the recording; resuming starts a new part (spec A3.4).
      onEnded: () => {
        setError("The microphone stopped. Check it is plugged in, then press Resume.");
        void closeMic().then(() => meetingAi.recPause(sessionId)).catch(() => undefined);
        setPhase("paused");
      },
    });
    mic.current = { handle, sender };
  }, [audio, closeMic]);

  const fail = (e: unknown) => setError(e instanceof Error && e.name === "CaptureError" ? e.message : voiceErrorMessage(e));

  const start = async () => {
    setError(null);
    setPhase("starting");
    try {
      const s = await meetingAi.recStart(title.trim(), people.split(",").map((p) => p.trim()).filter(Boolean), false, consent);
      session.current = s;
      try {
        await openMic(s.id);
      } catch (e) {
        // No microphone: end the empty recording so nothing is left half-started.
        await meetingAi.recStop(s.id).catch(() => undefined);
        session.current = null;
        throw e;
      }
      setElapsed(0);
      setPhase("recording");
    } catch (e) {
      fail(e);
      setPhase("form");
    }
  };

  const pause = async () => {
    const s = session.current;
    if (!s) return;
    setError(null);
    await closeMic();
    try {
      await meetingAi.recPause(s.id);
      setPhase("paused");
    } catch (e) { fail(e); }
  };

  const resume = async () => {
    const s = session.current;
    if (!s) return;
    setError(null);
    try {
      await meetingAi.recResume(s.id);
      await openMic(s.id);
      setPhase("recording");
    } catch (e) {
      await meetingAi.recPause(s.id).catch(() => undefined);
      fail(e);
      setPhase("paused");
    }
  };

  const stop = async () => {
    const s = session.current;
    if (!s) return;
    setError(null);
    setPhase("stopping");
    await closeMic();
    try {
      const done = await meetingAi.recStop(s.id);
      session.current = null;
      reset();
      onRecorded(done.meeting_id);
    } catch (e) {
      fail(e);
      setPhase("paused");
    }
  };

  const reset = () => { setTitle(""); setPeople(""); setConsent(false); setElapsed(0); setPhase("form"); setError(null); };
  const busy = phase !== "form";

  return (
    // While a recording exists the dialog cannot be dismissed by accident: Stop is the way out.
    <Dialog open={open} onOpenChange={(o) => { if (!o && !busy) { reset(); onClose(); } }}>
      <DialogContent className="max-w-md gap-3 p-5" onEscapeKeyDown={(e) => busy && e.preventDefault()} onInteractOutside={(e) => busy && e.preventDefault()}>
        <DialogTitle className="text-base font-semibold">{busy ? "Recording" : "Record a meeting"}</DialogTitle>
        {!busy ? (
          <>
            <DialogDescription className="text-[13px] text-muted-foreground">
              Pluto records from this computer's microphone and saves the audio here. It is sent for transcription only after you stop.
            </DialogDescription>
            <div className="mt-3 space-y-3">
              <div className="space-y-1.5">
                <label htmlFor="rec-title" className="text-xs font-medium text-muted-foreground">Meeting title</label>
                <Input id="rec-title" value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Launch sync" />
              </div>
              <div className="space-y-1.5">
                <label htmlFor="rec-people" className="text-xs font-medium text-muted-foreground">Participants (separate with commas)</label>
                <Input id="rec-people" value={people} onChange={(e) => setPeople(e.target.value)} placeholder="Asha, Ravi, Meena" />
              </div>
              <label className="flex items-start gap-2 text-[13px]">
                <input type="checkbox" className="mt-0.5 size-4 accent-primary" checked={consent} onChange={(e) => setConsent(e.target.checked)} />
                <span>Everyone in this meeting knows it is being recorded.</span>
              </label>
              {error && <p role="alert" className="text-[13px] text-destructive">{error}</p>}
            </div>
            <div className="mt-4 flex justify-end gap-2">
              <Button variant="ghost" onClick={() => { reset(); onClose(); }}>Cancel</Button>
              <Button disabled={!consent} onClick={() => void start()}><Mic />Start recording</Button>
            </div>
          </>
        ) : (
          <>
            <DialogDescription className="sr-only">A recording is in progress.</DialogDescription>
            <div className="mt-2 flex flex-col items-center gap-2 py-4">
              <span className="flex items-center gap-2 text-sm font-medium" role="status">
                <Circle className={live ? "size-3 animate-pulse fill-red-500 text-red-500" : "size-3 text-muted-foreground"} />
                {phase === "paused" ? "Paused" : phase === "stopping" ? "Saving…" : phase === "starting" ? "Starting…" : "Recording"}
              </span>
              <span className="text-3xl font-semibold tabular-nums" role="timer" aria-label={`${clock(elapsed)} recorded`}>{clock(elapsed)}</span>
              <p className="text-xs text-muted-foreground">{title.trim() || "Recorded meeting"}</p>
            </div>
            {error && <p role="alert" className="text-[13px] text-destructive">{error}</p>}
            <div className="mt-2 flex justify-center gap-2">
              {phase === "paused" ? (
                <Button variant="outline" onClick={() => void resume()}><Play />Resume</Button>
              ) : (
                <Button variant="outline" disabled={phase !== "recording"} onClick={() => void pause()}><Pause />Pause</Button>
              )}
              <Button variant="destructive" disabled={phase === "starting" || phase === "stopping"} onClick={() => void stop()}><Square />Stop and save</Button>
            </div>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}
