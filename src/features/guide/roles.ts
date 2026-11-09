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
