import { NotebookPen, Pin, Plus, SearchX } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { NoteSummary } from "@/ipc";
import { ShowMore, useProgressive } from "@/lib/progressive";
import { NoteCard, NoteRow, PinnedCard, type NoteHandlers } from "./NoteCard";

export type NotesLayout = "grid" | "list";

function Section({ title, icon, count, children }: { title: string; icon?: React.ReactNode; count: number; children: React.ReactNode }) {
  return (
    <section aria-label={title}>
      <h2 className="mb-2.5 flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
        {icon}
        {title}
        <span className="tabular-nums text-muted-foreground/70">{count}</span>
      </h2>
      {children}
    </section>
  );
}

function Items({ notes, layout, ...h }: { notes: NoteSummary[]; layout: NotesLayout } & NoteHandlers) {
  const page = useProgressive(notes);
  const more = <ShowMore more={page.more} onMore={page.showMore} noun="notes" />;
  if (layout === "list") {
    return (
      <>
      <div className="divide-y overflow-hidden rounded-xl border bg-card">
        <div aria-hidden className="flex h-8 items-center gap-3 bg-muted/40 px-4 text-[11px] font-medium text-muted-foreground">
          <span className="size-2 shrink-0" /><span className="w-56 shrink-0">Name</span><span className="flex-1">Preview</span>
          <span className="hidden w-32 lg:block">Labels</span><span className="w-20 shrink-0 text-right">Edited</span>
        </div>
        {page.visible.map((n) => <NoteRow key={n.id} note={n} {...h} />)}
      </div>
      {more}
      </>
    );
  }
  return (
    <>
      <div className="gap-3 [columns:16rem]">{page.visible.map((n) => <NoteCard key={n.id} note={n} {...h} />)}</div>
      {more}
    </>
  );
}

/** Pinned notes: larger, accent-edged cards in one row, deliberately unlike the regular grid. */
export function PinnedRow({ notes, className, ...h }: { notes: NoteSummary[]; className?: string } & NoteHandlers) {
  return (
    <section aria-label="Pinned">
      <h2 className="mb-2.5 flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
        <Pin className="size-3 rotate-45 fill-primary text-primary" />Pinned<span className="tabular-nums text-muted-foreground/70">{notes.length}</span>
      </h2>
      <div className={`grid gap-3 ${className ?? "sm:grid-cols-2 xl:grid-cols-3"}`}>
        {notes.map((n) => <PinnedCard key={n.id} note={n} {...h} />)}
      </div>
    </section>
  );
}

export function Empty({ icon, title, hint, action }: { icon: React.ReactNode; title: string; hint: string; action?: React.ReactNode }) {
  return (
    <div className="flex flex-col items-center rounded-xl border border-dashed px-6 py-14 text-center">
      <span className="mb-3 grid size-10 place-items-center rounded-lg bg-muted text-muted-foreground [&_svg]:size-5">{icon}</span>
      <p className="text-sm font-medium">{title}</p>
      <p className="mt-1 max-w-sm text-[13px] text-muted-foreground">{hint}</p>
      {action && <div className="mt-4">{action}</div>}
    </div>
  );
}

export function NotesBoard({
  notes, layout, mode, onNew, ...h
}: { notes: NoteSummary[]; layout: NotesLayout; mode: "notes" | "archive" | "label" | "search"; onNew: () => void } & NoteHandlers) {
  if (notes.length === 0) {
    if (mode === "search") return <Empty icon={<SearchX />} title="No matching notes" hint="Try a different word, or check the Archive." />;
    if (mode === "archive") return <Empty icon={<NotebookPen />} title="Nothing archived" hint="Archive a note to tuck it away without deleting it." />;
    if (mode === "label") return <Empty icon={<NotebookPen />} title="No notes with this label" hint="Add the label to a note from its editor." />;
    return (
      <Empty
        icon={<NotebookPen />}
        title="No notes yet"
        hint="Notes you write live here. Press Ctrl+N to start one."
        action={<Button onClick={onNew}><Plus />New note</Button>}
      />
    );
  }
  const split = mode === "notes" || mode === "label";
  const pinned = split ? notes.filter((n) => n.pinned) : [];
  const rest = split ? notes.filter((n) => !n.pinned) : notes;
  return (
    <div className="space-y-7">
      {pinned.length > 0 && <PinnedRow notes={pinned} {...h} />}
      {rest.length > 0 && (
        <Section title={pinned.length ? "All notes" : mode === "search" ? "Results" : "Notes"} count={rest.length}>
          <Items notes={rest} layout={layout} {...h} />
        </Section>
      )}
    </div>
  );
}
