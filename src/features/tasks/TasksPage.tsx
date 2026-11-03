import { useMemo, useState } from "react";
import { CalendarClock, ChevronDown, ListChecks, Plus, Search, X } from "lucide-react";
import { completed, notify } from "@/lib/notify";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { DropdownMenu, DropdownMenuContent, DropdownMenuRadioGroup, DropdownMenuRadioItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { PRIORITY } from "@/features/overview/TodayAgenda";
import { Empty } from "@/features/notes/NotesBoard";
import { PageHeader } from "@/features/shell/PageHeader";
import { useAllTasks, useTaskActions, useTasksQuery } from "@/hooks/usePluto";
import { STATUSES, STATUS_LABEL } from "@/lib/taskStatus";
import { cn } from "@/lib/utils";
import type { Priority, TaskFilters, TaskRow, TaskStatus, TaskView } from "@/ipc";
import type { TaskFocus } from "@/features/overview/YesterdayLine";
import { DeleteTaskButton } from "./DeleteTaskButton";
import { LinkChips, PlanBadge, StarButton } from "./TaskBits";
import { ymd } from "@/lib/dates";
import { taskOccurs } from "@/lib/repeat";
import { TaskDetail } from "./TaskDetail";
import { CourseTag } from "@/features/student/CoursePicker";
import { useCourses, useStudentPack } from "@/features/student/useStudent";
import { WeekCalendar } from "@/features/calendar/WeekCalendar";
import { ShowMore, useProgressive } from "@/lib/progressive";

const VIEWS: { key: TaskView; label: string; empty: string }[] = [
  { key: "today", label: "Today", empty: "Nothing planned for today." },
  { key: "upcoming", label: "Upcoming", empty: "Nothing planned for the next 7 days." },
  { key: "pending", label: "Pending", empty: "Nothing is paused. Pause a task when you are waiting on something." },
  { key: "all", label: "All", empty: "No tasks match." },
  { key: "completed", label: "Completed", empty: "Nothing completed in this range yet." },
];

const shortDate = (ymd: string) => new Date(`${ymd}T00:00:00`).toLocaleDateString([], { month: "short", day: "numeric" });
const when = (ms: number) => new Date(ms).toLocaleDateString([], { month: "short", day: "numeric" });

/** One task in a list. `n` numbers it (Projects); `inProject` hides the project it is already under. */
export function Row({ row, onOpen, n, inProject = false }: { row: TaskRow; onOpen: (id: string) => void; n?: number; inProject?: boolean }) {
  const { task, overdue } = row;
  const actions = useTaskActions();
  // A repeating task's box is today's tick (FR-109); finishing the whole task is in its detail.
  const done = task.repeat ? row.done_today : task.status === "COMPLETED";
  const toggle = async () => {
    if (task.repeat) {
      const today = ymd(new Date());
      const repeatsToday = row.done_today || taskOccurs(task, today);
      if (!repeatsToday) return notify.info("It doesn't repeat today", { description: "Open it to change its days." });
      await actions.tick(task.id, today, !row.done_today);
      if (!row.done_today) notify.success(`Ticked for today: “${task.title}”`);
      return;
    }
    if (done) return void actions.setDone(task.id, false);
    await actions.setDone(task.id, true);
    completed(task.title, () => void actions.setDone(task.id, false));
  };
  return (
    <li className={cn("group flex h-11 items-center gap-3 rounded-md px-2 hover:bg-accent/50", task.starred && "bg-primary/[0.06]")}>
      {n !== undefined && <span className="w-6 shrink-0 text-right text-xs tabular-nums text-muted-foreground">{n}.</span>}
      <Checkbox
        aria-label={task.repeat ? (done ? `Untick ${task.title} for today` : `Tick ${task.title} for today`) : done ? `Reopen ${task.title}` : `Complete ${task.title}`}
        checked={done} disabled={task.status === "CANCELLED" || (!!task.repeat && task.status === "COMPLETED")} onCheckedChange={() => void toggle()}
      />
      <button type="button" onClick={() => onOpen(task.id)} aria-label={`Open ${task.title}`}
        className={cn("min-w-24 shrink truncate rounded text-left text-[13px] outline-none hover:underline focus-visible:ring-2 focus-visible:ring-ring", task.starred && "font-medium", ((done && !task.repeat) || task.status === "CANCELLED") && "text-muted-foreground line-through")}>
        {task.title}
      </button>
      <LinkChips links={task.links} />
      <span className="flex-1" />
      <PlanBadge task={task} />
      <CourseTag kind="task" id={task.id} />
      {task.project && !inProject && <Badge>{task.project}</Badge>}
      {task.status !== "PLANNED" && task.status !== "COMPLETED" && <Badge variant={task.status === "IN_PROGRESS" ? "primary" : "default"}>{STATUS_LABEL[task.status]}</Badge>}
      {overdue && <Badge variant="destructive">Overdue</Badge>}
      {task.due_date && !overdue && (
        <span className="flex shrink-0 items-center gap-1 whitespace-nowrap text-[11px] tabular-nums text-muted-foreground" title="Deadline"><CalendarClock className="size-3" />{shortDate(task.due_date)}</span>
      )}
      {done && task.completed_at && <span className="w-14 text-right text-[11px] tabular-nums text-muted-foreground">{when(task.completed_at)}</span>}
      {task.priority && (
        <span className={cn("w-16 text-right text-[11px]", PRIORITY[task.priority].cls)} title={`${PRIORITY[task.priority].label} priority`}>
          <span aria-hidden>{PRIORITY[task.priority].glyph}</span> {PRIORITY[task.priority].label}
        </span>
      )}
      <StarButton starred={task.starred} title={task.title} onToggle={() => void actions.star(task.id, !task.starred)} />
      <DeleteTaskButton task={task} />
    </li>
  );
}

function Pick({ label, value, onChange, options }: { label: string; value: string; onChange: (v: string) => void; options: { value: string; label: string }[] }) {
  const current = options.find((o) => o.value === value);
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="outline" size="sm" className="h-8" aria-label={`${label}: ${current?.label ?? ""}`}>
          {value === "" ? label : current?.label}<ChevronDown />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start">
        <DropdownMenuRadioGroup value={value} onValueChange={onChange}>
          <DropdownMenuRadioItem value="">Any {label.toLowerCase()}</DropdownMenuRadioItem>
          {options.map((o) => <DropdownMenuRadioItem key={o.value} value={o.value}>{o.label}</DropdownMenuRadioItem>)}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** Tasks: the five views, filters, search and quick add (R1-27). Click a task for its detail. */
export function TasksPage({ focus = null, onClearFocus = () => {} }: { focus?: TaskFocus | null; onClearFocus?: () => void }) {
  const [view, setView] = useState<TaskView>("today");
  const [priority, setPriority] = useState("");
  const [status, setStatus] = useState("");
  const [project, setProject] = useState("");
  const [course, setCourse] = useState("");
  const studentPack = useStudentPack();
  const courseInfo = useCourses();
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [query, setQuery] = useState("");
  const [draft, setDraft] = useState("");
  const [open, setOpen] = useState<string | null>(null);
  const { quickAdd } = useTaskActions();

  const filters = useMemo<TaskFilters>(() => ({
    ...(priority ? { priority: priority as Priority } : {}),
    ...(project ? { project } : {}),
    ...(status && view === "all" ? { status: status as TaskStatus } : {}),
    ...(from && (view === "all" || view === "completed") ? { from } : {}),
    ...(to && (view === "all" || view === "completed") ? { to } : {}),
  }), [priority, project, status, from, to, view]);

  const { data: rows = [], isLoading } = useTasksQuery(view, filters);
  const { data: everything = [] } = useAllTasks();
  const projects = useMemo(() => [...new Set(everything.map((r) => r.task.project).filter((p): p is string => !!p))].sort(), [everything]);

  const q = query.trim().toLowerCase();
  // A focus (e.g. "Unfinished from yesterday") replaces the view: those tasks, while still open.
  const base = focus
    ? everything.filter(({ task }) => focus.ids.includes(task.id) && task.status !== "COMPLETED" && task.status !== "CANCELLED")
    : rows;
  const inCourse = course ? base.filter(({ task }) => courseInfo.taskCourse(task.id)?.id === course) : base;
  const shown = q ? inCourse.filter(({ task }) => `${task.title} ${task.description} ${task.project ?? ""}`.toLowerCase().includes(q)) : base;
  const page = useProgressive(shown);
  const filtered = !!(priority || project || course || status || from || to);
  const meta = VIEWS.find((v) => v.key === view);

  const add = async () => {
    const title = draft.trim();
    if (!title) return;
    setDraft("");
    await quickAdd(title);
  };
  const reset = () => { setPriority(""); setStatus(""); setProject(""); setCourse(""); setFrom(""); setTo(""); };

  return (
    <div className="space-y-4">
      <PageHeader title="Tasks" description="Plan, run and finish your work." />

      <form className="flex max-w-2xl gap-2" onSubmit={(e) => { e.preventDefault(); void add(); }}>
        <Input id="new-task" value={draft} onChange={(e) => setDraft(e.target.value)} placeholder="Add a task for today" aria-label="New task" maxLength={300} />
        <Button type="submit" variant="secondary" size="icon" aria-label="Add task" disabled={!draft.trim()}><Plus /></Button>
      </form>

      {focus && (
        <div className="flex items-center gap-2 rounded-lg border border-primary/30 bg-primary/5 px-3 py-2 text-[13px]" role="status">
          <span className="font-medium">{focus.title}</span>
          <span className="text-muted-foreground">· {base.length} {base.length === 1 ? "task" : "tasks"}</span>
          <Button size="sm" variant="ghost" className="ml-auto" onClick={onClearFocus}><X />Show all views</Button>
        </div>
      )}

      <div className={cn("flex flex-wrap items-center gap-2", focus && "hidden")} role="group" aria-label="Filters">
        <ToggleGroup type="single" value={view} onValueChange={(v) => v && setView(v as TaskView)} aria-label="View">
          {VIEWS.map((v) => <ToggleGroupItem key={v.key} value={v.key}>{v.label}</ToggleGroupItem>)}
        </ToggleGroup>
        <Pick label="Priority" value={priority} onChange={setPriority} options={(["HIGH", "MEDIUM", "LOW"] as Priority[]).map((p) => ({ value: p, label: PRIORITY[p].label }))} />
        {studentPack && courseInfo.courses.length > 0 && <Pick label="Course" value={course} onChange={setCourse} options={courseInfo.courses.map((c) => ({ value: c.id, label: c.name }))} />}
        {projects.length > 0 && <Pick label="Project" value={project} onChange={setProject} options={projects.map((p) => ({ value: p, label: p }))} />}
        {view === "all" && <Pick label="Status" value={status} onChange={setStatus} options={STATUSES.map((s) => ({ value: s, label: STATUS_LABEL[s] }))} />}
        {(view === "all" || view === "completed") && (
          <>
            <Input type="date" value={from} max={to || undefined} onChange={(e) => setFrom(e.target.value)} aria-label="From date" className="w-36" />
            <Input type="date" value={to} min={from || undefined} onChange={(e) => setTo(e.target.value)} aria-label="To date" className="w-36" />
          </>
        )}
        {filtered && <Button variant="ghost" size="sm" onClick={reset}>Reset filters</Button>}
        <label className="relative ml-auto flex h-8 w-56 items-center">
          <Search className="pointer-events-none absolute left-2.5 size-4 text-muted-foreground" />
          <input
            value={query} onChange={(e) => setQuery(e.target.value)} onKeyDown={(e) => e.key === "Escape" && setQuery("")}
            placeholder="Search tasks" aria-label="Search tasks"
            className="h-8 w-full rounded-md border border-input bg-card pl-8 pr-7 text-[13px] outline-none placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring"
          />
          {query && <button type="button" aria-label="Clear search" onClick={() => setQuery("")} className="absolute right-1.5 grid size-5 place-items-center rounded text-muted-foreground hover:bg-accent"><X className="size-3.5" /></button>}
        </label>
      </div>

      <section aria-label="Task list" className="rounded-xl border bg-card p-2">
        {isLoading ? (
          <div className="space-y-2 p-2" aria-hidden>{[0, 1, 2].map((i) => <div key={i} className="h-9 animate-pulse rounded-md bg-muted" />)}</div>
        ) : shown.length === 0 ? (
          <div className="p-4"><Empty icon={<ListChecks />} title={q || filtered ? "No tasks match" : (meta?.empty ?? "No tasks")} hint={q || filtered ? "Try a different word or reset the filters." : "Add one above to get started."} /></div>
        ) : (
          <>
            <ul>{page.visible.map((r) => <Row key={r.task.id} row={r} onOpen={setOpen} />)}</ul>
            <ShowMore more={page.more} onMore={page.showMore} noun="tasks" />
          </>
        )}
      </section>
      <p className="px-1 text-xs text-muted-foreground">{shown.length} {shown.length === 1 ? "task" : "tasks"}</p>

      {/* Owner 2026-10-25: the week, large, on its own tile under the list. */}
      <div className="overflow-x-auto rounded-xl"><WeekCalendar large onOpenTask={setOpen} className="min-w-[56rem]" /></div>

      <TaskDetail taskId={open} onClose={() => setOpen(null)} />
    </div>
  );
}
