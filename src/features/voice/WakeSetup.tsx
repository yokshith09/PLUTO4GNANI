// Always-on listening (T-705): the Settings row, the warning before turning it on, and teaching
// Pluto the user's own "Pluto". Detection runs on this computer; only what is said after "Pluto"
// goes to Gnani, exactly like a push-to-talk turn.
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { Switch } from "@/components/ui/switch";
import { voiceErrorMessage, voiceIpc } from "@/ipc/voiceClient";
import type { VoiceSettings, WakeStatus } from "@/ipc/generated";
import { notify } from "@/lib/notify";
import { Row } from "@/features/settings/parts";
import { useOptionalVoice } from "./VoiceProvider";

/** How many times teaching asks for "Pluto"; the shell needs `needed` good ones. */
export const ASK_FOR = 8;
/** Each recording is this long: room for one word said after the prompt. */
export const CLIP_MS = 1800;

export const ALWAYS_ON_WARNING =
  "Pluto keeps the microphone on and listens for its name on this computer; Windows shows the microphone icon while it does. Nothing is sent until you say “Pluto”. Then what you say next is sent to Gnani, just like holding the talk key.";

function hintFor(s: VoiceSettings, wake: WakeStatus | null): string {
  if (!s.cloud_voice_enabled) return "Needs cloud voice, which is off.";
  if (!wake?.enrolled) return "Say “Pluto”, then your command. First, teach Pluto your voice.";
  if (s.always_on_enabled) return wake.session_id ? "Listening for “Pluto”." : "Starting…";
  return "Say “Pluto”, then your command, without pressing a key.";
}

export function AlwaysOnRow({ s, onChange }: { s: VoiceSettings; onChange: (on: boolean) => void }) {
  const voice = useOptionalVoice();
  const wake = voice?.wake ?? null;
  const [confirm, setConfirm] = useState(false);
  const [teaching, setTeaching] = useState(false);
  const canTurnOn = s.cloud_voice_enabled && !!wake?.enrolled;
  return (
    <>
      <Row label="Always-on listening" hint={hintFor(s, wake)}>
        <div className="flex items-center gap-2">
          {voice && s.cloud_voice_enabled && (
            <Button variant="outline" size="sm" onClick={() => setTeaching(true)}>
              {wake?.enrolled ? "Teach again" : "Teach Pluto your voice"}
            </Button>
          )}
          <Switch
            checked={s.always_on_enabled && canTurnOn}
            disabled={!canTurnOn}
            aria-label="Always-on listening"
            onCheckedChange={(on) => (on ? setConfirm(true) : onChange(false))}
          />
        </div>
      </Row>

      <Dialog open={confirm} onOpenChange={setConfirm}>
        <DialogContent className="max-w-md gap-3 p-5">
          <DialogTitle className="text-base font-semibold">Turn on always-on listening?</DialogTitle>
          <DialogDescription className="text-[13px] leading-relaxed text-muted-foreground">{ALWAYS_ON_WARNING}</DialogDescription>
          <div className="mt-4 flex justify-end gap-2">
            <Button variant="ghost" onClick={() => setConfirm(false)}>Not now</Button>
            <Button onClick={() => { setConfirm(false); onChange(true); }}>Turn on</Button>
          </div>
        </DialogContent>
      </Dialog>

      {voice && teaching && <TeachDialog record={voice.recordClip} onClose={() => setTeaching(false)} />}
    </>
  );
}

type Step = { kind: "ready" } | { kind: "listening" } | { kind: "said"; ok: boolean; message: string } | { kind: "done" };

/** Asks for "Pluto" up to `ASK_FOR` times; a recording that isn't one clear word is asked again. */
export function TeachDialog({ record, onClose }: { record: (ms: number) => Promise<{ ok: boolean; message: string; clips: number }>; onClose: () => void }) {
  const [started, setStarted] = useState(false);
  const [step, setStep] = useState<Step>({ kind: "ready" });
  const [clips, setClips] = useState(0);
  const [needed, setNeeded] = useState(6);
  const [busy, setBusy] = useState(false);

  const begin = async () => {
    try {
      const w = await voiceIpc.wakeTeachBegin();
      setNeeded(w.needed);
      setClips(0);
      setStarted(true);
    } catch (e) {
      notify.error(voiceErrorMessage(e));
    }
  };
  const sayOnce = async () => {
    setBusy(true);
    setStep({ kind: "listening" });
    try {
      const c = await record(CLIP_MS);
      setClips(c.clips);
      setStep({ kind: "said", ok: c.ok, message: c.message });
    } catch (e) {
      setStep({ kind: "said", ok: false, message: voiceErrorMessage(e) });
    } finally {
      setBusy(false);
    }
  };
  const finish = async () => {
    setBusy(true);
    try {
      await voiceIpc.wakeTeachFinish();
      setStep({ kind: "done" });
    } catch (e) {
      notify.error(voiceErrorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  const close = async () => {
    // Leaving before the end keeps no half-taught wake word.
    if (started && step.kind !== "done") await voiceIpc.wakeForget().catch(() => undefined);
    onClose();
  };

  const enough = clips >= needed;
  const allAsked = clips >= ASK_FOR;
  return (
    <Dialog open onOpenChange={(open) => !open && void close()}>
      <DialogContent className="max-w-md gap-3 p-5">
        <DialogTitle className="text-base font-semibold">Teach Pluto your voice</DialogTitle>
        {!started ? (
          <>
            <DialogDescription className="text-[13px] leading-relaxed text-muted-foreground">
              Say “Pluto” {ASK_FOR} times, the way you'll say it to start a command. It takes about a minute, and the recordings stay on this computer. Teaching again replaces the voice Pluto knows.
            </DialogDescription>
            <div className="mt-4 flex justify-end gap-2">
              <Button variant="ghost" onClick={() => void close()}>Cancel</Button>
              <Button onClick={() => void begin()}>Start</Button>
            </div>
          </>
        ) : step.kind === "done" ? (
          <>
            <DialogDescription className="text-[13px] leading-relaxed">Pluto knows your voice now. Turn on always-on listening, then say “Pluto”, followed by your command.</DialogDescription>
            <div className="mt-4 flex justify-end">
              <Button onClick={onClose}>Done</Button>
            </div>
          </>
        ) : (
          <>
            <DialogDescription className="text-[13px] text-muted-foreground" aria-live="polite">
              {step.kind === "listening" ? "Listening… say “Pluto” now." : step.kind === "said" ? step.message : "Press the button, then say “Pluto”."}
            </DialogDescription>
            <p className="mt-2 text-sm tabular-nums" aria-label="Recordings">
              {Math.min(clips, ASK_FOR)} of {ASK_FOR}
            </p>
            <div className="mt-4 flex justify-end gap-2">
              <Button variant="ghost" onClick={() => void close()}>Cancel</Button>
              {enough && (
                <Button variant={allAsked ? "default" : "outline"} disabled={busy} onClick={() => void finish()}>
                  Finish
                </Button>
              )}
              {!allAsked && (
                <Button disabled={busy} onClick={() => void sayOnce()}>
                  {step.kind === "listening" ? "Listening…" : "Say “Pluto”"}
                </Button>
              )}
            </div>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}
