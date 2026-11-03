import { CalendarClock } from "lucide-react";
import { daysBetween, ymd } from "@/lib/dates";
import { useCourseData, useExams, useStudentPack } from "./useStudent";

export function countdownLabel(days: number): string {
  if (days <= 0) return "Today";
  if (days === 1) return "Tomorrow";
  return `${days} days`;
}

/** Today's strip of the next exams with days to go. Student pack only; empty until an exam is added. */
export function ExamCountdown({ onManage }: { onManage: () => void }) {
  const on = useStudentPack();
  const { exams } = useExams();
  const { courses } = useCourseData();
  if (!on || exams.length === 0) return null;
  const today = ymd(new Date());
  const name = (id: string | null) => courses.find((c) => c.id === id)?.name;
  return (
    <section aria-label="Exam countdown" className="flex flex-wrap items-center gap-x-6 gap-y-2 rounded-xl border bg-card px-5 py-3">
      <span className="flex items-center gap-2 text-[13px] font-medium"><CalendarClock aria-hidden className="size-4 text-primary" />Exams</span>
      <ul className="flex flex-1 flex-wrap gap-x-6 gap-y-1">
        {exams.slice(0, 3).map((e) => {
          const days = daysBetween(today, e.on_date);
          return (
            <li key={e.id} className="text-[13px]">
              <span className={days <= 3 ? "font-semibold text-destructive" : "font-semibold"}>{countdownLabel(days)}</span>
              <span className="text-muted-foreground"> · {e.title}{name(e.course_id) ? ` (${name(e.course_id)})` : ""}</span>
            </li>
          );
        })}
      </ul>
      <button type="button" onClick={onManage} className="rounded text-xs text-primary outline-none hover:underline focus-visible:ring-2 focus-visible:ring-ring">Manage</button>
    </section>
  );
}
