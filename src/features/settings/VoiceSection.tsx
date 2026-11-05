import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ConfirmAction } from "@/components/ConfirmAction";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { voiceErrorMessage, voiceIpc, type KeyAccount } from "@/ipc/voiceClient";
import type { Destination, VoiceSettings, VoiceSettingsPatch } from "@/ipc/generated";
import { notify } from "@/lib/notify";
import { Row, Section } from "./parts";

export const LANGUAGES: { code: string; name: string }[] = [
  { code: "hi-IN", name: "Hindi" }, { code: "en-IN", name: "English" }, { code: "ta-IN", name: "Tamil" }, { code: "te-IN", name: "Telugu" },
  { code: "kn-IN", name: "Kannada" }, { code: "ml-IN", name: "Malayalam" }, { code: "mr-IN", name: "Marathi" }, { code: "pa-IN", name: "Punjabi" },
  { code: "bn-IN", name: "Bengali" }, { code: "gu-IN", name: "Gujarati" },
];
/** Gnani's live speech service takes at most this many languages at once. */
export const MAX_LANGUAGES = 4;

const KEYS: { account: KeyAccount; label: string; flag: "has_gnani_key" | "has_gemini_key" | "has_groq_key"; hint: string }[] = [
  { account: "gnani", label: "Gnani key", flag: "has_gnani_key", hint: "Speech to text and text to speech." },
  { account: "gemini", label: "Gemini key", flag: "has_gemini_key", hint: "Finds decisions and action items, answers questions." },
  { account: "groq", label: "Groq key", flag: "has_groq_key", hint: "Understands voice commands; the backup for Gemini." },
];

const DESTINATION: Record<Destination, string> = {
  gnani_realtime: "Gnani (live speech)", gnani_rest: "Gnani (speech)", gnani_batch: "Gnani (meeting audio)", gnani_tts: "Gnani (voice)",
  gnani_clone: "Gnani (voice clone)", gemini: "Gemini", groq: "Groq",
};

export const DISCLOSURE = "When this is on, audio you record, import or speak is sent to Gnani to be turned into text, and the text of your meetings and commands is sent to Google Gemini or Groq to find decisions, answer questions and write recaps. Nothing is sent while it is off. Pluto keeps a list of everything it sends.";

function KeyRow({ account, label, flag, hint, saved, onChanged }: (typeof KEYS)[number] & { saved: boolean; onChanged: () => void }) {
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const run = async (work: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await work();
      setText("");
      onChanged();
    } catch (e) {
      notify.error(voiceErrorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Row label={label} hint={saved ? "Saved on this computer. It is never shown again." : hint}>
      {saved ? (
        <ConfirmAction message={`Remove the ${label.toLowerCase()}?`} verb="Remove" onConfirm={() => void run(() => voiceIpc.keyClear(account))}>
          <Button variant="outline" size="sm" disabled={busy} data-flag={flag}>Remove</Button>
        </ConfirmAction>
      ) : (
        <>
          <Input type="password" autoComplete="off" spellCheck={false} aria-label={label} placeholder="Paste key" className="w-44" value={text} onChange={(e) => setText(e.target.value)} />
          <Button size="sm" disabled={busy || !text.trim()} onClick={() => void run(() => voiceIpc.keySet(account, text.trim()))}>Save</Button>
        </>
      )}
    </Row>
  );
}

/** Voice and privacy (T-401): the cloud-voice opt-in with its disclosure, keys, languages, and what was sent. */
export function VoiceSection() {
  const qc = useQueryClient();
  const { data: s } = useQuery({ queryKey: ["voice-settings"], queryFn: voiceIpc.settingsGet });
  const { data: log = [] } = useQuery({ queryKey: ["privacy-log"], queryFn: () => voiceIpc.privacyLogList(20) });
  const [confirmOn, setConfirmOn] = useState(false);
  const refresh = () => void Promise.all([qc.invalidateQueries({ queryKey: ["voice-settings"] }), qc.invalidateQueries({ queryKey: ["privacy-log"] })]);
  const patch = useMutation({
    mutationFn: (p: VoiceSettingsPatch) => voiceIpc.settingsSet(p),
    onSuccess: (next: VoiceSettings) => qc.setQueryData(["voice-settings"], next),
    onError: (e) => notify.error(voiceErrorMessage(e)),
  });
  if (!s) return null;

  const chosen = s.languages;
  const toggleLanguage = (code: string, on: boolean) => {
    const next = on ? [...chosen, code] : chosen.filter((c) => c !== code);
    if (next.length <= MAX_LANGUAGES) patch.mutate({ languages: next });
  };
  const clearLog = async () => {
    try {
      await voiceIpc.privacyLogClear();
      refresh();
    } catch (e) {
      notify.error(voiceErrorMessage(e));
    }
  };

  return (
    <>
      <Section title="Voice and privacy">
        <Row label="Use cloud voice" hint="Off by default. Needed for meeting transcripts, voice commands and AI summaries.">
          <Switch
            checked={s.cloud_voice_enabled} aria-label="Use cloud voice"
            onCheckedChange={(on) => (on ? setConfirmOn(true) : patch.mutate({ cloud_voice_enabled: false }))}
          />
        </Row>
        <p className="py-3 text-xs leading-relaxed text-muted-foreground">{DISCLOSURE}</p>
        {KEYS.map((k) => <KeyRow key={k.account} {...k} saved={s[k.flag]} onChanged={refresh} />)}
        <div className="py-3.5">
          <p className="text-[13px] font-medium">Languages</p>
          <p className="text-xs text-muted-foreground">
            {chosen.length === 0 ? "Pluto will detect the language by itself." : `Listening for ${chosen.length} of ${MAX_LANGUAGES}.`} Choose up to {MAX_LANGUAGES} to help with accuracy.
          </p>
          <ul aria-label="Languages" className="mt-2 grid grid-cols-2 gap-x-4 gap-y-1.5 sm:grid-cols-3">
            {LANGUAGES.map((l) => {
              const on = chosen.includes(l.code);
              return (
                <li key={l.code}>
                  <label className="flex items-center gap-2 text-[13px]">
                    <input type="checkbox" className="size-4 accent-primary" checked={on} disabled={!on && chosen.length >= MAX_LANGUAGES} onChange={(e) => toggleLanguage(l.code, e.target.checked)} />
                    {l.name}
                  </label>
                </li>
              );
            })}
          </ul>
        </div>
        <Row label="Transcript style" hint="How mixed-language speech is written down.">
          <ToggleGroup type="single" value={s.stt_format} onValueChange={(v) => v && patch.mutate({ stt_format: v as VoiceSettings["stt_format"] })} aria-label="Transcript style">
            <ToggleGroupItem value="codemix">Mixed</ToggleGroupItem>
            <ToggleGroupItem value="transliterate">Roman letters</ToggleGroupItem>
            <ToggleGroupItem value="verbatim">Exact words</ToggleGroupItem>
          </ToggleGroup>
        </Row>
        <Row label="Expressive replies" hint="Lets spoken replies sound warmer (laughs, pauses).">
          <Switch checked={s.audio_tags} onCheckedChange={(v) => patch.mutate({ audio_tags: v })} aria-label="Expressive replies" />
        </Row>
        <Row label="Always-on listening" hint="Not available yet. It will stay off unless you turn it on.">
          <Switch checked={false} disabled aria-label="Always-on listening" />
        </Row>
      </Section>

      <Section title="Sent to the cloud">
        {log.length === 0 ? (
          <p className="py-3.5 text-[13px] text-muted-foreground">Nothing has been sent yet.</p>
        ) : (
          <ul aria-label="Sent to the cloud" className="divide-y">
            {log.map((e) => (
              <li key={e.id} className="flex flex-wrap items-baseline gap-x-3 py-2 text-[13px]">
                <span className="w-44 shrink-0 font-medium">{DESTINATION[e.destination]}</span>
                <span className="min-w-0 flex-1 text-muted-foreground">{e.purpose}</span>
                <span className="shrink-0 tabular-nums text-xs text-muted-foreground">{(e.bytes / 1024).toFixed(e.bytes < 10_240 ? 1 : 0)} KB · {new Date(e.at).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}</span>
              </li>
            ))}
          </ul>
        )}
        <Row label="Clear this list" hint="Removes the list only. It does not recall anything already sent.">
          <ConfirmAction message="Clear the list of what was sent?" verb="Clear" onConfirm={() => void clearLog()}>
            <Button variant="outline" size="sm" disabled={log.length === 0}>Clear</Button>
          </ConfirmAction>
        </Row>
      </Section>

      <Dialog open={confirmOn} onOpenChange={setConfirmOn}>
        <DialogContent className="max-w-md">
          <DialogTitle className="text-base font-semibold">Turn on cloud voice?</DialogTitle>
          <DialogDescription className="text-[13px] leading-relaxed text-muted-foreground">{DISCLOSURE}</DialogDescription>
          <div className="mt-4 flex justify-end gap-2">
            <Button variant="ghost" onClick={() => setConfirmOn(false)}>Not now</Button>
            <Button onClick={() => { setConfirmOn(false); patch.mutate({ cloud_voice_enabled: true }); }}>Turn on</Button>
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
