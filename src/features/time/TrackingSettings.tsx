import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { Switch } from "@/components/ui/switch";
import { Settings2 } from "lucide-react";
import { Row } from "@/features/settings/parts";
import { useSetting } from "@/hooks/useTheme";
import { ipc } from "@/ipc";
import { notify } from "@/lib/notify";
import { isMac } from "@/lib/platform";

/**
 * Tracking and privacy: the switches that decide what the Time page can show. A button in the
 * Time page header opens them (they used to sit in a tile at the bottom of the page: owner
 * 2026-10-27), with the current state on the button so you can see it without opening it.
 */
export function TrackingSettings() {
  const [tracking, setTracking] = useSetting("tracking.apps");
  const [excluded, setExcluded] = useSetting("tracking.exclude_apps");
  const [sites, setSites] = useSetting("tracking.sites");
  const [safari, setSafari] = useSetting("tracking.safari");
  const [exclText, setExclText] = useState<string | null>(null);
  const [confirm, setConfirm] = useState(false);
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button variant="outline" size="sm" onClick={() => setOpen(true)} aria-label="Tracking and privacy settings" data-tour="time-privacy">
        <span className={tracking ? "size-2 rounded-full bg-emerald-500" : "size-2 rounded-full bg-muted-foreground/50"} />
        {tracking ? "Tracking on" : "Tracking off"}
        <Settings2 />
      </Button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="max-w-xl gap-1 p-5">
          <DialogTitle className="text-base font-semibold">Tracking and privacy</DialogTitle>
          <DialogDescription className="text-xs text-muted-foreground">What Pluto records for the Time page. All of it stays on this computer.</DialogDescription>
          <div className="mt-2 divide-y">
        <Row label="Track app usage" hint="Which app is in front, and for how long. Stays on this computer. Window titles are never stored.">
          <Switch checked={tracking ?? false} onCheckedChange={(v) => void setTracking(v)} aria-label="Track app usage" />
        </Row>
        <Row label="Track websites (domain only)" hint="Keeps only the site's name, like github.com. Private windows are skipped. Needs app tracking on.">
          <Switch checked={sites ?? false} disabled={!tracking} onCheckedChange={(v) => void setSites(v)} aria-label="Track websites" />
        </Row>
        {isMac && (
          <Row label="Include Safari" hint="Safari can't say whether a window is private, so private sites would be counted too.">
            <Switch checked={safari ?? false} disabled={!tracking || !sites} onCheckedChange={(v) => void setSafari(v)} aria-label="Include Safari" />
          </Row>
        )}
        <div className="py-3.5">
          <label htmlFor="excluded" className="text-[13px] font-medium">Never track these apps</label>
          <p className="mb-2 text-xs text-muted-foreground">One per line, for example “1Password.exe”.</p>
          <textarea
            id="excluded" rows={3} value={exclText ?? (excluded ?? []).join("\n")}
            onChange={(e) => setExclText(e.target.value)}
            onBlur={() => { if (exclText !== null) { void setExcluded(exclText.split("\n").map((x) => x.trim()).filter(Boolean)); setExclText(null); } }}
            className="w-full resize-none rounded-md border border-input bg-transparent px-3 py-2 text-[13px] outline-none placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring"
          />
        </div>
        <Row label="Delete tracked time" hint="Removes every recorded app and site session.">
          <Button variant="destructive" size="sm" onClick={() => setConfirm(true)}>Delete…</Button>
        </Row>
          </div>
        </DialogContent>
      </Dialog>
      <Dialog open={confirm} onOpenChange={setConfirm}>
        <DialogContent className="max-w-sm gap-3 p-5">
          <DialogTitle className="text-base font-semibold">Delete all tracked time?</DialogTitle>
          <DialogDescription className="text-[13px] text-muted-foreground">Your notes, tasks and reminders are not affected. This can’t be undone.</DialogDescription>
          <div className="flex justify-end gap-2 pt-1">
            <Button variant="ghost" size="sm" onClick={() => setConfirm(false)}>Cancel</Button>
            <Button variant="destructive" size="sm" onClick={() => void ipc.usageDelete(null, null).then(() => { setConfirm(false); notify.info("Tracked time deleted"); })}>Delete</Button>
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
