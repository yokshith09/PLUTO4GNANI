import { useState } from "react";
import { Copy, Download, NotebookText, RefreshCw, ScrollText, Square, Volume2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { realAudio, type AudioDeps } from "@/features/voice/VoiceProvider";
import { useSpeakText } from "@/features/voice/useSpeakText";
import { meetingAi } from "@/ipc/meetingAi";
import { voiceErrorMessage } from "@/ipc/voiceClient";
import { notify } from "@/lib/notify";
import { extractSummary, upsertSummary } from "./summaryNote";

const LANGUAGES = [
  { code: "en-IN", label: "English" }, { code: "hi-IN", label: "हिन्दी" }, { code: "ta-IN", label: "தமிழ்" }, { code: "kn-IN", label: "ಕನ್ನಡ" },
];

/**
 * The meeting's summary (T-205): a short written recap of what was decided and who does what, from
 * the decisions and action items on the meeting only. It lives in the meeting's own notes, under
 * "## Summary", and is saved the moment it is written. "Regenerate" replaces it (and saves again);
 * "Open in notes" goes to the notes it is part of. "Read aloud" speaks it with Timbre in its
 * language (a test tone while Cloud voice is off).
 */
export function RecapPanel({ meetingId, notes = "", onNotes, onOpenNotes, hasItems = true, audio = realAudio }: {
  meetingId: string;
  /** The meeting's notes; the summary is read from them. */
  notes?: string;
  /** Called with the new notes when a summary is written; the editor saves them at once. */
  onNotes?: (notes: string) => void;
  onOpenNotes?: () => void;
  /** There are decisions or action items to summarise. */
  hasItems?: boolean;
  audio?: AudioDeps;
}) {
  const [language, setLanguage] = useState("en-IN");
  // Shown until the notes catch up with it (the editor passes them back a moment later).
  const [fresh, setFresh] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const voice = useSpeakText(audio);
  const text = extractSummary(notes) ?? fresh;

  const write = async () => {
    setBusy(true);
    setError(null);
    try {
      const written = await meetingAi.recapText(meetingId, language);
      setFresh(written);
      onNotes?.(upsertSummary(notes, written));
      notify.info(text ? "Summary regenerated and saved" : "Summary saved in the notes");
    } catch (e) {
      setError(voiceErrorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  const saveMp3 = async () => {
    setSaving(true);
    setError(null);
    try {
      const path = await meetingAi.recap(meetingId, language, false, true);
      if (path) notify.info(`Saved ${path}`);
    } catch (e) {
      setError(voiceErrorMessage(e));
    } finally {
      setSaving(false);
    }
  };
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text ?? "");
      notify.info("Summary copied");
    } catch {
      notify.error("Couldn't copy. Select the text instead.");
    }
  };

  return (
    <section aria-label="Recap" className="space-y-3 rounded-xl border bg-card p-5">
      <div className="flex flex-wrap items-center gap-2">
        <ScrollText className="size-4 text-primary" />
        <h2 className="text-[13px] font-semibold">Summary</h2>
        <label htmlFor="recap-language" className="sr-only">Recap language</label>
        <select id="recap-language" value={language} onChange={(e) => setLanguage(e.target.value)} className="ml-auto h-8 rounded-md border bg-transparent px-2 text-[13px]">
          {LANGUAGES.map((l) => <option key={l.code} value={l.code}>{l.label}</option>)}
        </select>
        <Button size="sm" variant={text ? "outline" : "default"} disabled={busy || !hasItems} onClick={() => void write()}>
          {text ? <RefreshCw /> : null}{busy ? "Writing…" : text ? "Regenerate" : "Write summary"}
        </Button>
      </div>
      {text ? (
        <div className="space-y-2">
          <p className="max-w-prose text-[14px] leading-relaxed" aria-live="polite">{text}</p>
          <div className="flex flex-wrap gap-1">
            {voice.speaking
              ? <Button size="sm" variant="ghost" onClick={voice.stop}><Square />Stop</Button>
              : <Button size="sm" variant="ghost" onClick={() => void voice.speak(text, language)}><Volume2 />Read aloud</Button>}
            <Button size="sm" variant="ghost" onClick={() => void copy()}><Copy />Copy</Button>
            {onOpenNotes && <Button size="sm" variant="ghost" onClick={onOpenNotes}><NotebookText />Open in notes</Button>}
            <Button size="sm" variant="ghost" disabled={saving} onClick={() => void saveMp3()}><Download />{saving ? "Saving…" : "Save as mp3"}</Button>
          </div>
          <p className="text-[12px] text-muted-foreground">Saved in this meeting's notes. Regenerate replaces it.</p>
        </div>
      ) : (
        <p className="text-[12px] text-muted-foreground">
          {hasItems ? "Written from the decisions and action items below, saved in this meeting's notes. The text is sent to Gemini or Groq." : "Add decisions or action items first (or find them from the transcript), then write the summary."}
        </p>
      )}
      {(error ?? voice.error) && <p role="alert" className="text-[13px] text-destructive">{error ?? voice.error}</p>}
    </section>
  );
}
