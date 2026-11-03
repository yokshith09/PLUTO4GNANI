import { useMemo, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { CalendarClock, ChevronRight, FolderKanban, NotebookPen, Pencil, Plus, Timer } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { PageHeader, Tile } from "@/features/shell/PageHeader";
import { NoteEditor } from "@/features/notes/NoteEditor";
import { TaskDetail } from "@/features/tasks/TaskDetail";
import { Row } from "@/features/tasks/TasksPage";
import { Badge } from "@/components/ui/badge";
import { toEdit } from "@/features/tasks/edit";
import { useAllTasks, useNotes, useTaskActions } from "@/hooks/usePluto";
import { useFocus, useFocusLength } from "@/hooks/useFocus";
import { ymd } from "@/lib/dates";
import { STATUS_LABEL } from "@/lib/taskStatus";
import { planRename, summarize } from "./summary";
import { errorMessage, ipc, type NoteSummary, type TaskRow } from "@/ipc";
import { notify } from "@/lib/notify";
import { timeAgo } from "@/lib/time";
import { cn } from "@/lib/utils";

interface Project { name: string; tasks: TaskRow[]; notes: NoteSummary[]; open: number }

/** Projects from the names used on tasks and notes; one being started has no items yet. */
export function projectsFrom(tasks: TaskRow[], notes: NoteSummary[], extra: string[] = []): Project[] {
  const names = new Set<string>(extra);
  for (const r of tasks) if (r.task.project && !r.task.source_action_item_id) names.add(r.task.project);
  for (const n of notes) if (n.project) names.add(n.project);
  return [...names].sort((a, b) => a.localeCompare(b)).map((name) => {
    const ts = tasks.filter((r) => r.task.project === name && !r.task.source_action_item_id);
    return { name, tasks: ts, notes: notes.filter((n) => n.project === name), open: ts.filter((r) => r.task.status !== "COMPLETED" && r.task.status !== "CANCELLED").length };
  });
}

/** Open first, then finished; each group in the order they were added. */
const ordered = (rows: TaskRow[]) => {
  const done = (r: TaskRow) => r.task.status === "COMPLETED" || r.task.status === "CANCELLED";
  return [...rows].sort((a, b) => Number(done(a)) - Number(done(b)) || a.task.created_at - b.task.created_at);
};

/** A thin progress bar and the overdue count under a project's name in the list. */
function ProjectBar({ tasks, today }: { tasks: TaskRow[]; today: string }) {
  const s = summarize(tasks, today);
  return (
    <span className="mt-2 flex items-center gap-2" aria-hidden>
      <span className="h-1 flex-1 overflow-hidden rounded-full bg-muted"><span className="block h-full rounded-full bg-primary" style={{ width: `${s.percent}%` }} /></span>
      {s.overdue > 0 && <span className="size-1.5 rounded-full bg-destructive" title={`${s.overdue} overdue`} />}
    </span>
  );
}

/** Projects (owner 2026-10-25): a project's tasks and notes together, numbered. */
export function ProjectsPage() {
  const qc = useQueryClient();
  const { data: tasks = [] } = useAllTasks();
  const { data: notes = [] } = useNotes(false, null, "last_edited");
  const { quickAdd, update } = useTaskActions();
  const [started, setStarted] = useState<string[]>([]);
  const projects = useMemo(() => projectsFrom(tasks, notes, started), [tasks, notes, started]);
  const [chosen, setChosen] = useState<string | null>(null);
  const current = projects.find((p) => p.name === chosen) ?? projects[0] ?? null;
  const focus = useFocus();
  const { minutes } = useFocusLength();
  const today = ymd(new Date());
  const stats = useMemo(() => (current ? summarize(current.tasks, today) : null), [current, today]);
  const next = stats?.next ?? null;
  const sorted = current ? ordered(current.tasks) : [];
  const isDone = (r: TaskRow) => r.task.status === "COMPLETED" || r.task.status === "CANCELLED";
  const openRows = sorted.filter((r) => !isDone(r));
  const doneRows = sorted.filter(isDone);
  const [newName, setNewName] = useState("");
  const [taskDraft, setTaskDraft] = useState("");
  const [openTask, setOpenTask] = useState<string | null>(null);
  const [openNote, setOpenNote] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);

  /** Changes the name on every task and note in the project (a project is only that name). */
  const rename = async (to: string) => {
    if (!current) return;
    const plan = planRename(tasks, notes, current.name, to);
    setRenaming(null);
    if (!plan.ok) return;
    const name = to.trim().slice(0, 100);
    try {
      for (const id of plan.taskIds) {
        const row = tasks.find((r) => r.task.id === id);
        if (row) await update(id, { ...toEdit(row.task), project: name });
      }
      for (const id of plan.noteIds) await ipc.noteUpdate(id, { project: name });
      await qc.invalidateQueries({ queryKey: ["notes"] });
      setStarted((s) => s.map((x) => (x === current.name ? name : x)));
      setChosen(name);
      notify.success(`Renamed to “${name}”`);
    } catch (e) {
      notify.error(errorMessage(e));
    }
  };

  const start = () => {
    const name = newName.trim().slice(0, 100);
    if (!name) return;
    setStarted((s) => (s.includes(name) ? s : [...s, name]));
    setChosen(name);
    setNewName("");
  };
  const addTask = async () => {
    const title = taskDraft.trim();
    if (!title || !current) return;
    setTaskDraft("");
    try {
      const t = await quickAdd(title);
      await update(t.id, { ...toEdit(t), project: current.name });
    } catch (e) {
      notify.error(errorMessage(e));
    }
  };
  const addNote = async () => {
    if (!current) return;
    try {
      const n = await ipc.noteCreate({ project: current.name });
      await qc.invalidateQueries({ queryKey: ["notes"] });
      setOpenNote(n.id);
    } catch (e) {
      notify.error(errorMessage(e));
    }
  };

  return (
    <div className="space-y-5">
      <PageHeader title="Projects" description="A project’s tasks and notes in one place." />
      <div className="grid gap-5 lg:grid-cols-[16rem_minmax(0,1fr)]">
        <nav aria-label="Projects" className="space-y-3">
          <ul className="space-y-0.5">
            {projects.map((p) => (
              <li key={p.name}>
                <button type="button" onClick={() => setChosen(p.name)} aria-current={current?.name === p.name ? "page" : undefined}
                  className={cn("w-full rounded-lg border px-3 py-2.5 text-left outline-none focus-visible:ring-2 focus-visible:ring-ring",
                    current?.name === p.name ? "border-primary/40 bg-accent text-foreground" : "border-transparent text-muted-foreground hover:bg-accent/60 hover:text-foreground")}>
                  <span className="flex items-center gap-2.5 text-[13.5px]">
                    <FolderKanban className="size-4 shrink-0" />
                    <span className={cn("min-w-0 flex-1 truncate", current?.name === p.name && "font-semibold")}>{p.name}</span>
                    <span className="text-xs tabular-nums" title="Open tasks">{p.open}</span>
                  </span>
                  <ProjectBar tasks={p.tasks} today={today} />
                </button>
              </li>
            ))}
          </ul>
          <form className="flex gap-2" onSubmit={(e) => { e.preventDefault(); start(); }}>
            <Input value={newName} onChange={(e) => setNewName(e.target.value)} placeholder="New project" aria-label="New project name" maxLength={100} />
            <Button type="submit" size="icon" variant="secondary" aria-label="Start project" disabled={!newName.trim()}><Plus /></Button>
          </form>
        </nav>

        {current ? (
          <section aria-label={`Project ${current.name}`} className="min-w-0 space-y-5">
            <div>
              {renaming !== null ? (
                <form onSubmit={(e) => { e.preventDefault(); void rename(renaming); }} className="flex items-center gap-2">
                  <Input autoFocus value={renaming} maxLength={100} aria-label="Project name" onChange={(e) => setRenaming(e.target.value)} onBlur={() => void rename(renaming)} onKeyDown={(e) => e.key === "Escape" && setRenaming(null)} className="h-9 max-w-xs text-lg font-semibold" />
                </form>
              ) : (
                <h2 className="group flex items-center gap-2 text-xl font-semibold tracking-tight">
                  {current.name}
                  <button type="button" aria-label={`Rename ${current.name}`} onClick={() => setRenaming(current.name)} className="grid size-7 place-items-center rounded-md text-muted-foreground outline-none hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"><Pencil className="size-3.5" /></button>
                </h2>
              )}
              <p className="mt-0.5 text-[13px] text-muted-foreground">
                {current.tasks.length - current.open} of {current.tasks.length} tasks done · {current.notes.length} {current.notes.length === 1 ? "note" : "notes"}
              </p>
              {stats && (
                <ul aria-label="Project summary" className="mt-2 flex flex-wrap gap-2 text-xs">
                  <li className="rounded-md bg-muted px-2 py-1 tabular-nums">{stats.open} open</li>
                  {stats.overdue > 0 && <li className="rounded-md bg-destructive/15 px-2 py-1 font-medium tabular-nums text-destructive">{stats.overdue} overdue</li>}
                  {stats.nextDeadline && <li className="flex items-center gap-1 rounded-md bg-muted px-2 py-1"><CalendarClock aria-hidden className="size-3" />Next deadline {new Date(`${stats.nextDeadline}T00:00:00`).toLocaleDateString([], { month: "short", day: "numeric" })}</li>}
                  <li className="rounded-md bg-muted px-2 py-1 tabular-nums">{stats.percent}% done</li>
                </ul>
              )}
            </div>
            {stats && stats.total > 0 && (
              <div role="progressbar" aria-label="Project progress" aria-valuemin={0} aria-valuemax={100} aria-valuenow={stats.percent} className="h-2 overflow-hidden rounded-full bg-muted">
                <div className="h-full rounded-full bg-primary transition-[width]" style={{ width: `${stats.percent}%` }} />
              </div>
            )}
            {next && (
              <section aria-label="Next up" className="flex flex-wrap items-center gap-3 rounded-xl border border-primary/30 bg-primary/5 px-4 py-3">
                <div className="min-w-0 flex-1">
                  <p className="text-[11px] font-semibold uppercase tracking-wide text-primary">Next up</p>
                  <p className="mt-0.5 truncate text-[15px] font-semibold">{next.task.title}</p>
                  <p className="mt-0.5 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                    <Badge variant={next.task.status === "IN_PROGRESS" ? "primary" : "default"}>{STATUS_LABEL[next.task.status]}</Badge>
                    {next.task.due_date && <span>Due {new Date(`${next.task.due_date}T00:00:00`).toLocaleDateString([], { month: "short", day: "numeric" })}</span>}
                  </p>
                </div>
                <Button size="sm" variant="outline" onClick={() => setOpenTask(next.task.id)}>Open</Button>
                {!focus.session && <Button size="sm" onClick={() => void focus.start("FOCUS", minutes, next.task.id)}><Timer />Focus {minutes} min</Button>}
              </section>
            )}
            <div className="grid gap-5 2xl:grid-cols-[minmax(0,1.4fr)_minmax(0,1fr)]">
              <Tile title="Tasks" count={current.tasks.length} footer={
                <form className="flex gap-2" onSubmit={(e) => { e.preventDefault(); void addTask(); }}>
                  <Input value={taskDraft} onChange={(e) => setTaskDraft(e.target.value)} placeholder={`Add a task to ${current.name}`} aria-label="New project task" maxLength={300} />
                  <Button type="submit" variant="secondary" size="icon" aria-label="Add project task" disabled={!taskDraft.trim()}><Plus /></Button>
                </form>
              }>
                {current.tasks.length === 0 ? (
                  <p className="px-2 py-8 text-center text-[13px] text-muted-foreground">No tasks yet. Add the first one below.</p>
                ) : (
                  <>
                    <ul>{openRows.map((r, i) => <Row key={r.task.id} row={r} onOpen={setOpenTask} n={i + 1} inProject />)}</ul>
                    {doneRows.length > 0 && (
                      <details className="group mt-3 border-t pt-3">
                        <summary className="flex cursor-pointer list-none items-center gap-1.5 px-2 text-xs font-medium text-muted-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring [&::-webkit-details-marker]:hidden">
                          <ChevronRight aria-hidden className="size-3.5 transition-transform group-open:rotate-90" />Done ({doneRows.length})
                        </summary>
                        <ul className="mt-1">{doneRows.map((r, i) => <Row key={r.task.id} row={r} onOpen={setOpenTask} n={openRows.length + i + 1} inProject />)}</ul>
                      </details>
                    )}
                  </>
                )}
              </Tile>
              <Tile title="Notes" count={current.notes.length} action={<Button size="sm" variant="outline" onClick={() => void addNote()}><Plus />Note</Button>}>
                {current.notes.length === 0 ? (
                  <p className="px-2 py-8 text-center text-[13px] text-muted-foreground">No notes yet.</p>
                ) : (
                  <ol className="space-y-2">
                    {current.notes.map((n, i) => (
                      <li key={n.id}>
                        <button type="button" onClick={() => setOpenNote(n.id)} aria-label={`Open note ${n.title || "Untitled"}`}
                          className="w-full rounded-lg border bg-muted/20 px-4 py-3 text-left outline-none hover:border-foreground/25 focus-visible:ring-2 focus-visible:ring-ring">
                          <span className="flex items-baseline gap-2">
                            <span className="text-xs tabular-nums text-muted-foreground">{i + 1}.</span>
                            <span className="min-w-0 flex-1 truncate text-[13.5px] font-semibold">{n.title || "Untitled"}</span>
                            <span className="shrink-0 text-[11px] text-muted-foreground">{timeAgo(n.edited_at)}</span>
                          </span>
                          {n.excerpt && <span className="mt-1.5 line-clamp-2 block text-[13px] leading-relaxed text-foreground/75">{n.excerpt}</span>}
                        </button>
                      </li>
                    ))}
                  </ol>
                )}
              </Tile>
            </div>
          </section>
        ) : (
          <div className="grid place-items-center rounded-xl border bg-card px-6 py-16 text-center">
            <div>
              <span className="mx-auto mb-3 grid size-10 place-items-center rounded-lg bg-muted text-muted-foreground"><NotebookPen className="size-5" /></span>
              <p className="text-sm font-medium">No projects yet</p>
              <p className="mt-1 text-[13px] text-muted-foreground">Name one on the left, or set a project on a task or note.</p>
            </div>
          </div>
        )}
      </div>
      <TaskDetail taskId={openTask} onClose={() => setOpenTask(null)} />
      <NoteEditor noteId={openNote} onClose={() => setOpenNote(null)} />
    </div>
  );
}
