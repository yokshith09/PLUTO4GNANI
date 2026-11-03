import { useEffect, useState } from "react";
import { ConfirmAction } from "@/components/ConfirmAction";
import { Presets } from "@/components/Presets";
import { dayPresets } from "@/lib/datePresets";
import { AlignLeft, Bell, CalendarClock, CalendarPlus, Check, ChevronRight, Timer, ExternalLink, FilePlus2, Flag, FolderKanban, History, Lightbulb, Link2, MessageSquarePlus, Trash2, X } from "lucide-react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { NoteEditor } from "@/features/notes/NoteEditor";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { useAllTasks, useNotes, useTask, useTaskActions, useTaskHistory, useWorkUpdates } from "@/hooks/usePluto";
import { overdueLabel, ymd } from "@/lib/dates";
import { STATUS_LABEL, isFinished, transitionsFrom } from "@/lib/taskStatus";
import { clockTime, formatWhen } from "@/lib/time";
import { errorMessage, ipc, type Priority, type Task, type TaskEvent } from "@/ipc";
import { notify } from "@/lib/notify";
import { clock as focusClock, hoursLabel } from "@/lib/focus";
import { toEdit } from "./edit";
import { useFocus, useFocusLength } from "@/hooks/useFocus";
import { FocusLength } from "@/features/focus/FocusTile";
import { TaskReminders } from "./TaskReminders";
import { StarButton } from "./TaskBits";
import { CoursePicker } from "@/features/student/CoursePicker";
import { STEPS, dueLine, stepIndex, taskHints } from "./hints";
import { LinksEditor, PlanEditor, RepeatHistory, planOf, planStateOf, samePlan } from "./TaskPlanEditor";

const NONE = "none";
const PRIORITIES = [{ value: NONE, label: "None" }, { value: "LOW", label: "Low" }, { value: "MEDIUM", label: "Medium" }, { value: "HIGH", label: "High" }];
const DELETE_WINDOW_MS = 5 * 60_000;

/** A heading with one short line saying what the section is for (owner 2026-10-25: "what is deadline, reminder, note?"). */
function SectionHead({ icon, title, hint, count }: { icon: React.ReactNode; title: string; hint: string; count?: number }) {
  return (
    <div>
      <h3 className="flex items-center gap-1.5 text-[13px] font-semibold [&_svg]:size-4 [&_svg]:text-muted-foreground">{icon}{title}{count !== undefined && count > 0 && <span className="text-xs font-normal tabular-nums text-muted-foreground">{count}</span>}</h3>
      <p className="mt-0.5 text-xs text-muted-foreground">{hint}</p>
    </div>
  );
}

/** One setting of the task in its own card. */
/** A smaller part inside a card, with its own one-line hint. */
function Sub({ icon, title, hint, children, first = false }: { icon: React.ReactNode; title: string; hint: string; children: React.ReactNode; first?: boolean }) {
  return (
    <section aria-label={title} className={`space-y-2.5 ${first ? "" : "border-t pt-4"}`}>
      <SectionHead icon={icon} title={title} hint={hint} />
      {children}
    </section>
  );
}

function HintButton({ action, run }: { action: { id: "move-today" | "start" | "set-today"; label: string }; run: (id: "move-today" | "start" | "set-today") => Promise<void> }) {
  return <Button size="sm" variant="outline" onClick={() => void run(action.id)}>{action.label}</Button>;
}

/** Planned, In progress, Completed: where the task is, at a glance. */
function StatusTrack({ status }: { status: Task["status"] }) {
  const at = stepIndex(status);
  if (at < 0) return <Badge>{STATUS_LABEL[status]}</Badge>;
  return (
    <ol aria-label="Progress" className="flex items-center gap-1.5 text-xs">
      {STEPS.map((st, i) => (
        <li key={st.id} aria-current={i === at ? "step" : undefined} className="flex items-center gap-1.5">
          {i > 0 && <ChevronRight aria-hidden className="size-3 text-muted-foreground/60" />}
          <span className={`flex items-center gap-1 rounded-full px-2 py-0.5 font-medium ${i === at ? "bg-primary/15 text-primary" : i < at ? "text-foreground" : "text-muted-foreground"}`}>
            {i < at && <Check aria-hidden className="size-3" />}
            {i === at && status === "PENDING" ? "Paused" : st.label}
          </span>
        </li>
      ))}
    </ol>
  );
}

const day = (s: string | null) => (s ? new Date(`${s}T00:00:00`).toLocaleDateString([], { month: "short", day: "numeric" }) : "");

function describe(e: TaskEvent): string {
  switch (e.kind) {
    case "CREATED": return "Created";
    case "STATUS": return `${STATUS_LABEL[(e.from_status ?? "PLANNED") as Task["status"]] ?? e.from_status} → ${STATUS_LABEL[(e.to_status ?? "PLANNED") as Task["status"]] ?? e.to_status}`;
    case "DEFERRED": return `Moved from ${day(e.from_date) || "no day"} to ${day(e.to_date)}`;
    case "EDITED": return e.to_date !== null ? `Planned day set to ${day(e.to_date)}` : "Edited";
    default: return e.kind.charAt(0) + e.kind.slice(1).toLowerCase();
  }
}

/** Everything about one task: edit it, move it through its life, defer it, log progress, see its history. */
export function TaskDetail({ taskId, onClose }: { taskId: string | null; onClose: () => void }) {
  const { data: task } = useTask(taskId);
  return (
    <Dialog open={taskId !== null} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="flex max-h-[calc(100vh-2rem)] w-[min(48rem,calc(100vw-2rem))] max-w-none flex-col gap-0 overflow-hidden p-0 sm:max-w-none">
        {task ? <Body key={task.id} task={task} onClose={onClose} /> : <div className="p-8 text-center text-[13px] text-muted-foreground"><DialogTitle className="sr-only">Task</DialogTitle>Loading…</div>}
      </DialogContent>
    </Dialog>
  );
}

function Body({ task, onClose }: { task: Task; onClose: () => void }) {
  const actions = useTaskActions();
  const focus = useFocus();
  const { data: focusedMs = 0 } = useQuery({ queryKey: ["focus-total", task.id, focus.session?.id ?? null], queryFn: () => ipc.focusTaskTotal(task.id) });
  const { minutes, set: setMinutes } = useFocusLength();
  const { data: allTasks = [] } = useAllTasks();
  const { data: notes = [] } = useNotes(false, null, "last_edited");
  const { data: history = [] } = useTaskHistory(task.id);
  const { data: updates = [] } = useWorkUpdates(task.id);
  const [title, setTitle] = useState(task.title);
  const [description, setDescription] = useState(task.description);
  const [project, setProject] = useState(task.project ?? "");
  const [priority, setPriority] = useState<string>(task.priority ?? NONE);
  const [planned, setPlanned] = useState(task.planned_date ?? "");
  const [due, setDue] = useState(task.due_date ?? "");
  const [noteId, setNoteId] = useState(task.note_id ?? "");
  const [plan, setPlan] = useState(() => planStateOf(task));
  const planChanged = !samePlan(planOf(plan), planOf(planStateOf(task)));
  const [update, setUpdate] = useState("");
  const [deferTo, setDeferTo] = useState("");
  const [openNote, setOpenNote] = useState<string | null>(null);
  const qc = useQueryClient();
  /** FR-42: make a note from this task (title and details), link it, and open it. */
  const noteFromTask = async () => {
    try {
      const note = await ipc.noteCreate({ title: title.trim() || task.title, body: description });
      await actions.update(task.id, { ...toEdit(task), note_id: note.id });
      setNoteId(note.id);
      await qc.invalidateQueries({ queryKey: ["notes"] });
      setOpenNote(note.id);
    } catch (e) {
      notify.error(errorMessage(e));
    }
  };
  const [now, setNow] = useState(Date.now());
  useEffect(() => { const t = setInterval(() => setNow(Date.now()), 30_000); return () => clearInterval(t); }, []);

  const finished = isFinished(task.status);
  const dirty = title.trim() !== task.title || description !== task.description || project.trim() !== (task.project ?? "")
    || priority !== (task.priority ?? NONE) || planned !== (task.planned_date ?? "") || due !== (task.due_date ?? "") || noteId !== (task.note_id ?? "") || planChanged;
  const tomorrow = ymd(new Date(new Date().setDate(new Date().getDate() + 1)));

  const save = async () => {
    if (!title.trim()) return notify.warning("Give the task a title.");
    // Nothing changed: Save still closes, like the owner expects (never a dead button).
    if (!dirty) return onClose();
    if (plan.mode === "repeat" && plan.repeat.kind === "weekly" && plan.repeat.days.length === 0) return notify.warning("Pick at least one day of the week.");
    if (plan.mode === "days" && plan.days.length === 0) return notify.warning("Pick at least one day.");
    await actions.update(task.id, {
      ...toEdit(task), title: title.trim(), description, project: project.trim() || null,
      priority: priority === NONE ? null : (priority as Priority), planned_date: planned || null, due_date: due || null, note_id: noteId || null,
    });
    if (planChanged) await actions.setPlan(task.id, planOf(plan));
    notify.success("Task saved");
    // FR-41: after an edit, back to the screen the task was opened from (Today or Tasks).
    onClose();
  };
  const addUpdate = async () => {
    if (!update.trim()) return;
    await actions.addUpdate(task.id, update);
    setUpdate("");
  };

  const todayStr = ymd(new Date(now));
  const draft: Task = { ...task, planned_date: planned || null, due_date: due || null };
  const hints = taskHints(draft, todayStr);
  const dueText = finished ? "" : dueLine(draft, todayStr);
  const primaryTo = task.status === "IN_PROGRESS" ? "COMPLETED" : finished ? "PLANNED" : "IN_PROGRESS";
  const runHint = async (id: "move-today" | "start" | "set-today") => {
    if (id === "start") return void actions.transition(task.id, "IN_PROGRESS");
    // These only fill the form; Save makes them stick, like any other edit.
    if (id === "move-today") setDue(todayStr);
    setPlanned(todayStr);
  };
  const overdueText = due && due < todayStr && !finished ? overdueLabel(due, todayStr) : null;
  const projects = [...new Set(allTasks.map((r) => r.task.project).filter((p): p is string => !!p))].sort();

  const others = transitionsFrom(task.status).filter((t) => t.to !== primaryTo);
  const primary = transitionsFrom(task.status).find((t) => t.to === primaryTo);
  // What is set in "Advanced options", so it can be seen without opening it.
  const extras = [
    due && `Deadline ${day(due)}`,
    project.trim() && project.trim(),
    noteId && "Note linked",
    description.trim() && "Details",
    updates.length > 0 && `${updates.length} update${updates.length === 1 ? "" : "s"}`,
  ].filter(Boolean).join(" · ");

  return (
    <>
      {/* The essentials only (owner 2026-10-27: "so many buttons, it feels confusing"): title, the one
          next step, when, priority, Save. Everything else is in "Advanced options". */}
      <div className="border-b px-6 pb-4 pt-5 pr-14">
        <DialogDescription className="mb-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
          <span className="font-medium uppercase tracking-wide">{STATUS_LABEL[task.status]}</span>
          {task.project && <span className="flex items-center gap-1 rounded-md bg-muted px-1.5 py-0.5"><FolderKanban aria-hidden className="size-3" />{task.project}</span>}
          {dueText && <span className={overdueText ? "font-medium text-destructive" : "font-medium text-foreground"}>{dueText}</span>}
          {focusedMs > 0 && <span className="tabular-nums" title="Finished focus sessions on this task">Focused {hoursLabel(focusedMs)}</span>}
          {focus.session?.task_id === task.id && <span className="font-medium text-primary tabular-nums">Focusing · {focusClock(focus.remaining)} left</span>}
        </DialogDescription>
        <div className="flex items-center gap-2">
          <DialogTitle className="sr-only">Task: {task.title}</DialogTitle>
          <input
            value={title} onChange={(e) => setTitle(e.target.value)} maxLength={300} aria-label="Title"
            className="min-w-0 flex-1 bg-transparent text-xl font-semibold tracking-tight outline-none placeholder:text-muted-foreground"
          />
          <StarButton starred={task.starred} title={task.title} onToggle={() => void actions.star(task.id, !task.starred)} className="opacity-100" />
          {primary && (
            <div role="group" aria-label="Status">
              <Button size="sm" onClick={() => void actions.transition(task.id, primary.to)}>{primary.label}</Button>
            </div>
          )}
        </div>
      </div>

      <div className="min-h-0 flex-1 space-y-5 overflow-y-auto px-6 py-5">
        {hints[0] && (
          <p aria-label="Suggestions" className="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-lg bg-primary/5 px-3 py-2 text-[13px]">
            <Lightbulb aria-hidden className="size-4 shrink-0 text-primary" />
            <span className="min-w-0 flex-1">{hints[0].text}</span>
            {hints[0].action && <HintButton action={hints[0].action} run={runHint} />}
          </p>
        )}

        <div className="grid gap-5 sm:grid-cols-[minmax(0,1.4fr)_minmax(0,1fr)]">
          <section aria-label="When" className="space-y-1.5">
            <p className="flex items-center gap-1.5 text-xs font-medium text-muted-foreground"><CalendarPlus aria-hidden className="size-3.5" />When <span className="font-normal">· the day it shows on Today</span></p>
            <PlanEditor value={plan} onChange={setPlan} planned={planned} onPlanned={setPlanned} />
            <RepeatHistory task={task} />
          </section>
          <section className="space-y-1.5">
            <p className="flex items-center gap-1.5 text-xs font-medium text-muted-foreground"><Flag aria-hidden className="size-3.5" />Priority</p>
            <ToggleGroup type="single" value={priority} onValueChange={(v) => v && setPriority(v)} aria-label="Priority" className="w-full">
              {PRIORITIES.map((p) => <ToggleGroupItem key={p.value} value={p.value} className="flex-1 px-2">{p.label}</ToggleGroupItem>)}
            </ToggleGroup>
          </section>
        </div>

        <details className="group rounded-lg border" aria-label="Advanced options">
          <summary className="flex cursor-pointer list-none items-center gap-2 px-4 py-3 text-[13px] font-semibold outline-none focus-visible:ring-2 focus-visible:ring-ring [&::-webkit-details-marker]:hidden">
            <ChevronRight aria-hidden className="size-4 text-muted-foreground transition-transform group-open:rotate-90" />
            Advanced options
            <span className="min-w-0 flex-1 truncate text-xs font-normal text-muted-foreground">{extras || "Details, deadline, reminder, project, note, links, focus"}</span>
          </summary>
          <div className="grid gap-6 border-t p-4 md:grid-cols-2">
            <div className="min-w-0 space-y-5">
              <section aria-label="Details" className="space-y-2">
                <SectionHead icon={<AlignLeft />} title="Details" hint="What this task is about." />
                <textarea
                  value={description} onChange={(e) => setDescription(e.target.value)} rows={4} maxLength={20000} aria-label="Description" placeholder="Add details…"
                  className="w-full resize-none rounded-md border border-input bg-transparent px-3 py-2 text-[13.5px] leading-relaxed outline-none placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring"
                />
              </section>
              <Sub first icon={<CalendarClock />} title="Deadline" hint="The last day it can be done. After that it shows as overdue.">
                <div className="flex gap-2">
                  <Input id="td-due" type="date" value={due} onChange={(e) => setDue(e.target.value)} aria-label="Deadline date" />
                  {due && <Button size="sm" variant="ghost" onClick={() => setDue("")}>Clear</Button>}
                </div>
                <Presets label="Deadline presets" options={dayPresets()} value={due} onPick={setDue} />
              </Sub>
              <Sub icon={<Bell />} title="Reminder" hint="A notification and the pet ring before it starts.">
                <TaskReminders task={task} />
              </Sub>
              {!finished && plan.mode === "one" && (
                <Sub icon={<CalendarPlus />} title="Move it" hint="Push it to another day now.">
                  <div className="flex flex-wrap items-center gap-2">
                    <Button size="sm" variant="outline" aria-label="Defer to tomorrow" onClick={() => void actions.defer(task.id, tomorrow)}>Tomorrow</Button>
                    <Input type="date" value={deferTo} min={tomorrow} onChange={(e) => setDeferTo(e.target.value)} aria-label="Defer to" className="h-8 w-36" />
                    <Button size="sm" variant="outline" disabled={!deferTo} onClick={() => void actions.defer(task.id, deferTo).then(() => setDeferTo(""))}>Move</Button>
                  </div>
                </Sub>
              )}
            </div>
            <div className="min-w-0 space-y-5">
              <div className="space-y-1.5">
                <label htmlFor="td-project" className="flex items-center gap-1.5 text-xs font-medium text-muted-foreground"><FolderKanban aria-hidden className="size-3.5" />Project</label>
                <Input id="td-project" value={project} maxLength={100} onChange={(e) => setProject(e.target.value)} placeholder="No project" aria-label="Project name" list="td-projects" />
                <datalist id="td-projects">{projects.map((p) => <option key={p} value={p} />)}</datalist>
              </div>
              <CoursePicker kind="task" id={task.id} />
              <div className="space-y-1.5">
                <label htmlFor="td-note" className="flex items-center gap-1.5 text-xs font-medium text-muted-foreground"><Link2 aria-hidden className="size-3.5" />Note</label>
                <div className="flex gap-2">
                  <select id="td-note" aria-label="Linked note" value={noteId} onChange={(e) => setNoteId(e.target.value)} className="h-8 min-w-0 flex-1 rounded-md border border-input bg-card px-2 text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-ring">
                    <option value="">No note</option>
                    {notes.map((n) => <option key={n.id} value={n.id}>{n.title || "Untitled"}</option>)}
                  </select>
                  {noteId ? (
                    <Button size="sm" variant="outline" aria-label="Open note" onClick={() => setOpenNote(noteId)}><ExternalLink />Open</Button>
                  ) : (
                    <Button size="sm" variant="outline" aria-label="New note from this task" onClick={() => void noteFromTask()}><FilePlus2 />New note</Button>
                  )}
                </div>
              </div>
              <LinksEditor task={task} />
              {!finished && !focus.session && (
                <div className="space-y-1.5">
                  <p className="flex items-center gap-1.5 text-xs font-medium text-muted-foreground"><Timer aria-hidden className="size-3.5" />Focus on it</p>
                  <div className="flex items-center gap-2">
                    <FocusLength minutes={minutes} onChange={(m) => void setMinutes(m)} />
                    <Button size="sm" variant="outline" onClick={() => void focus.start("FOCUS", minutes, task.id)}><Timer />Focus {minutes} min</Button>
                  </div>
                </div>
              )}
              {others.length > 0 && (
                <div className="space-y-1.5">
                  <p className="text-xs font-medium text-muted-foreground">Status</p>
                  <StatusTrack status={task.status} />
                  <div className="flex flex-wrap gap-2" role="group" aria-label="Other status changes">
                    {others.map((t) => <Button key={t.to} size="sm" variant="outline" onClick={() => void actions.transition(task.id, t.to)}>{t.label}</Button>)}
                  </div>
                </div>
              )}
            </div>
            <section aria-label="Work updates" className="space-y-2 md:col-span-2">
              <SectionHead icon={<MessageSquarePlus />} title="Work updates" hint="Short progress notes, each with the time." count={updates.length} />
              <form className="flex gap-2" onSubmit={(e) => { e.preventDefault(); void addUpdate(); }}>
                <Input value={update} onChange={(e) => setUpdate(e.target.value)} placeholder="What happened?" aria-label="New work update" maxLength={5000} />
                <Button type="submit" size="sm" variant="secondary" disabled={!update.trim()}>Add</Button>
              </form>
              {updates.length > 0 && (
                <ul className="space-y-2">
                  {updates.map((u) => (
                    <li key={u.id} className="group/update flex items-start gap-2 rounded-md bg-muted/40 px-3 py-2">
                      <div className="min-w-0 flex-1">
                        <p className="whitespace-pre-line text-[13px] leading-snug">{u.body}</p>
                        <p className="mt-0.5 text-[11px] text-muted-foreground">{formatWhen(u.created_at, now)}</p>
                      </div>
                      {now - u.created_at <= DELETE_WINDOW_MS && (
                        <ConfirmAction message="Delete this update?" onConfirm={() => void actions.removeUpdate(u.id)}>
                          <button type="button" aria-label="Delete update" title="Can be deleted for 5 minutes"
                            className="grid size-6 shrink-0 place-items-center rounded text-muted-foreground outline-none hover:bg-destructive/15 hover:text-destructive focus-visible:ring-2 focus-visible:ring-ring"><X className="size-3.5" /></button>
                        </ConfirmAction>
                      )}
                    </li>
                  ))}
                </ul>
              )}
            </section>
            <div aria-label="History" className="space-y-1 md:col-span-2">
              <p className="flex items-center gap-1.5 text-[13px] font-semibold"><History aria-hidden className="size-4 text-muted-foreground" />History <span className="text-xs font-normal tabular-nums text-muted-foreground">{history.length}</span></p>
              <ol className="space-y-1">
                {[...history].reverse().map((e, i) => (
                  <li key={i} className="flex items-baseline gap-3 text-[12px]">
                    <span className="w-32 shrink-0 tabular-nums text-muted-foreground">{formatWhen(e.at, now)}</span>
                    <span>{describe(e)}</span>
                  </li>
                ))}
              </ol>
              <p className="text-[11px] text-muted-foreground">Created {formatWhen(task.created_at, now)}{task.completed_at ? ` · finished at ${clockTime(task.completed_at)}` : ""}</p>
            </div>
            <div className="flex items-center gap-2 md:col-span-2">
              <ConfirmAction
                message={finished ? "Delete this task and its history for good?" : "This task isn’t finished. Delete it anyway?"}
                onConfirm={() => void actions.removeAny(task).then(onClose)}
              >
                <Button size="sm" variant="ghost" className="text-muted-foreground hover:text-destructive"><Trash2 />Delete task</Button>
              </ConfirmAction>
            </div>
          </div>
        </details>
      </div>

      <div className="flex items-center gap-2 border-t px-4 py-3">
        {dirty && <span className="flex items-center gap-1.5 text-xs text-muted-foreground"><span aria-hidden className="size-1.5 rounded-full bg-primary" />Unsaved changes</span>}
        <div className="ml-auto flex items-center gap-2">
          <Button size="sm" variant="ghost" onClick={onClose}>Close</Button>
          <Button size="sm" onClick={() => void save()}>Save changes</Button>
        </div>
      </div>
      <NoteEditor noteId={openNote} onClose={() => setOpenNote(null)} />
    </>
  );
}
