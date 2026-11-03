import { useState } from "react";
import { ConfirmAction } from "@/components/ConfirmAction";
import { RotateCcw, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { timeAgo } from "@/lib/time";
import { useBin, useNoteActions } from "@/hooks/usePluto";

export const BIN_DAYS = 30;

export function BinList({ limit }: { limit?: number }) {
  const { data = [] } = useBin();
  const actions = useNoteActions();
  const rows = limit ? data.slice(0, limit) : data;
  if (rows.length === 0) {
    return <p className="px-1 py-8 text-center text-[13px] text-muted-foreground">The Bin is empty. Deleted notes stay here for {BIN_DAYS} days.</p>;
  }
  return (
    <ul className="-mx-1.5">
      {rows.map((n) => (
        <li key={n.id} className="group flex h-9 items-center gap-2.5 rounded-md px-1.5 hover:bg-accent/50">
          <span className="min-w-0 flex-1 truncate text-[13px]">{n.title || n.excerpt || "Untitled note"}</span>
          <span className="text-[11px] text-muted-foreground">Deleted {timeAgo(n.deleted_at)}</span>
          <Button variant="ghost" size="icon-sm" aria-label={`Restore “${n.title || "note"}”`} title="Restore" onClick={() => void actions.restore(n.id)}><RotateCcw /></Button>
          <ConfirmAction message="Delete this note for good? It can't be brought back." verb="Delete" onConfirm={() => void actions.purge(n.id)}>
            <Button variant="ghost" size="icon-sm" aria-label={`Delete “${n.title || "note"}” forever`} title="Delete forever" className="hover:text-destructive"><Trash2 /></Button>
          </ConfirmAction>
        </li>
      ))}
    </ul>
  );
}

export function EmptyBinButton() {
  const { data = [] } = useBin();
  const actions = useNoteActions();
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button variant="outline" size="sm" disabled={data.length === 0} onClick={() => setOpen(true)}>Empty Bin</Button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="max-w-sm gap-3 p-5">
          <DialogTitle className="text-base font-semibold">Empty the Bin?</DialogTitle>
          <DialogDescription className="text-[13px] text-muted-foreground">
            {data.length} {data.length === 1 ? "note" : "notes"} will be deleted permanently. This can’t be undone.
          </DialogDescription>
          <div className="flex justify-end gap-2 pt-1">
            <Button variant="ghost" size="sm" onClick={() => setOpen(false)}>Cancel</Button>
            <Button variant="destructive" size="sm" onClick={() => void actions.emptyBin().then(() => setOpen(false))}>Delete forever</Button>
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
