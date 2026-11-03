import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { notify } from "@/lib/notify";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { errorMessage, ipc } from "@/ipc";
import { Row, Section } from "./parts";

const kb = (bytes: number) => (bytes < 1024 * 1024 ? `${Math.max(1, Math.round(bytes / 1024))} KB` : `${(bytes / 1024 / 1024).toFixed(1)} MB`);

/** Your data (R0-07…09): take it all out, put it back into an empty Pluto, or erase everything. */
export function DataSection() {
  const { data: paths } = useQuery({ queryKey: ["app-paths"], queryFn: ipc.appPaths });
  const [busy, setBusy] = useState(false);
  const [importing, setImporting] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [word, setWord] = useState("");

  const run = async (work: () => Promise<void>) => {
    setBusy(true);
    try {
      await work();
    } catch (e) {
      notify.error(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  const doExport = () => run(async () => {
    const done = await ipc.dataExport();
    if (done) notify.info("Export saved", { description: `${done.path} · ${kb(done.bytes)}` });
  });
  const doImport = () => run(async () => {
    setImporting(false);
    const counts = await ipc.dataImport();
    if (counts) {
      const total = Object.entries(counts).filter(([t]) => !["settings", "user_preferences"].includes(t)).reduce((n, [, c]) => n + c, 0);
      notify.info("Import finished", { description: `${total} items restored.` });
    }
  });
  const doDelete = () => run(async () => {
    await ipc.dataDeleteAll(word);
    setDeleting(false);
    setWord("");
    notify.info("Everything was deleted", { description: "Pluto is back to how it was on the first day." });
  });

  return (
    <>
      <Section title="Your data">
        <Row label="Stored on this computer" hint={paths?.database ?? ""}>
          <Button variant="outline" size="sm" onClick={() => void ipc.folderOpen("data").catch((e: unknown) => notify.error(errorMessage(e)))}>Open folder</Button>
        </Row>
        <Row label="Export everything" hint="Saves your notes, tasks, reminders, logs and settings to one file you keep.">
          <Button variant="outline" size="sm" disabled={busy} onClick={() => void doExport()}>Export…</Button>
        </Row>
        <Row label="Import from a file" hint="Restores an export into an empty Pluto. It never mixes with data that is already here.">
          <Button variant="outline" size="sm" disabled={busy} onClick={() => setImporting(true)}>Import…</Button>
        </Row>
        <Row label="Delete all data" hint="Erases every note, task, reminder, log and setting on this computer. This can’t be undone.">
          <Button variant="destructive" size="sm" disabled={busy} onClick={() => setDeleting(true)}>Delete all…</Button>
        </Row>
      </Section>

      <Dialog open={importing} onOpenChange={setImporting}>
        <DialogContent className="max-w-sm gap-3 p-5">
          <DialogTitle className="text-base font-semibold">Import into an empty Pluto?</DialogTitle>
          <DialogDescription className="text-[13px] text-muted-foreground">
            Pick a file you exported from Pluto. If this Pluto already has notes, tasks or reminders, nothing is imported; delete all data first. If anything in the file is wrong, nothing is imported at all.
          </DialogDescription>
          <div className="flex justify-end gap-2 pt-1">
            <Button variant="ghost" size="sm" onClick={() => setImporting(false)}>Cancel</Button>
            <Button size="sm" onClick={() => void doImport()}>Choose file…</Button>
          </div>
        </DialogContent>
      </Dialog>

      <Dialog open={deleting} onOpenChange={(o) => { setDeleting(o); if (!o) setWord(""); }}>
        <DialogContent className="max-w-sm gap-3 p-5">
          <DialogTitle className="text-base font-semibold">Delete everything?</DialogTitle>
          <DialogDescription className="text-[13px] text-muted-foreground">
            Every note, task, reminder, daily log, tracked time and setting will be erased from this computer. Export first if you might want it back. Type <span className="font-mono font-semibold text-foreground">DELETE</span> to confirm.
          </DialogDescription>
          <form className="space-y-3" onSubmit={(e) => { e.preventDefault(); if (word === "DELETE") void doDelete(); }}>
            <Input value={word} onChange={(e) => setWord(e.target.value)} aria-label="Type DELETE to confirm" autoComplete="off" spellCheck={false} />
            <div className="flex justify-end gap-2">
              <Button type="button" variant="ghost" size="sm" onClick={() => setDeleting(false)}>Cancel</Button>
              <Button type="submit" variant="destructive" size="sm" disabled={word !== "DELETE" || busy}>Delete everything</Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>
    </>
  );
}
