import { useState } from "react";
import { Copy, ScrollText, Square, Volume2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { realAudio, type AudioDeps } from "@/features/voice/VoiceProvider";
import { useSpeakText } from "@/features/voice/useSpeakText";
import { meetingAi } from "@/ipc/meetingAi";
import { voiceErrorMessage } from "@/ipc/voiceClient";
import { notify } from "@/lib/notify";

const LANGUAGES = [
  { code: "en-IN", label: "English" }, { code: "hi-IN", label: "हिन्दी" }, { code: "ta-IN", label: "தமிழ்" }, { code: "kn-IN", label: "ಕನ್ನಡ" },
];

/**
 * A short written recap of what was decided and who does what (T-205), from the decisions and
 * action items on the meeting only. "Read aloud" speaks it with Timbre in the recap's language
 * (a test tone while Cloud voice is off), and can be stopped.
 */
export function RecapPanel({ meetingId, audio = realAudio }: { meetingId: string; audio?: AudioDeps }) {
  const [language, setLanguage] = useState("en-IN");
  const [text, setText] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const voice = useSpeakText(audio);

  const write = async () => {
    setBusy(true);
    setError(null);
    try {
      setText(await meetingAi.recapText(meetingId, language));
    } catch (e) {
      setText(null);
      setError(voiceErrorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text ?? "");
      notify.info("Recap copied");
    } catch {
      notify.error("Couldn't copy. Select the text instead.");
    }
  };

  return (
    <section aria-label="Recap" className="mt-6 space-y-3 rounded-xl border bg-card/50 p-4">
      <div className="flex flex-wrap items-center gap-2">
        <ScrollText className="size-4 text-muted-foreground" />
        <h2 className="text-sm font-semibold">Recap</h2>
        <label htmlFor="recap-language" className="sr-only">Recap language</label>
        <select id="recap-language" value={language} onChange={(e) => setLanguage(e.target.value)} className="h-8 rounded-md border bg-transparent px-2 text-[13px]">
          {LANGUAGES.map((l) => <option key={l.code} value={l.code}>{l.label}</option>)}
        </select>
        <Button size="sm" variant="outline" disabled={busy} onClick={() => void write()}>{busy ? "Writing…" : text ? "Write again" : "Write recap"}</Button>
      </div>
      {text && (
        <div className="space-y-2">
          <p className="max-w-prose text-[14px] leading-relaxed" aria-live="polite">{text}</p>
          <div className="flex gap-1">
            {voice.speaking
              ? <Button size="sm" variant="ghost" onClick={voice.stop}><Square />Stop</Button>
              : <Button size="sm" variant="ghost" onClick={() => void voice.speak(text, language)}><Volume2 />Read aloud</Button>}
            <Button size="sm" variant="ghost" onClick={() => void copy()}><Copy />Copy</Button>
          </div>
        </div>
      )}
      {(error ?? voice.error) && <p role="alert" className="text-[13px] text-destructive">{error ?? voice.error}</p>}
      {!text && !error && <p className="text-[12px] text-muted-foreground">Uses only the decisions and action items on this meeting. The text is sent to Gemini or Groq.</p>}
    </section>
  );
}
