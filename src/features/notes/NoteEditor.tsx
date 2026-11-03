import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { ConfirmAction } from "@/components/ConfirmAction";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Archive, ArchiveRestore, Bold, Check, CheckSquare, Eye, FolderKanban, Heading2, Italic, List, ListOrdered, Palette, Pencil, Pin, PinOff, Plus, SquareArrowOutUpRight, Tag, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Checkbox } from "@/components/ui/checkbox";
import { mod } from "@/lib/platform";
import { cn } from "@/lib/utils";
import { timeAgo } from "@/lib/time";
import { ipc, NOTE_COLORS, type Note, type NoteColor } from "@/ipc";
import { keys, useAllTasks, useLabels, useNoteActions, useNotes, useTasksQuery } from "@/hooks/usePluto";
import { TaskDetail } from "@/features/tasks/TaskDetail";
import { STATUS_LABEL } from "@/lib/taskStatus";
import { COLOR_NAMES } from "./colors";
import { MarkdownView } from "./MarkdownView";
import { CoursePicker } from "@/features/student/CoursePicker";
import { NOTE_FONTS, continueList, fontClass, lines, toggleCheck, wrap, type Edit, type LineKind, type NoteFont } from "./format";

const FONT_KEY = ["prefs", "ui.note_font"] as const;

/** The font for notes (Sans, Serif or Mono), one choice for every note. */
function useNoteFont() {
  const qc = useQueryClient();
  const { data } = useQuery({ queryKey: FONT_KEY, queryFn: async () => (await ipc.prefsGet<NoteFont>("ui.note_font")) ?? "sans" });
  const set = async (f: NoteFont) => { qc.setQueryData(FONT_KEY, f); await ipc.prefsSet("ui.note_font", f); };
  return { font: data ?? "sans", set };
}

/** Every project name in use, from tasks and notes. */
export function useProjectNames(): string[] {
  const { data: tasks = [] } = useAllTasks();
  const { data: notes = [] } = useNotes(false, null, "last_edited");
  return [...new Set([...tasks.map((r) => r.task.project), ...notes.map((n) => n.project)].filter((p): p is string => !!p))].sort((a, b) => a.localeCompare(b));
}

const TITLE_MAX = 200;
const BODY_MAX = 100_000;

/** `fullWindow`: the note fills its own window (opened with "Own window"). */
export function NoteEditor({ noteId, onClose, fullWindow = false }: { noteId: string | null; onClose: () => void; fullWindow?: boolean }) {
  const { data: note } = useQuery({
    queryKey: noteId ? keys.note(noteId) : ["note", "none"],
    queryFn: () => ipc.noteGet(noteId ?? ""),
    enabled: noteId !== null,
  });
  return (
    <Dialog open={noteId !== null} onOpenChange={(o) => !o && onClose()}>
      {note && noteId === note.id && <EditorBody key={note.id} note={note} onClose={onClose} fullWindow={fullWindow} />}
    </Dialog>
  );
}

/** FR-42: the tasks that link to this note; each opens its task. */
function LinkedTasks({ noteId }: { noteId: string }) {
  const { data: rows = [] } = useTasksQuery("all", { note_id: noteId });
  const [open, setOpen] = useState<string | null>(null);
  if (rows.length === 0) return null;
  return (
    <section aria-label="Linked tasks" className="pt-3">
      <p className="mb-1 text-xs font-medium text-muted-foreground">Linked tasks</p>
      <ul className="flex flex-wrap gap-1.5">
        {rows.map(({ task }) => (
          <li key={task.id}>
            <button
              type="button" onClick={() => setOpen(task.id)} aria-label={`Open task ${task.title}`}
              className="rounded-md border border-foreground/15 px-2 py-1 text-xs hover:bg-foreground/8"
            >
              {task.title} <span className="text-muted-foreground">· {STATUS_LABEL[task.status]}</span>
            </button>
          </li>
        ))}
      </ul>
      <TaskDetail taskId={open} onClose={() => setOpen(null)} />
    </section>
  );
}

function EditorBody({ note, onClose, fullWindow }: { note: Note; onClose: () => void; fullWindow: boolean }) {
  const actions = useNoteActions();
  const { font, set: setFont } = useNoteFont();
  const projects = useProjectNames();
  const [project, setProject] = useState(note.project ?? "");
  const { data: labelCounts = [] } = useLabels();
  const [title, setTitle] = useState(note.title);
  const [body, setBody] = useState(note.body);
  const [status, setStatus] = useState<"saved" | "saving">("saved");
  const [newLabel, setNewLabel] = useState("");
  const [preview, setPreview] = useState(false);
  const [labelsOpen, setLabelsOpen] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const latest = useRef({ title, body });
  latest.current = { title, body };
  const dirty = useRef(false);
  const titleRef = useRef<HTMLInputElement>(null);
  const bodyRef = useRef<HTMLTextAreaElement>(null);

  // Where the caret goes after the next render: set right after React writes the new text, so
  // nothing typed in between can land in the wrong place.
  const caret = useRef<{ start: number; end: number; focus: boolean } | null>(null);
  useLayoutEffect(() => {
    const el = bodyRef.current;
    const c = caret.current;
    if (!el || !c) return;
    caret.current = null;
    if (c.focus) el.focus();
    el.setSelectionRange(c.start, c.end);
  }, [body]);

  const flush = async () => {
    clearTimeout(timer.current);
    if (!dirty.current) return;
    dirty.current = false;
    await actions.update(note.id, latest.current);
    setStatus("saved");
  };
  const edit = (next: { title?: string; body?: string }) => {
    if (next.title !== undefined) setTitle(next.title);
    if (next.body !== undefined) setBody(next.body);
    dirty.current = true;
    setStatus("saving");
    clearTimeout(timer.current);
    timer.current = setTimeout(() => void flush(), 500);
  };
  useEffect(() => () => clearTimeout(timer.current), []);

  const close = async () => {
    await flush();
    await actions.discardIfEmpty(note.id);
    onClose();
  };
  /** Owner 2026-10-25: keep writing in a window of its own; the dashboard is free again. */
  const popOut = async () => {
    await flush();
    await ipc.noteWindowOpen(note.id);
    onClose();
  };
  /** A toolbar button: change the text and keep the selection on what was changed. */
  const format = (f: (e: Edit) => Edit) => {
    const el = bodyRef.current;
    if (!el) return;
    const next = f({ text: body, start: el.selectionStart, end: el.selectionEnd });
    caret.current = { start: next.start, end: next.end, focus: true };
    edit({ body: next.text });
  };
  const line = (kind: LineKind) => format((e) => lines(e, kind));
  const saveProject = () => {
    if ((note.project ?? "") !== project.trim()) void actions.update(note.id, { project: project.trim() });
  };
  const labelIds = note.labels.map((l) => l.id);
  const toggleLabel = (id: string) =>
    void actions.update(note.id, { label_ids: labelIds.includes(id) ? labelIds.filter((x) => x !== id) : [...labelIds, id] });
  const addLabel = async () => {
    const name = newLabel.trim();
    if (!name) return;
    const label = await actions.createLabel(name);
    setNewLabel("");
    await actions.update(note.id, { label_ids: [...labelIds, label.id] });
  };

  return (
    <DialogContent
      data-note-color={note.color}
      aria-describedby="note-editor-desc"
      onOpenAutoFocus={(e) => {
        e.preventDefault();
        const el = note.title ? bodyRef.current : titleRef.current;
        el?.focus();
        el?.setSelectionRange(el.value.length, el.value.length);
      }}
      onInteractOutside={(e) => {
        e.preventDefault();
        void close();
      }}
      onEscapeKeyDown={(e) => {
        e.preventDefault();
        void close();
      }}
      onKeyDown={(e) => {
        if (!(e.ctrlKey || e.metaKey)) return;
        const k = e.key.toLowerCase();
        if (k === "d" && !e.shiftKey) { e.preventDefault(); void actions.pin(note.id, !note.pinned); }
        else if (k === "e" && !e.shiftKey) { e.preventDefault(); setPreview((p) => !p); }
        else if (k === "a" && e.shiftKey) { e.preventDefault(); void actions.archive(note.id, !note.archived).then(onClose); }
        else if (k === "l" && !e.shiftKey) { e.preventDefault(); setLabelsOpen(true); }
        else if (k === "b" && !e.shiftKey && !preview) { e.preventDefault(); format((x) => wrap(x, "**")); }
        else if (k === "i" && !e.shiftKey && !preview) { e.preventDefault(); format((x) => wrap(x, "_")); }
      }}
      showClose={false}
      className={cn("gap-0 overflow-hidden", fullWindow && "left-0 top-0 flex h-screen w-screen max-w-none translate-x-0 translate-y-0 flex-col rounded-none border-0 data-[state=open]:zoom-in-100", note.color !== "default" && "note-tint")}
    >
      <DialogTitle className="sr-only">Edit note</DialogTitle>
      <DialogDescription id="note-editor-desc" className="sr-only">Changes save automatically.</DialogDescription>
      <div role="toolbar" aria-label="Formatting" className="flex flex-wrap items-center gap-0.5 border-b border-foreground/10 px-4 py-2">
        <ToolButton label={`Bold (${mod}+B)`} onClick={() => format((e) => wrap(e, "**"))} disabled={preview}><Bold /></ToolButton>
        <ToolButton label={`Italic (${mod}+I)`} onClick={() => format((e) => wrap(e, "_"))} disabled={preview}><Italic /></ToolButton>
        <ToolButton label="Heading" onClick={() => line("heading")} disabled={preview}><Heading2 /></ToolButton>
        <span aria-hidden className="mx-1 h-5 w-px bg-foreground/15" />
        <ToolButton label="Numbered list" onClick={() => line("number")} disabled={preview}><ListOrdered /></ToolButton>
        <ToolButton label="Bullet list" onClick={() => line("bullet")} disabled={preview}><List /></ToolButton>
        <ToolButton label="Checklist" onClick={() => line("check")} disabled={preview}><CheckSquare /></ToolButton>
        <span aria-hidden className="mx-1 h-5 w-px bg-foreground/15" />
        <select aria-label="Font" value={font} onChange={(e) => void setFont(e.target.value as NoteFont)}
          className="h-7 rounded-md border border-foreground/15 bg-transparent px-1.5 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring">
          {NOTE_FONTS.map((f) => <option key={f.value} value={f.value} className="bg-popover">{f.label}</option>)}
        </select>
        <div className="ml-auto"><CoursePicker kind="note" id={note.id} compact /></div>
        <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
          <FolderKanban className="size-3.5" />
          <input value={project} onChange={(e) => setProject(e.target.value)} onBlur={saveProject} onKeyDown={(e) => e.key === "Enter" && (e.currentTarget as HTMLInputElement).blur()}
            list="note-projects" maxLength={100} placeholder="No project" aria-label="Project"
            className="h-7 w-36 rounded-md border border-foreground/15 bg-transparent px-2 text-xs text-foreground outline-none placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring" />
          <datalist id="note-projects">{projects.map((p) => <option key={p} value={p} />)}</datalist>
        </label>
        {!fullWindow && (
          <Button variant="ghost" size="sm" className="ml-1 h-7 px-2 text-xs" onClick={() => void popOut()} title="Keep writing in a window of its own">
            <SquareArrowOutUpRight />Own window
          </Button>
        )}
      </div>
      <div className={cn("flex flex-col gap-1 px-6 pb-2 pt-5", fullWindow && "min-h-0 flex-1", fontClass(font))}>
        <Input
          ref={titleRef}
          value={title}
          maxLength={TITLE_MAX}
          onChange={(e) => edit({ title: e.target.value })}
          placeholder="Title"
          aria-label="Title"
          className="h-auto border-0 bg-transparent px-0 text-xl font-semibold tracking-tight shadow-none focus-visible:ring-0"
        />
        {preview ? (
          <div aria-label="Preview" className={cn("min-h-64 overflow-y-auto", fullWindow ? "flex-1" : "max-h-[50vh]")}><MarkdownView source={body} onToggle={(n) => edit({ body: toggleCheck(body, n) })} /></div>
        ) : (
          <textarea
            ref={bodyRef}
            value={body}
            maxLength={BODY_MAX}
            onChange={(e) => edit({ body: e.target.value })}
            onKeyDown={(e) => {
              if (e.key !== "Enter" || e.shiftKey || e.ctrlKey || e.metaKey || e.altKey || e.nativeEvent.isComposing) return;
              const el = e.currentTarget;
              const next = continueList({ text: body, start: el.selectionStart, end: el.selectionEnd });
              if (!next) return;
              e.preventDefault();
              caret.current = { start: next.start, end: next.end, focus: false };
              edit({ body: next.text });
            }}
            placeholder="Take a note…"
            aria-label="Note"
            className={cn("min-h-64 resize-none overflow-y-auto bg-transparent text-[15px] leading-relaxed outline-none placeholder:text-muted-foreground", fullWindow ? "flex-1" : "max-h-[50vh]")}
          />
        )}
        <LinkedTasks noteId={note.id} />
        {note.labels.length > 0 && (
          <div className="flex flex-wrap gap-1.5 pt-2">
            {note.labels.map((l) => (
              <ConfirmAction key={l.id} message={`Take “${l.name}” off this note?`} verb="Remove" onConfirm={() => toggleLabel(l.id)}>
              <button
                type="button"
                title={`Remove “${l.name}”`}
                className="rounded-full bg-foreground/10 px-2.5 py-0.5 text-xs font-medium hover:bg-foreground/20"
              >
                {l.name} ×
              </button>
              </ConfirmAction>
            ))}
          </div>
        )}
      </div>
      <div className="flex items-center gap-1 border-t border-foreground/10 px-4 py-3">
        <Popover>
          <PopoverTrigger asChild>
            <Button variant="ghost" size="icon-sm" aria-label="Colour" title="Colour"><Palette /></Button>
          </PopoverTrigger>
          <PopoverContent side="top" className="w-auto">
            <div className="grid grid-cols-5 gap-2" role="radiogroup" aria-label="Note colour">
              {NOTE_COLORS.map((c: NoteColor) => (
                <button
                  key={c}
                  type="button"
                  role="radio"
                  aria-checked={note.color === c}
                  aria-label={COLOR_NAMES[c]}
                  title={COLOR_NAMES[c]}
                  data-note-color={c}
                  onClick={() => void actions.update(note.id, { color: c })}
                  className={cn(
                    "note-dot grid size-8 place-items-center rounded-full outline-none ring-offset-2 ring-offset-popover focus-visible:ring-2 focus-visible:ring-ring",
                    c === "default" && "!bg-secondary",
                    note.color === c && "ring-2 ring-foreground/70",
                  )}
                >
                  {note.color === c && <Check className="size-4 text-background" strokeWidth={3} />}
                </button>
              ))}
            </div>
          </PopoverContent>
        </Popover>
        <Popover open={labelsOpen} onOpenChange={setLabelsOpen}>
          <PopoverTrigger asChild>
            <Button variant="ghost" size="icon-sm" aria-label="Labels" title="Labels"><Tag /></Button>
          </PopoverTrigger>
          <PopoverContent side="top">
            <ul className="mb-2 max-h-48 space-y-0.5 overflow-y-auto">
              {labelCounts.map(({ label }) => (
                <li key={label.id}>
                  <label className="flex cursor-pointer items-center gap-2.5 rounded-lg px-2 py-1.5 hover:bg-foreground/8">
                    <Checkbox checked={labelIds.includes(label.id)} onCheckedChange={() => toggleLabel(label.id)} className="rounded-md" />
                    <span className="truncate">{label.name}</span>
                  </label>
                </li>
              ))}
              {labelCounts.length === 0 && <li className="px-2 py-1.5 text-muted-foreground">No labels yet.</li>}
            </ul>
            <form
              className="flex gap-2"
              onSubmit={(e) => {
                e.preventDefault();
                void addLabel();
              }}
            >
              <Input value={newLabel} maxLength={50} onChange={(e) => setNewLabel(e.target.value)} placeholder="New label" aria-label="New label" className="h-8" />
              <Button type="submit" size="icon-sm" variant="secondary" aria-label="Add label"><Plus /></Button>
            </form>
          </PopoverContent>
        </Popover>
        <Button variant="ghost" size="icon-sm" aria-label={note.pinned ? "Unpin" : "Pin"} title={note.pinned ? "Unpin" : "Pin"} onClick={() => void actions.pin(note.id, !note.pinned)}>
          {note.pinned ? <PinOff /> : <Pin />}
        </Button>
        <Button variant="ghost" size="icon-sm" aria-label={note.archived ? "Unarchive" : "Archive"} title={note.archived ? "Unarchive" : "Archive"} onClick={() => void actions.archive(note.id, !note.archived).then(onClose)}>
          {note.archived ? <ArchiveRestore /> : <Archive />}
        </Button>
        <ConfirmAction message="Move this note to the Bin?" onConfirm={() => void actions.remove(note.id).then(onClose)}>
          <Button variant="ghost" size="icon-sm" aria-label="Delete" title="Delete" className="hover:text-destructive">
            <Trash2 />
          </Button>
        </ConfirmAction>
        <Button
          variant="ghost" size="icon-sm" aria-pressed={preview} aria-label={preview ? "Edit" : "Preview"} title={`${preview ? "Edit" : "Preview"} (${mod}+E)`}
          onClick={() => setPreview((p) => !p)}
        >
          {preview ? <Pencil /> : <Eye />}
        </Button>
        <span className="ml-auto mr-2 text-xs text-foreground/50" aria-live="polite">
          {status === "saving" ? "Saving…" : `Edited ${timeAgo(note.edited_at)}`}
        </span>
        <Button size="sm" onClick={() => void close()}>Done</Button>
      </div>
    </DialogContent>
  );
}

function ToolButton({ label, onClick, disabled, children }: { label: string; onClick: () => void; disabled?: boolean; children: React.ReactNode }) {
  return (
    <button type="button" aria-label={label} title={label} onClick={onClick} disabled={disabled}
      className="grid size-7 place-items-center rounded-md text-muted-foreground outline-none hover:bg-foreground/10 hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-40 [&_svg]:size-4">
      {children}
    </button>
  );
}
