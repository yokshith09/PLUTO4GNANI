import type { Page } from "@/features/shell/Sidebar";
import type { Role } from "./roles";

export interface Question {
  id: string;
  q: string;
  options: [string, string, string];
  answer: 0 | 1 | 2;
  why: string;
  show?: { page: Page; label: string };
}

/** Plain facts about Pluto, checked against how it really behaves. */
export const BANK: Record<string, Question> = {
  "quick-add": { id: "quick-add", q: "You are in another app. How do you add a task without opening Pluto?", options: ["Press + on the pet's hover card", "Email it to yourself", "You have to open Pluto first"], answer: 0, why: "Hover the pet, press +, type one line and press Enter: it is added for today. Right-click the pet for more shortcuts.", show: { page: "tasks", label: "See Tasks" } },
  focus: { id: "focus", q: "What happens when a focus session ends?", options: ["Nothing, you must check the clock", "The pet cheers and offers you a short break", "Pluto closes"], answer: 1, why: "Start a focus session on Today or from a task. The timer shows beside the pet, and when it ends the pet offers a break you can take with one click.", show: { page: "today", label: "See Focus" } },
  "privacy-default": { id: "privacy-default", q: "The first time you open Pluto, what does it track about your computer use?", options: ["Every app and website", "Only the apps you use", "Nothing: tracking is off until you turn it on"], answer: 2, why: "App time and website time are both off until you switch them on, for every kind of user. You can turn them off again, leave out apps, and delete what was recorded.", show: { page: "time", label: "Open Time" } },
  "privacy-where": { id: "privacy-where", q: "Where does the time Pluto records go?", options: ["To Pluto's servers", "To your cloud account", "Only into a file on this computer"], answer: 2, why: "Pluto has no account and no server. Notes, tasks and time stay in one database on this computer, and Settings → Data exports or erases all of it.", show: { page: "settings", label: "Open Settings" } },
  "privacy-titles": { id: "privacy-titles", q: "When website time is on, what does Pluto keep about a site you visit?", options: ["The full address and page title", "Only the site's name, like example.com, and how long", "A screenshot of the page"], answer: 1, why: "Only the site name and the time. Window titles, page titles and addresses are never stored.", show: { page: "time", label: "Open Time" } },
  "privacy-radar": { id: "privacy-radar", q: "What does the Privacy radar on the Time page show?", options: ["Who is using your webcam", "Which sites you visited are known ad or tracking companies", "Which passwords have leaked"], answer: 1, why: "Switch on website time, then open Time and choose Sites. Pluto only reads the site name, so a site it does not flag is 'not flagged', never 'clean'.", show: { page: "time", label: "Open Time" } },
  delete: { id: "delete", q: "You delete a task by mistake. Where is it?", options: ["Gone for good", "In the Bin, until it is emptied or ages out", "In your email"], answer: 1, why: "Deleted tasks and notes wait in the Bin, and every delete asks first. Open the Bin to restore one.", show: { page: "bin", label: "Open the Bin" } },
  projects: { id: "projects", q: "What are Projects for?", options: ["Compiling code", "Grouping tasks and notes by piece of work", "Sharing with a team"], answer: 1, why: "Give tasks and notes a project name and they gather on one page. It is all local: nothing is shared.", show: { page: "projects", label: "Open Projects" } },
  pet: { id: "pet", q: "What does one click on the pet do?", options: ["Opens Pluto", "Hides the pet", "Starts a timer"], answer: 0, why: "Click opens Pluto, double-click hides the pet, hover shows today, right-click opens shortcuts. Drag it anywhere. It also hides itself while another app is full screen.", show: { page: "character", label: "Open Character" } },
};

/** Five questions per role, in the order they are asked. Privacy questions are in every set. */
export const QUIZ_ORDER: Record<Role, string[]> = {
  student: ["quick-add", "focus", "privacy-default", "privacy-radar", "delete"],
  developer: ["projects", "privacy-default", "privacy-titles", "privacy-radar", "quick-add"],
  student_dev: ["quick-add", "projects", "focus", "privacy-default", "privacy-radar"],
  general: ["pet", "quick-add", "privacy-default", "privacy-radar", "delete"],
};
export const questionsFor = (role: Role): Question[] => QUIZ_ORDER[role].map((id) => BANK[id]).filter((q): q is Question => !!q);

export const LEARN_PREF = "ui.learn";
export interface LearnState { answers: Record<string, number>; dismissed: boolean }
export function parseLearn(raw: unknown): LearnState {
  const r = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const answers: Record<string, number> = {};
  if (r.answers && typeof r.answers === "object") {
    for (const [k, v] of Object.entries(r.answers as Record<string, unknown>)) if (k in BANK && (v === 0 || v === 1 || v === 2)) answers[k] = v;
  }
  return { answers, dismissed: r.dismissed === true };
}
