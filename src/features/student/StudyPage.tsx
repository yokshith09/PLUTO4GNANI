import { useQuery } from "@tanstack/react-query";
import { ChevronLeft, ChevronRight, Copy, Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { ConfirmAction } from "@/components/ConfirmAction";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { PageHeader } from "@/features/shell/PageHeader";
import { ipc } from "@/ipc";
import type { WeeklyReport } from "@/ipc/types";
import { notify } from "@/lib/notify";
import { daysBetween, parseYmd, ymd } from "@/lib/dates";
import { hoursLabel } from "@/lib/focus";
import { countdownLabel } from "./ExamCountdown";
import { useCourses, useExams } from "./useStudent";

/** The seven days ending on the given day, as the first day. */
export const weekStart = (end: Date): string => { const d = new Date(end.getFullYear(), end.getMonth(), end.getDate() - 6); return ymd(d); };
export const shiftWeek = (from: string, weeks: number): string => { const d = parseYmd(from); d.setDate(d.getDate() + 7 * weeks); return ymd(d); };

/** The report as plain text you can paste anywhere. */
export function reportText(r: WeeklyReport): string {
  const rows = r.courses.map((c) => `- ${c.course?.name ?? "No course"}: ${hoursLabel(c.focus_ms)} focus, ${c.tasks_done} ${c.tasks_done === 1 ? "task" : "tasks"} done`);
  return [`Week ${r.from} to ${r.to}`, ...rows, `Total: ${hoursLabel(r.total_focus_ms)} focus, ${r.total_tasks_done} done`].join("\n");
}

function Courses() {
  const c = useCourses();
  const [name, setName] = useState("");
  const [editing, setEditing] = useState<{ id: string; name: string } | null>(null);
  const fail = (e: unknown) => notify.error((e as { message?: string })?.message ?? "That did not work.");
  return (
    <section aria-label="Courses" className="rounded-xl border bg-card p-5">
      <h2 className="mb-3 text-sm font-semibold">Courses</h2>
      <form className="flex max-w-md gap-2" onSubmit={(e) => { e.preventDefault(); if (name.trim()) c.create.mutate(name, { onSuccess: () => setName(""), onError: fail }); }}>
        <Input value={name} maxLength={60} onChange={(e) => setName(e.target.value)} placeholder="Add a course, e.g. Physics" aria-label="New course" />
        <Button type="submit" variant="secondary" size="icon" aria-label="Add course" disabled={!name.trim()}><Plus /></Button>
      </form>
      <ul className="mt-3 divide-y">
        {c.courses.map((x) => (
          <li key={x.id} className="flex h-10 items-center gap-2">
            {editing?.id === x.id ? (
              <form className="flex flex-1 gap-2" onSubmit={(e) => { e.preventDefault(); c.rename.mutate({ id: x.id, name: editing.name }, { onSuccess: () => setEditing(null), onError: fail }); }}>
                <Input autoFocus value={editing.name} maxLength={60} aria-label={`Rename ${x.name}`} className="h-8" onChange={(e) => setEditing({ id: x.id, name: e.target.value })} onKeyDown={(e) => e.key === "Escape" && setEditing(null)} />
              </form>
            ) : (
              <button type="button" className="flex-1 rounded text-left text-[13px] outline-none hover:underline focus-visible:ring-2 focus-visible:ring-ring" onClick={() => setEditing({ id: x.id, name: x.name })} title="Rename">{x.name}</button>
            )}
            <ConfirmAction message={`Delete ${x.name}? Its exams go too; tasks and notes stay.`} onConfirm={() => c.remove.mutate(x.id, { onError: fail })}>
              <Button variant="ghost" size="icon" aria-label={`Delete ${x.name}`}><Trash2 /></Button>
            </ConfirmAction>
          </li>
        ))}
        {c.courses.length === 0 && <li className="py-3 text-[13px] text-muted-foreground">No courses yet. Add one, then pick it on a task or a note.</li>}
      </ul>
    </section>
  );
}

function Exams() {
  const { courses } = useCourses();
  const { exams, add, remove } = useExams();
  const [title, setTitle] = useState("");
  const [date, setDate] = useState("");
  const [course, setCourse] = useState("");
  const today = ymd(new Date());
  const name = (id: string | null) => courses.find((c) => c.id === id)?.name;
  return (
    <section aria-label="Exams" className="rounded-xl border bg-card p-5">
      <h2 className="mb-3 text-sm font-semibold">Exams</h2>
      <form className="flex flex-wrap gap-2" onSubmit={(e) => { e.preventDefault(); add.mutate({ course: course || null, title, date }, { onSuccess: () => { setTitle(""); setDate(""); } }); }}>
        <Input value={title} maxLength={100} onChange={(e) => setTitle(e.target.value)} placeholder="Exam, e.g. Mid-term" aria-label="Exam name" className="w-48" />
        <Input type="date" value={date} min={today} onChange={(e) => setDate(e.target.value)} aria-label="Exam date" className="w-40" />
        <select aria-label="Exam course" value={course} onChange={(e) => setCourse(e.target.value)} className="h-9 rounded-md border border-input bg-card px-2 text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-ring">
          <option value="">No course</option>
          {courses.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
        </select>
        <Button type="submit" variant="secondary" disabled={!title.trim() || !date}><Plus />Add exam</Button>
      </form>
      <ul className="mt-3 divide-y">
        {exams.map((e) => (
          <li key={e.id} className="flex h-10 items-center gap-3 text-[13px]">
            <span className="w-20 font-semibold tabular-nums">{countdownLabel(daysBetween(today, e.on_date))}</span>
            <span className="flex-1 truncate">{e.title}{name(e.course_id) && <span className="text-muted-foreground"> · {name(e.course_id)}</span>}</span>
            <span className="text-xs tabular-nums text-muted-foreground">{e.on_date}</span>
            <ConfirmAction message={`Delete ${e.title}?`} onConfirm={() => remove.mutate(e.id)}>
              <Button variant="ghost" size="icon" aria-label={`Delete ${e.title}`}><Trash2 /></Button>
            </ConfirmAction>
          </li>
        ))}
        {exams.length === 0 && <li className="py-3 text-[13px] text-muted-foreground">No exams coming up.</li>}
      </ul>
    </section>
  );
}

function Report() {
  const [from, setFrom] = useState(() => weekStart(new Date()));
  const report = useQuery({ queryKey: ["weekly-report", from], queryFn: () => ipc.weeklyReport(from) });
  const r = report.data;
  const copy = async () => {
    if (!r) return;
    try { await navigator.clipboard.writeText(reportText(r)); notify.success("Report copied"); } catch { notify.error("Could not copy"); }
  };
  const thisWeek = from === weekStart(new Date());
  return (
    <section aria-label="Weekly report" className="rounded-xl border bg-card p-5">
      <div className="mb-3 flex flex-wrap items-center gap-2">
        <h2 className="text-sm font-semibold">Weekly report</h2>
        <div className="ml-auto flex items-center gap-1">
          <Button variant="ghost" size="icon" aria-label="Previous week" onClick={() => setFrom(shiftWeek(from, -1))}><ChevronLeft /></Button>
          <span className="min-w-40 text-center text-xs tabular-nums text-muted-foreground">{r ? `${r.from} to ${r.to}` : from}</span>
          <Button variant="ghost" size="icon" aria-label="Next week" disabled={thisWeek} onClick={() => setFrom(shiftWeek(from, 1))}><ChevronRight /></Button>
          <Button variant="outline" size="sm" onClick={() => void copy()} disabled={!r}><Copy />Copy</Button>
        </div>
      </div>
      {r && (
        <table className="w-full text-left text-[13px]">
          <thead className="text-xs text-muted-foreground"><tr><th scope="col" className="py-1.5 font-medium">Course</th><th scope="col" className="py-1.5 font-medium">Focus time</th><th scope="col" className="py-1.5 font-medium">Tasks done</th></tr></thead>
          <tbody className="divide-y">
            {r.courses.map((c) => (
              <tr key={c.course?.id ?? "none"}><th scope="row" className="py-2 font-normal">{c.course?.name ?? "No course"}</th><td className="tabular-nums">{hoursLabel(c.focus_ms)}</td><td className="tabular-nums">{c.tasks_done}</td></tr>
            ))}
            <tr className="font-medium"><th scope="row" className="py-2 font-medium">Total</th><td className="tabular-nums">{hoursLabel(r.total_focus_ms)}</td><td className="tabular-nums">{r.total_tasks_done}</td></tr>
          </tbody>
        </table>
      )}
      <p className="mt-3 text-xs text-muted-foreground">Focus time counts finished focus sessions started on a task that has a course, minus paused time.</p>
    </section>
  );
}

/** Study: courses, exams and the weekly report (the student pack). */
export function StudyPage() {
  return (
    <div className="space-y-4">
      <PageHeader title="Study" description="Courses, exams and your week." />
      <div className="grid gap-4 lg:grid-cols-2"><Courses /><Exams /></div>
      <Report />
    </div>
  );
}
