import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ipc } from "@/ipc";
import type { Course, CourseLinks, Exam } from "@/ipc/types";
import { ROLE_PREF, STUDENT_PACK_PREF, parseRole, parseStudentPack, studentPackOn } from "@/features/guide/roles";
import { usePref } from "@/features/guide/usePref";
import { ymd } from "@/lib/dates";

/** Is the student pack on? The saved switch wins; otherwise the role decides (Student and Student who codes). */
export function useStudentPack(): boolean {
  const role = usePref(ROLE_PREF, parseRole);
  const pack = usePref(STUDENT_PACK_PREF, parseStudentPack);
  if (role.loading || pack.loading) return false;
  return studentPackOn(role.value ?? "general", pack.value);
}

const KEY = ["courses"] as const;
const NONE: Course[] = [];
const NO_LINKS: CourseLinks = { tasks: [], notes: [] };

/** Read-only course data: cheap enough for every row and card. */
export function useCourseData() {
  const courses = useQuery({ queryKey: [...KEY, "list"], queryFn: () => ipc.coursesList() });
  const links = useQuery({ queryKey: [...KEY, "links"], queryFn: () => ipc.courseLinks() });
  const list = courses.data ?? NONE;
  const l = links.data ?? NO_LINKS;
  const byId = new Map(list.map((c) => [c.id, c]));
  const taskCourse = (id: string): Course | undefined => byId.get(l.tasks.find((x) => x.item_id === id)?.course_id ?? "");
  const noteCourse = (id: string): Course | undefined => byId.get(l.notes.find((x) => x.item_id === id)?.course_id ?? "");
  return { courses: list, loading: courses.isPending, taskCourse, noteCourse };
}

export function useCourses() {
  const qc = useQueryClient();
  const data = useCourseData();
  const refresh = () => Promise.all([qc.invalidateQueries({ queryKey: KEY }), qc.invalidateQueries({ queryKey: ["weekly-report"] })]);
  const mutate = <A,>(fn: (a: A) => Promise<unknown>) => useMutation({ mutationFn: fn, onSuccess: refresh });
  const create = mutate((name: string) => ipc.courseCreate(name));
  const rename = mutate((a: { id: string; name: string }) => ipc.courseRename(a.id, a.name));
  const remove = mutate((id: string) => ipc.courseDelete(id));
  const setTask = mutate((a: { id: string; course: string | null }) => ipc.taskSetCourse(a.id, a.course));
  const setNote = mutate((a: { id: string; course: string | null }) => ipc.noteSetCourse(a.id, a.course));
  return { ...data, create, rename, remove, setTask, setNote };
}

export function useExams() {
  const qc = useQueryClient();
  const today = ymd(new Date());
  const exams = useQuery({ queryKey: ["exams", today], queryFn: () => ipc.examsFrom(today) });
  const refresh = () => qc.invalidateQueries({ queryKey: ["exams"] });
  const add = useMutation({ mutationFn: (a: { course: string | null; title: string; date: string }) => ipc.examAdd(a.course, a.title, a.date), onSuccess: refresh });
  const remove = useMutation({ mutationFn: (id: string) => ipc.examDelete(id), onSuccess: refresh });
  return { exams: (exams.data ?? []) as Exam[], add, remove };
}
