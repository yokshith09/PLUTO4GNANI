import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { FileAudio, Upload } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { meetingAi } from "@/ipc/meetingAi";
import { voiceErrorMessage } from "@/ipc/voiceClient";

/** Shown as the limit in the dialog; the shell enforces it (`file_too_large`). */
export const MAX_IMPORT_MB = 10;

const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;

/** Import an audio file as a new meeting (T-202). The file path comes only from the system dialog. */
export function ImportDialog({ open, onClose, onImported }: { open: boolean; onClose: () => void; onImported: (meetingId: string) => void }) {
  const qc = useQueryClient();
  const [title, setTitle] = useState("");
  const [people, setPeople] = useState("");
  const [path, setPath] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const reset = () => { setTitle(""); setPeople(""); setPath(null); setError(null); setBusy(false); };
  const close = () => { reset(); onClose(); };

  const pick = async () => {
    setError(null);
    try {
      const picked = await meetingAi.pickAudio();
      if (picked) {
        setPath(picked);
        setTitle((t) => t || fileName(picked).replace(/\.[^.]+$/, "").replace(/[-_]+/g, " "));
      }
    } catch (e) {
      setError(voiceErrorMessage(e));
    }
  };

  const start = async () => {
    if (!path) return;
    setBusy(true);
    setError(null);
    try {
      const participants = people.split(",").map((p) => p.trim()).filter(Boolean);
      const job = await meetingAi.import(title.trim() || fileName(path), participants, path);
      await qc.invalidateQueries({ queryKey: ["meetings"] });
      reset();
      onImported(job.meeting_id);
    } catch (e) {
      setError(voiceErrorMessage(e));
      setBusy(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && close()}>
      <DialogContent className="max-w-md gap-3 p-5">
        <DialogTitle className="text-base font-semibold">Import a recording</DialogTitle>
        <DialogDescription className="text-[13px] text-muted-foreground">
          Pluto sends the audio to Gnani to be transcribed, then finds decisions and action items. WAV, MP3, M4A, FLAC or OGG, up to {MAX_IMPORT_MB} MB.
        </DialogDescription>
        <div className="mt-3 space-y-3">
          <div className="space-y-1.5">
            <label htmlFor="import-title" className="text-xs font-medium text-muted-foreground">Meeting title</label>
            <Input id="import-title" value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Launch sync" />
          </div>
          <div className="space-y-1.5">
            <label htmlFor="import-people" className="text-xs font-medium text-muted-foreground">Participants (separate with commas)</label>
            <Input id="import-people" value={people} onChange={(e) => setPeople(e.target.value)} placeholder="Asha, Ravi, Meena" />
          </div>
          <div className="flex items-center gap-2">
            <Button type="button" variant="outline" size="sm" onClick={() => void pick()}><FileAudio />Choose audio file</Button>
            <span className="min-w-0 truncate text-[13px] text-muted-foreground" aria-live="polite">{path ? fileName(path) : "No file chosen"}</span>
          </div>
          {error && <p role="alert" className="text-[13px] text-destructive">{error}</p>}
        </div>
        <div className="mt-4 flex justify-end gap-2">
          <Button variant="ghost" onClick={close}>Cancel</Button>
          <Button disabled={!path || busy} onClick={() => void start()}><Upload />{busy ? "Starting…" : "Start transcription"}</Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
