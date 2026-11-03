import { useState } from "react";
import { ConfirmAction } from "@/components/ConfirmAction";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Pencil, Plus, Quote as QuoteIcon, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { ipc } from "@/ipc";
import { cn } from "@/lib/utils";
import { AUTHOR_MAX, BUILT_IN_QUOTES, TEXT_MAX, parseQuotes, quoteFor, type Quote, type QuoteSettings } from "./quotes";

const KEY = ["prefs", "ui.quotes"] as const;

export function useQuotes() {
  const qc = useQueryClient();
  const { data } = useQuery({ queryKey: KEY, queryFn: async () => parseQuotes(await ipc.prefsGet("ui.quotes")) });
  const save = async (next: QuoteSettings) => {
    qc.setQueryData(KEY, next);
    await ipc.prefsSet("ui.quotes", next);
  };
  return { settings: data, save };
}

/** The quote of the day, large, in the empty space beside the greeting. Hover it to edit quotes. */
export function QuoteOfTheDay({ className }: { className?: string }) {
  const { settings, save } = useQuotes();
  const [editing, setEditing] = useState(false);
  if (!settings) return null;
  const quote = quoteFor(settings, new Date());
  return (
    <>
      {quote ? (
        <figure className={cn("group relative max-w-xl overflow-hidden rounded-xl border border-primary/40 bg-primary/10 py-4 pl-6 pr-5 shadow-sm", className)}>
          <span aria-hidden className="absolute inset-y-0 left-0 w-1.5 bg-primary" />
          <QuoteIcon aria-hidden className="mb-1.5 size-6 text-primary" />
          <blockquote className="text-xl font-semibold leading-snug tracking-tight">{quote.text}</blockquote>
          {quote.author && <figcaption className="mt-2 text-[13px] font-medium text-primary">— {quote.author}</figcaption>}
          <button type="button" aria-label="Edit quotes" onClick={() => setEditing(true)}
            className="absolute right-2 top-2 grid size-7 place-items-center rounded-md text-muted-foreground opacity-0 outline-none hover:bg-accent hover:text-foreground focus-visible:opacity-100 focus-visible:ring-2 focus-visible:ring-ring group-hover:opacity-100 data-[state=open]:opacity-100">
            <Pencil className="size-3.5" />
          </button>
        </figure>
      ) : (
        <Button variant="ghost" size="sm" className={cn("text-muted-foreground", className)} onClick={() => setEditing(true)}><QuoteIcon />Quotes</Button>
      )}
      <QuotesDialog open={editing} onClose={() => setEditing(false)} settings={settings} onSave={save} />
    </>
  );
}

function QuotesDialog({ open, onClose, settings, onSave }: { open: boolean; onClose: () => void; settings: QuoteSettings; onSave: (s: QuoteSettings) => Promise<void> }) {
  const [text, setText] = useState("");
  const [author, setAuthor] = useState("");
  const [editAt, setEditAt] = useState<number | null>(null);
  const set = (patch: Partial<QuoteSettings>) => void onSave({ ...settings, ...patch });
  const submit = () => {
    const q: Quote = { text: text.trim(), author: author.trim() };
    if (!q.text) return;
    const custom = editAt === null ? [q, ...settings.custom] : settings.custom.map((c, i) => (i === editAt ? q : c));
    set({ custom });
    setText(""); setAuthor(""); setEditAt(null);
  };
  const edit = (i: number) => {
    const q = settings.custom[i];
    if (!q) return;
    setEditAt(i); setText(q.text); setAuthor(q.author);
  };
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-lg gap-0 p-0">
        <div className="border-b p-5">
          <DialogTitle className="text-base font-semibold">Quotes</DialogTitle>
          <DialogDescription className="mt-1 text-[13px]">One shows on Today each day. Your own come first.</DialogDescription>
        </div>
        <div className="max-h-[60vh] space-y-5 overflow-y-auto p-5">
          <form className="space-y-2" onSubmit={(e) => { e.preventDefault(); submit(); }}>
            <label htmlFor="quote-text" className="text-xs font-medium text-muted-foreground">{editAt === null ? "Add a quote" : "Edit quote"}</label>
            <textarea id="quote-text" value={text} maxLength={TEXT_MAX} rows={2} onChange={(e) => setText(e.target.value)} placeholder="The quote"
              className="w-full resize-none rounded-md border border-input bg-transparent px-3 py-2 text-[13px] outline-none placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring" />
            <div className="flex gap-2">
              <Input aria-label="Who said it" value={author} maxLength={AUTHOR_MAX} onChange={(e) => setAuthor(e.target.value)} placeholder="Who said it (optional)" />
              <Button type="submit"><Plus />{editAt === null ? "Add" : "Save"}</Button>
            </div>
          </form>

          {settings.custom.length > 0 && (
            <section aria-label="Your quotes">
              <h3 className="mb-2 text-xs font-medium text-muted-foreground">Your quotes</h3>
              <ul className="divide-y rounded-lg border">
                {settings.custom.map((q, i) => (
                  <li key={`${i}-${q.text}`} className="flex items-start gap-2 px-3 py-2.5">
                    <div className="min-w-0 flex-1">
                      <p className="text-[13px] leading-snug">{q.text}</p>
                      {q.author && <p className="text-xs text-muted-foreground">— {q.author}</p>}
                    </div>
                    <Button size="icon" variant="ghost" className="size-7" aria-label={`Edit “${q.text}”`} onClick={() => edit(i)}><Pencil className="size-3.5" /></Button>
                    <ConfirmAction message="Delete this quote?" onConfirm={() => set({ custom: settings.custom.filter((_, j) => j !== i) })}>
                      <Button size="icon" variant="ghost" className="size-7" aria-label={`Delete “${q.text}”`}><Trash2 className="size-3.5" /></Button>
                    </ConfirmAction>
                  </li>
                ))}
              </ul>
            </section>
          )}

          <div className="space-y-3 rounded-lg border p-3">
            <label className="flex items-center justify-between gap-3 text-[13px]">
              <span>Include {BUILT_IN_QUOTES.length} built-in quotes</span>
              <Switch checked={settings.builtIns} onCheckedChange={(v) => set({ builtIns: v })} aria-label="Include built-in quotes" />
            </label>
            <label className="flex items-center justify-between gap-3 text-[13px]">
              <span>Show a quote on Today</span>
              <Switch checked={settings.show} onCheckedChange={(v) => set({ show: v })} aria-label="Show a quote on Today" />
            </label>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
