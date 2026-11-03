import { CourseTag } from "@/features/student/CoursePicker";
import { forwardRef, type ComponentPropsWithoutRef, type MouseEvent } from "react";
import { ConfirmAction } from "@/components/ConfirmAction";
import { Archive, ArchiveRestore, Pin, PinOff, Trash2 } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import { timeAgo } from "@/lib/time";
import type { NoteSummary } from "@/ipc";
import { withBoxes } from "./format";

export interface NoteHandlers {
  onOpen: (id: string) => void;
  onPin: (id: string, pinned: boolean) => void;
  onArchive: (id: string, archived: boolean) => void;
  onDelete: (id: string) => void;
}

/** A small icon button on a note card. It passes extra props on, so it can also be a popover's trigger. */
const Action = forwardRef<HTMLButtonElement, Omit<ComponentPropsWithoutRef<"button">, "onClick"> & { label: string; onClick?: (e: MouseEvent<HTMLButtonElement>) => void; danger?: boolean }>(function Action({ label, onClick, children, danger, ...rest }, ref) {
  const handle = (e: MouseEvent<HTMLButtonElement>) => {
    e.stopPropagation();
    onClick?.(e);
  };
  return (
    <button
      {...rest}
      ref={ref}
      type="button"
      title={label}
      aria-label={label}
      onClick={handle}
      onKeyDown={(e) => e.stopPropagation()}
      className={cn(
        "grid size-6 place-items-center rounded text-muted-foreground outline-none transition-colors hover:bg-foreground/10 hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring [&_svg]:size-3.5",
        danger && "hover:bg-destructive/15 hover:text-destructive",
      )}
    >
      {children}
    </button>
  );
});

function Actions({ note, onPin, onArchive, onDelete }: NoteHandlers & { note: NoteSummary }) {
  return (
    <div className="flex items-center gap-0.5">
      {!note.archived && (
        <Action label={note.pinned ? "Unpin" : "Pin"} onClick={() => onPin(note.id, !note.pinned)}>
          {note.pinned ? <PinOff /> : <Pin />}
        </Action>
      )}
      <Action label={note.archived ? "Unarchive" : "Archive"} onClick={() => onArchive(note.id, !note.archived)}>
        {note.archived ? <ArchiveRestore /> : <Archive />}
      </Action>
      <ConfirmAction message="Move this note to the Bin?" onConfirm={() => onDelete(note.id)}>
        <Action label="Delete" danger>
          <Trash2 />
        </Action>
      </ConfirmAction>
    </div>
  );
}

/**
 * The whole card opens the note, but the card itself is not a button: it holds other buttons (pin,
 * archive, delete), and controls inside a button cannot be reached by a screen reader. So the
 * opening is a real button laid over the card (named after the note, with the focus ring), and
 * the toolbar sits above it.
 */
function OpenNote({ note, onOpen }: { note: NoteSummary; onOpen: (id: string) => void }) {
  return (
    <button
      type="button"
      aria-label={note.title || "Untitled note"}
      onClick={() => onOpen(note.id)}
      className="absolute inset-0 z-[1] cursor-pointer rounded-[inherit] outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
    />
  );
}

export function NoteCard({ note, ...h }: { note: NoteSummary } & NoteHandlers) {
  return (
    <div
      data-note-color={note.color}
      className={cn(
        "group relative mb-3 block break-inside-avoid rounded-xl border p-4 text-left transition-colors hover:border-foreground/25",
        note.color === "default" ? "bg-card" : "note-tint",
      )}
    >
      <OpenNote note={note} onOpen={h.onOpen} />
      <div className="absolute right-2 top-2 z-[2] hidden rounded-md border bg-popover/95 p-0.5 shadow-sm group-hover:flex group-focus-within:flex has-[[data-state=open]]:flex">
        <Actions note={note} {...h} />
      </div>
      {note.pinned && !note.archived && (
        <Pin aria-label="Pinned" className="absolute right-3 top-3 size-3.5 rotate-45 fill-primary text-primary group-hover:hidden group-focus-within:hidden" />
      )}
      {note.title && <h3 className="mb-2 pr-5 text-[14.5px] font-semibold leading-snug tracking-tight">{note.title}</h3>}
      {note.excerpt && <p className="line-clamp-6 whitespace-pre-line text-[13px] leading-relaxed text-foreground/75">{withBoxes(note.excerpt)}</p>}
      {!note.title && !note.excerpt && <p className="text-[13px] italic text-muted-foreground">Empty note</p>}
      <div className="mt-4 flex items-center gap-1.5">
        <CourseTag kind="note" id={note.id} />{note.project && <Badge>{note.project}</Badge>}
        {note.labels.slice(0, 3).map((l) => (
          <Badge key={l.id}>{l.name}</Badge>
        ))}
        {note.labels.length > 3 && <span className="text-[11px] text-muted-foreground">+{note.labels.length - 3}</span>}
        <span className="ml-auto text-[11px] text-muted-foreground">{timeAgo(note.edited_at)}</span>
      </div>
    </div>
  );
}

export function NoteRow({ note, ...h }: { note: NoteSummary } & NoteHandlers) {
  return (
    <div
      data-note-color={note.color}
      className="group relative flex h-11 items-center gap-3 px-4 transition-colors hover:bg-accent/50"
    >
      <OpenNote note={note} onOpen={h.onOpen} />
      <span aria-hidden className={cn("size-2 shrink-0 rounded-full", note.color === "default" ? "bg-border" : "note-dot")} />
      <span className="w-56 shrink-0 truncate text-[13px] font-medium">{note.title || "Untitled"}</span>
      <span className="min-w-0 flex-1 truncate text-[13px] text-muted-foreground">{withBoxes(note.excerpt).replace(/\s+/g, " ")}</span>
      <span className="hidden items-center gap-1.5 lg:flex">
        {note.labels.slice(0, 2).map((l) => (
          <Badge key={l.id}>{l.name}</Badge>
        ))}
      </span>
      {note.pinned && !note.archived && <Pin aria-label="Pinned" className="size-3.5 rotate-45 fill-primary text-primary group-hover:hidden group-focus-within:hidden" />}
      <span className="w-20 shrink-0 text-right text-[11px] text-muted-foreground group-hover:hidden group-focus-within:hidden group-has-[[data-state=open]]:hidden">{timeAgo(note.edited_at)}</span>
      <div className="relative z-[2] hidden w-20 shrink-0 justify-end group-hover:flex group-focus-within:flex has-[[data-state=open]]:flex">
        <Actions note={note} {...h} />
      </div>
    </div>
  );
}

/** A pinned note: wider, taller, with an accent edge. Used only in the Pinned row. */
export function PinnedCard({ note, ...h }: { note: NoteSummary } & NoteHandlers) {
  return (
    <div
      data-note-color={note.color}
      className={cn(
        "group relative flex min-h-36 flex-col overflow-hidden rounded-xl border p-4 pt-5 text-left transition-colors hover:border-foreground/25",
        note.color === "default" ? "bg-card" : "note-tint",
      )}
    >
      <OpenNote note={note} onOpen={h.onOpen} />
      <span aria-hidden className="absolute inset-x-0 top-0 h-[3px] bg-primary" />
      <div className="absolute right-2 top-3 z-[2] hidden rounded-md border bg-popover/95 p-0.5 shadow-sm group-hover:flex group-focus-within:flex has-[[data-state=open]]:flex">
        <Actions note={note} {...h} />
      </div>
      <Pin aria-label="Pinned" className="absolute right-3.5 top-4 size-3.5 rotate-45 fill-primary text-primary group-hover:hidden group-focus-within:hidden" />
      <h3 className="mb-2 pr-6 text-[15px] font-semibold leading-snug tracking-tight">{note.title || "Untitled"}</h3>
      <p className="line-clamp-3 whitespace-pre-line text-[13px] leading-relaxed text-foreground/75">{withBoxes(note.excerpt)}</p>
      <div className="mt-auto flex items-center gap-1.5 pt-4">
        <CourseTag kind="note" id={note.id} />{note.project && <Badge>{note.project}</Badge>}
        {note.labels.slice(0, 2).map((l) => <Badge key={l.id}>{l.name}</Badge>)}
        <span className="ml-auto text-[11px] text-muted-foreground">{timeAgo(note.edited_at)}</span>
      </div>
    </div>
  );
}
