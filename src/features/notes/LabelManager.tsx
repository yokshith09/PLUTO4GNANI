import { useState } from "react";
import { Check, Pencil, Tag, Trash2, X } from "lucide-react";
import { ConfirmAction } from "@/components/ConfirmAction";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { useNoteActions } from "@/hooks/usePluto";
import type { LabelCount } from "@/ipc";

function Row({ item, onDeleted }: { item: LabelCount; onDeleted: (id: string) => void }) {
  const actions = useNoteActions();
  const { label, count } = item;
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState(label.name);

  const save = async () => {
    const next = name.trim();
    if (!next || next === label.name) return setEditing(false);
    try {
      await actions.renameLabel(label.id, next);
      setEditing(false);
    } catch {
      /* the error bubble above the pet says why (for example the name is taken); stay in edit mode */
    }
  };

  return (
    <li className="flex h-10 items-center gap-2 rounded-md px-2 hover:bg-accent/40">
      <Tag className="size-3.5 shrink-0 text-muted-foreground" />
      {editing ? (
        <form className="flex min-w-0 flex-1 items-center gap-1.5" onSubmit={(e) => { e.preventDefault(); void save(); }}>
          <Input autoFocus value={name} maxLength={50} onChange={(e) => setName(e.target.value)} aria-label={`New name for ${label.name}`} className="h-7" onKeyDown={(e) => e.key === "Escape" && (e.stopPropagation(), setEditing(false), setName(label.name))} />
          <Button type="submit" size="icon-sm" variant="secondary" aria-label="Save name"><Check /></Button>
          <Button type="button" size="icon-sm" variant="ghost" aria-label="Cancel rename" onClick={() => { setEditing(false); setName(label.name); }}><X /></Button>
        </form>
      ) : (
        <>
          <span className="min-w-0 flex-1 truncate text-[13px]">{label.name}</span>
          <span className="text-[11px] tabular-nums text-muted-foreground">{count} {count === 1 ? "note" : "notes"}</span>
          <Button size="icon-sm" variant="ghost" aria-label={`Rename ${label.name}`} onClick={() => setEditing(true)}><Pencil /></Button>
          <ConfirmAction message={`Delete “${label.name}”? It comes off ${count} ${count === 1 ? "note" : "notes"}.`} onConfirm={() => void actions.deleteLabel(label.id).then(() => onDeleted(label.id))}>
            <Button size="icon-sm" variant="ghost" aria-label={`Delete ${label.name}`} className="hover:text-destructive"><Trash2 /></Button>
          </ConfirmAction>
        </>
      )}
    </li>
  );
}

/** Rename and delete labels (R1-08). Deleting a label removes it from its notes; the notes stay. */
export function LabelManager({ open, onOpenChange, labels, onDeleted }: { open: boolean; onOpenChange: (o: boolean) => void; labels: LabelCount[]; onDeleted: (id: string) => void }) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md gap-0 overflow-hidden">
        <div className="border-b p-5 pr-12">
          <DialogTitle className="text-base font-semibold">Labels</DialogTitle>
          <DialogDescription className="mt-0.5 text-xs text-muted-foreground">Rename a label or delete it. Deleting never deletes the notes.</DialogDescription>
        </div>
        <div className="max-h-[60vh] overflow-y-auto p-3">
          {labels.length === 0 ? (
            <p className="px-2 py-6 text-center text-[13px] text-muted-foreground">No labels yet. Add one from a note’s tag button.</p>
          ) : (
            <ul>{labels.map((l) => <Row key={l.label.id} item={l} onDeleted={onDeleted} />)}</ul>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
