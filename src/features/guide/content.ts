/** The how-to list in Settings → Guide. Plain words, one short answer per feature. */
export interface GuideEntry { id: string; title: string; body: string; studentOnly?: boolean }

export const GUIDE: GuideEntry[] = [
  { id: "tasks", title: "Add a task", body: "Open Tasks, type it and press Enter. Open a task to give it a day, a deadline, a priority, a reminder, links or a project. Deleted tasks wait in the Bin." },
  { id: "notes", title: "Write a note", body: "Press Ctrl+N in Notes (or the shortcut from anywhere). Add labels and colours, pin it, or open it in a window of its own." },
  { id: "focus", title: "Focus on a task", body: "Start a focus timer from Today or from a task. Time you finish is added to that task, and a break is offered after. Pause or stop from the pet's card." },
  { id: "pet", title: "Your pet", body: "Hover for today at a glance, click to open Pluto, right-click for shortcuts. Pick another character or change how it reacts under Character." },
  { id: "tracking", title: "App and website time", body: "Off until you switch it on, in Time → Tracking and privacy. It stores app and site names only, never page titles, and stays on this computer. You can leave apps out and delete what was recorded." },
  { id: "privacy", title: "Privacy radar", body: "In Time → Sites, the radar marks visited sites that are known ad or tracking companies. It reads the list on this computer; nothing is sent anywhere." },
  { id: "diary", title: "Diary", body: "A private journal locked with your password. It autosaves, locks after five idle minutes, and keeps the text encrypted on this computer. Switch on \"Spot things to do\" to turn lines like \"I need to submit it by Friday\" into tasks. You get a recovery key once when you create it; keep it, because it is the way back in if you forget the password." },
  { id: "data", title: "Your data", body: "Everything is in one file on this computer. Settings → Data exports it, imports it, or erases it." },
  { id: "courses", title: "Courses and exams", body: "On the Study page add your courses and exam dates. Pick a course on a task or a note, and the exam countdown shows on Today.", studentOnly: true },
  { id: "report", title: "Weekly report", body: "On the Study page, the week's focus time and finished tasks per course. Use Copy to paste it into a message or a document.", studentOnly: true },
];

export const CREDIT = "Built by QUORBIT TECHNOLOGIES";
