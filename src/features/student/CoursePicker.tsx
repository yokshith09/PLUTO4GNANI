import { GraduationCap } from "lucide-react";
import { useState } from "react";
import { Input } from "@/components/ui/input";
import { useCourseData, useCourses, useStudentPack } from "./useStudent";

const NEW = "__new__";

/** Choose a course for a task or a note, or make one on the spot. Shows nothing when the student pack is off. */
export function CoursePicker({ kind, id, compact = false }: { kind: "task" | "note"; id: string; compact?: boolean }) {
  const on = useStudentPack();
  const c = useCourses();
  const [adding, setAdding] = useState(false);
  const [name, setName] = useState("");
  if (!on) return null;
  const current = (kind === "task" ? c.taskCourse(id) : c.noteCourse(id))?.id ?? "";
  const set = (course: string | null) => (kind === "task" ? c.setTask : c.setNote).mutate({ id, course });
  const make = () => {
    const n = name.trim();
    if (!n) return setAdding(false);
    c.create.mutate(n, { onSuccess: (made) => { set((made as { id: string }).id); setAdding(false); setName(""); } });
  };
  const cls = "h-8 min-w-0 rounded-md border border-input bg-card px-2 text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-ring";
  return (
    <div className={compact ? "flex items-center gap-1.5" : "space-y-1.5"}>
      <label htmlFor={`course-${kind}-${id}`} className="flex items-center gap-1.5 text-xs font-medium text-muted-foreground"><GraduationCap aria-hidden className="size-3.5" />Course</label>
      {adding ? (
        <form className="flex gap-2" onSubmit={(e) => { e.preventDefault(); make(); }}>
          <Input autoFocus value={name} maxLength={60} onChange={(e) => setName(e.target.value)} onKeyDown={(e) => e.key === "Escape" && setAdding(false)} placeholder="Course name" aria-label="New course name" className="h-8" />
        </form>
      ) : (
        <select
          id={`course-${kind}-${id}`} aria-label="Course" value={current} className={cls}
          onChange={(e) => (e.target.value === NEW ? setAdding(true) : set(e.target.value || null))}
        >
          <option value="">No course</option>
          {c.courses.map((x) => <option key={x.id} value={x.id}>{x.name}</option>)}
          <option value={NEW}>New course…</option>
        </select>
      )}
    </div>
  );
}

/** The small course chip on a row or card. */
export function CourseTag({ kind, id }: { kind: "task" | "note"; id: string }) {
  const on = useStudentPack();
  const c = useCourseData();
  const course = kind === "task" ? c.taskCourse(id) : c.noteCourse(id);
  if (!on || !course) return null;
  return <span className="inline-flex items-center gap-1 rounded-md bg-primary/10 px-1.5 py-0.5 text-[11px] text-primary"><GraduationCap aria-hidden className="size-3" />{course.name}</span>;
}
