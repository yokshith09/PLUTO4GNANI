import { useState } from "react";
import { ConfirmAction } from "@/components/ConfirmAction";
import { Bell, BellRing, CalendarCheck2, CalendarClock, Plus, Trash2, Users } from "lucide-react";
import { completed, notify } from "@/lib/notify";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { DeleteTaskButton } from "@/features/tasks/DeleteTaskButton";
import { LinkChips, PlanBadge, StarButton } from "@/features/tasks/TaskBits";
import { TaskDetail } from "@/features/tasks/TaskDetail";
import { AddReminder } from "@/features/reminders/AddReminder";
import { ReminderAlerts } from "@/features/reminders/ReminderAlerts";
import { EditReminder } from "@/features/reminders/EditReminder";
import { useReminderActions, useReminders, useTaskActions, useTodayTasks } from "@/hooks/usePluto";
import { overdueLabel, ymd } from "@/lib/dates";
import { formatWhen } from "@/lib/time";
import { cn } from "@/lib/utils";
import type { Priority, Reminder, Task } from "@/ipc";

export const PRIORITY: Record<Priority, { glyph: string; label: string; cls: string }> = {
  HIGH: { glyph: "▲", label: "High", cls: "text-primary" },
  MEDIUM: { glyph: "■", label: "Medium", cls: "text-muted-foreground" },
  LOW: { glyph: "▼", label: "Low", cls: "text-muted-foreground" },
};

const DAY = 86_400_000;
const shortDate = (ymd: string) => new Date(`${ymd}T00:00:00`).toLocaleDateString([], { month: "short", day: "numeric" });
const endOfToday = () => new Date(new Date().setHours(23, 59, 59, 999)).getTime();

/** One "Today" agenda: reminders due by tonight (including missed ones) plus the day's tasks. */
export function useAgenda() {
  const { data: tasks = [] } = useTodayTasks();
  const { data: all = [] } = useReminders(false);
  const end = endOfToday();
  const reminders = all.filter((r) => r.remind_at <= end);
  const upcoming = all.filter((r) => r.remind_at > end && r.remind_at <= end + 7 * DAY);
  const missed = reminders.filter((r) => r.remind_at < Date.now()).length;
  const overdue = tasks.filter((t) => t.overdue).length;
  return { tasks, reminders, upcoming, open: tasks.length + reminders.length, missed, overdue, next: all.find((r) => r.remind_at >= Date.now()) };
}

function SectionLabel({ icon, children, count, tone }: { icon: React.ReactNode; children: React.ReactNode; count: number; tone?: "danger" }) {
  return (
    <h3 className={cn("mb-1.5 flex items-center gap-1.5 px-2 text-xs font-medium [&_svg]:size-3.5", tone === "danger" ? "text-destructive" : "text-muted-foreground")}>
      {icon}{children}<span className="tabular-nums opacity-70">{count}</span>
    </h3>
  );
}

function ReminderRow({ r }: { r: Reminder }) {
  const { setDone, remove, star } = useReminderActions();
  const now = Date.now();
  const missed = r.remind_at < now;
  return (
    <li className="group flex min-h-10 items-center gap-3 rounded-lg px-2 hover:bg-accent/50">
      <Checkbox aria-label={`Mark “${r.title}” done`} checked={false} onCheckedChange={() => void setDone(r.id, true)} />
      <EditReminder reminder={r}>
        <button type="button" title="Change the text or time" aria-label={`Edit reminder “${r.title}”`}
          className={cn("min-w-0 flex-1 truncate rounded text-left text-[13px] outline-none hover:underline focus-visible:ring-2 focus-visible:ring-ring", r.starred && "font-medium")}>
          {r.title}
        </button>
      </EditReminder>
      <StarButton starred={r.starred} title={r.title} onToggle={() => void star(r.id, !r.starred)} />
      <span className={cn("flex items-center gap-1 text-[11px] tabular-nums", missed ? "text-destructive" : "text-muted-foreground")}>
        {missed ? <BellRing className="size-3" /> : <Bell className="size-3" />}
        {missed ? "Missed · " : ""}{formatWhen(r.remind_at, now)}
      </span>
      <ConfirmAction message="Delete this reminder?" onConfirm={() => void remove(r.id)}>
        <button
          type="button" aria-label={`Delete reminder “${r.title}”`}
          className="grid size-6 place-items-center rounded text-muted-foreground opacity-0 outline-none hover:bg-destructive/15 hover:text-destructive focus-visible:opacity-100 focus-visible:ring-2 focus-visible:ring-ring group-hover:opacity-100 data-[state=open]:opacity-100"
        >
          <Trash2 className="size-3.5" />
        </button>
      </ConfirmAction>
    </li>
  );
}

export function TodayAgenda({ showUpcoming = false }: { showUpcoming?: boolean }) {
  const { tasks, reminders, upcoming } = useAgenda();
  const { setDone, star, tick } = useTaskActions();
  const [openId, setOpenId] = useState<string | null>(null);
  const today = ymd(new Date());
  const late = tasks.filter((r) => r.overdue);
  const onTime = tasks.filter((r) => !r.overdue);
  const complete = async (task: Task) => {
    // A repeating task is ticked for today only; it comes back on its next day (FR-109).
    if (task.repeat) {
      const today = ymd(new Date());
      await tick(task.id, today, true);
      notify.success(`Ticked for today: “${task.title}”`, { action: { label: "Undo", onClick: () => void tick(task.id, today, false) } });
      return;
    }
    await setDone(task.id, true);
    completed(task.title, () => void setDone(task.id, false));
  };

  if (tasks.length === 0 && reminders.length === 0 && !(showUpcoming && upcoming.length)) {
    return (
      <div className="grid h-full place-items-center px-4 py-8 text-center">
        <div>
          <span className="mx-auto mb-2 grid size-9 place-items-center rounded-lg bg-muted text-muted-foreground"><CalendarCheck2 className="size-4" /></span>
          <p className="text-[13px] font-medium">Nothing planned yet</p>
          <p className="mt-0.5 text-xs text-muted-foreground">Add the first thing you want to finish today.</p>
        </div>
      </div>
    );
  }
  return (
    <div className="space-y-5">
      {late.length > 0 && (
        <section aria-label="Overdue">
          <SectionLabel icon={<CalendarClock />} count={late.length} tone="danger">Overdue</SectionLabel>
          <ul className="space-y-0.5">{late.map((r) => <TaskLine key={r.task.id} row={r} today={today} onOpen={setOpenId} onComplete={complete} onStar={(t) => void star(t.id, !t.starred)} />)}</ul>
        </section>
      )}
      {onTime.length > 0 && (
        <section aria-label="Tasks">
          <SectionLabel icon={<CalendarCheck2 />} count={onTime.length}>Tasks</SectionLabel>
          <ul className="space-y-0.5">{onTime.map((r) => <TaskLine key={r.task.id} row={r} today={today} onOpen={setOpenId} onComplete={complete} onStar={(t) => void star(t.id, !t.starred)} />)}</ul>
        </section>
      )}
      {reminders.length > 0 && (
        <section aria-label="Reminders">
          <SectionLabel icon={<Bell />} count={reminders.length}>Reminders</SectionLabel>
          <ul className="space-y-0.5">{reminders.map((r) => <ReminderRow key={r.id} r={r} />)}</ul>
        </section>
      )}
      {showUpcoming && upcoming.length > 0 && (
        <section aria-label="Upcoming reminders">
          <SectionLabel icon={<Bell />} count={upcoming.length}>Upcoming this week</SectionLabel>
          <ul className="space-y-0.5">{upcoming.map((r) => <ReminderRow key={r.id} r={r} />)}</ul>
        </section>
      )}
      <details className="group/alerts">
        <summary className="cursor-pointer text-xs text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring">Reminder alerts</summary>
        <div className="mt-2"><ReminderAlerts /></div>
      </details>
      <TaskDetail taskId={openId} onClose={() => setOpenId(null)} />
    </div>
  );
}

/** One task on Today: tick, title (opens the task), links, and on the right what matters most. */
function TaskLine({ row: { task, overdue }, today, onOpen, onComplete, onStar }: {
  row: { task: Task; overdue: boolean }; today: string; onOpen: (id: string) => void; onComplete: (t: Task) => void; onStar: (t: Task) => void;
}) {
  return (
    <li className={cn("group flex min-h-10 items-center gap-3 rounded-lg px-2 hover:bg-accent/50", task.starred && "bg-primary/[0.06]")}>
      <Checkbox aria-label={task.repeat ? `Tick ${task.title} for today` : `Complete ${task.title}`} checked={false} onCheckedChange={() => onComplete(task)} />
      <button type="button" title="Open the task" aria-label={`Edit ${task.title}`} onClick={() => onOpen(task.id)}
        className={cn("min-w-0 shrink truncate rounded text-left text-[13.5px] outline-none hover:underline focus-visible:ring-2 focus-visible:ring-ring", task.starred && "font-medium")}>
        {task.title}
      </button>
      <LinkChips links={task.links} max={2} />
      <span className="flex-1" />
      <PlanBadge task={task} />
      {/* FR-52: a task made from a meeting's action item names the meeting. */}
      {task.source_action_item_id && task.project && (
        <span title={`From the meeting “${task.project}”`} className="flex max-w-36 items-center gap-1 truncate rounded-md bg-violet/12 px-1.5 py-0.5 text-[11px] text-violet">
          <Users className="size-3 shrink-0" /><span className="truncate">{task.project}</span>
        </span>
      )}
      {task.status === "IN_PROGRESS" && <Badge variant="primary">In progress</Badge>}
      {overdue && task.due_date ? (
        <span className="shrink-0 text-xs font-medium text-destructive">{overdueLabel(task.due_date, today)}</span>
      ) : task.due_date ? (
        <span className="flex shrink-0 items-center gap-1 text-xs tabular-nums text-muted-foreground" title="Deadline"><CalendarClock className="size-3.5" />Due {shortDate(task.due_date)}</span>
      ) : null}
      {task.priority && (
        <span className={cn("w-16 shrink-0 text-right text-xs", PRIORITY[task.priority].cls)} title={`${PRIORITY[task.priority].label} priority`}>
          <span aria-hidden>{PRIORITY[task.priority].glyph}</span> {PRIORITY[task.priority].label}
        </span>
      )}
      <StarButton starred={task.starred} title={task.title} onToggle={() => onStar(task)} />
      <DeleteTaskButton task={task} />
    </li>
  );
}

/** Add a task (type and press Enter) or a reminder (bell). */
export function TodayComposer() {
  const { quickAdd } = useTaskActions();
  const [draft, setDraft] = useState("");
  const add = async () => {
    const title = draft.trim();
    if (!title) return;
    setDraft("");
    await quickAdd(title);
  };
  return (
    <form className="flex gap-2" onSubmit={(e) => { e.preventDefault(); void add(); }}>
      <Input value={draft} onChange={(e) => setDraft(e.target.value)} placeholder="Add a task for today" aria-label="New task" maxLength={200} />
      <Button type="submit" variant="secondary" size="icon" aria-label="Add task" disabled={!draft.trim()}><Plus /></Button>
      <AddReminder />
    </form>
  );
}
