import { useRef, useState } from "react";
import { Check, Pin } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useNoteActions } from "@/hooks/usePluto";
import { cn } from "@/lib/utils";

/** Quick capture, Keep style: one line to start, expands to a title and body. Ctrl/⌘ Enter saves, Esc discards. */
export function NoteComposer({ className, onOpenEditor }: { className?: string; onOpenEditor: () => void }) {
  const actions = useNoteActions();
  const [open, setOpen] = useState(false);
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [pinned, setPinned] = useState(false);
  const bodyRef = useRef<HTMLTextAreaElement>(null);

  const reset = () => { setOpen(false); setTitle(""); setBody(""); setPinned(false); };
  const save = async () => {
    if (!title.trim() && !body.trim()) return reset();
    const note = await actions.create({ title: title.trim(), body, color: "default", label_ids: [] });
    if (pinned) await actions.pin(note.id, true);
    reset();
  };

  if (!open) {
    return (
      <div className={cn("flex h-11 items-center gap-2 rounded-xl border bg-card px-4 focus-within:ring-2 focus-within:ring-ring text-[13px] text-muted-foreground shadow-sm transition-colors hover:border-foreground/25", className)}>
        <button type="button" onClick={() => setOpen(true)} className="h-full flex-1 text-left outline-none focus-visible:text-foreground">Take a note…</button>
        <button type="button" onClick={onOpenEditor} className="rounded px-2 py-1 text-xs outline-none hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring">Open full editor</button>
      </div>
    );
  }
  return (
    <form
      aria-label="Quick note"
      onSubmit={(e) => { e.preventDefault(); void save(); }}
      onKeyDown={(e) => {
        if (e.key === "Escape") reset();
        if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) { e.preventDefault(); void save(); }
      }}
      className={cn("rounded-xl border bg-card shadow-sm ring-1 ring-ring/30", className)}
    >
      <div className="flex items-center gap-2 px-4 pt-3">
        <input
          autoFocus value={title} onChange={(e) => setTitle(e.target.value)} maxLength={200} placeholder="Title" aria-label="Note title"
          onKeyDown={(e) => { if (e.key === "Enter" && !e.ctrlKey && !e.metaKey) { e.preventDefault(); bodyRef.current?.focus(); } }}
          className="min-w-0 flex-1 bg-transparent text-[14px] font-semibold outline-none placeholder:text-muted-foreground"
        />
        <button
          type="button" aria-pressed={pinned} aria-label={pinned ? "Unpin" : "Pin"} onClick={() => setPinned((p) => !p)}
          className={cn("grid size-7 place-items-center rounded outline-none hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring", pinned ? "text-primary" : "text-muted-foreground")}
        >
          <Pin className={cn("size-4 rotate-45", pinned && "fill-primary")} />
        </button>
      </div>
      <textarea
        ref={bodyRef} value={body} onChange={(e) => setBody(e.target.value)} rows={3} placeholder="Take a note…" aria-label="Note body"
        className="block w-full resize-none bg-transparent px-4 py-2 text-[13px] leading-relaxed outline-none placeholder:text-muted-foreground"
      />
      <div className="flex items-center justify-end gap-2 px-3 pb-3">
        <span className="mr-auto text-[11px] text-muted-foreground">Ctrl+Enter to save</span>
        <Button type="button" variant="ghost" size="sm" onClick={reset}>Discard</Button>
        <Button type="submit" size="sm"><Check />Save</Button>
      </div>
    </form>
  );
}
