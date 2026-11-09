import type { Page } from "@/features/shell/Sidebar";

/** One stop of the tour. `target` is a `data-tour` value on a real element; none = a centred card. */
export interface TourStep { id: StepId; target?: string; page: Page; title: string; body: string }
export type StepId = "today" | "tasks" | "notes" | "projects" | "time" | "privacy" | "pet" | "character" | "settings";

export const STEPS: Record<StepId, TourStep> = {
  today: { id: "today", target: "nav-today", page: "today", title: "Today: your day at a glance", body: "Your tasks, focus timer, time so far and links on one screen. Start your day here." },
  tasks: { id: "tasks", target: "nav-tasks", page: "tasks", title: "Tasks", body: "Type a task and press Enter. Give it a day, a priority and a next step when you want to. Deleted tasks and notes wait in the Bin first." },
  notes: { id: "notes", target: "nav-notes", page: "notes", title: "Notes", body: "Quick notes with labels, colours and pins. Open one in its own window to keep it beside your work." },
  projects: { id: "projects", target: "nav-projects", page: "projects", title: "Projects", body: "Group tasks and notes by project, so one piece of work has one home." },
  time: { id: "time", target: "nav-time", page: "time", title: "Time: where your day went", body: "Time per app and per site, by day, week or month. Nothing is recorded until you switch it on." },
  privacy: { id: "privacy", target: "time-privacy", page: "time", title: "Tracking and privacy", body: "This is where you switch app time and website time on or off, leave out apps, and delete what was recorded. Under Sites, the Privacy radar shows which visited sites are known ad or tracking companies. It only ever stores the site name, never page titles, and it all stays on this computer." },
  pet: { id: "pet", page: "today", title: "Your pet", body: "It lives on your desktop. Hover for today at a glance, click to open Pluto, right-click for shortcuts, and press + on its card to add a task in one line." },
  character: { id: "character", target: "nav-character", page: "character", title: "Character", body: "Pick another pet, dress it up, choose how it reacts, and switch reminders to stretch or drink water on or off." },
  settings: { id: "settings", target: "nav-settings", page: "settings", title: "That's the tour", body: "You can replay it any time under Settings → Guide, where short answers to the common questions are kept too." },
};

/** The stops, in order. Privacy is in every tour. */
export const TOUR_STEPS: TourStep[] = (["today", "tasks", "notes", "time", "privacy", "pet", "character", "settings"] as StepId[]).map((id) => STEPS[id]);

/** `pending`: show it next time Today is open. */
export const TOUR_PREF = "ui.tour";
export type TourState = "pending" | "done" | null;
export const parseTour = (raw: unknown): TourState => (raw === "pending" || raw === "done" ? raw : null);
