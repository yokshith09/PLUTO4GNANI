/**
 * Who is using Pluto (FR-83, owner 2026-10-28: "what differs for student, developer and others").
 * The role decides the order of the tour and the quiz and what the tour stresses. It never hides
 * anything and never turns on a privacy feature: app and website time stay off for every role
 * until the person switches them on. Roles start with ordering and emphasis; the features that
 * would start on or off by role (student pack, developer reactions) arrive with those features.
 */
export const ROLES = ["student", "developer", "student_dev", "general"] as const;
export type Role = (typeof ROLES)[number];
export const ROLE_PREF = "ui.role";

export const ROLE_INFO: Record<Role, { label: string; blurb: string }> = {
  student: { label: "Student", blurb: "Tasks, notes and focus time for classes and revision." },
  developer: { label: "Developer", blurb: "Projects, tasks and where your time goes while you build." },
  student_dev: { label: "Student who codes", blurb: "Both: coursework and projects in one place." },
  general: { label: "Something else", blurb: "Tasks, notes and a pet; set it up your own way." },
};

export function parseRole(raw: unknown): Role {
  return typeof raw === "string" && (ROLES as readonly string[]).includes(raw) ? (raw as Role) : "general";
}

/**
 * The one thing that differs by role (owner 2026-10-28): the student pack (course label, exam
 * countdown, weekly report). On for Student and Student who codes, off for the rest; everything
 * else is on for everyone. `ui.studentPack` (true/false) overrides the role default.
 */
export const STUDENT_PACK_PREF = "ui.studentPack";
export const STUDENT_PACK_FEATURES = ["Course label on tasks and notes", "Exam countdown on Today", "Weekly report"] as const;
export const studentPackDefault = (role: Role): boolean => role === "student" || role === "student_dev";
export const parseStudentPack = (raw: unknown): boolean | null => (typeof raw === "boolean" ? raw : null);
export const studentPackOn = (role: Role, override: boolean | null | undefined): boolean => override ?? studentPackDefault(role);
