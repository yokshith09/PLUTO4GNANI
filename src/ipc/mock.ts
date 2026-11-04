// In-memory stand-in for the Rust shell, used only by `vite dev` outside Tauri.
import { daysAgo, ymd } from "../lib/dates";
import { repeatDays, taskOccurs } from "../lib/repeat";
import { remainingMs } from "../lib/focus";
import type {
  AppUsage, DomainUsage, TimeRange, UsageSummary,
  AppError, BinNote, DailyLog, LogEntry, Meeting, MeetingInput, MeetingSummary, Reminder, Label, PlutoEvent, Note, NoteColor, NoteSummary, Task, TaskEvent, TaskRow, TaskStatus, WorkUpdate, FocusSession,
} from "./types";

type Args = Record<string, unknown> | undefined;

const HOUR = 3_600_000;

// Module-level so meetings made from audio keep unique ids even across mock instances (events are shared).
let audioMeetingSeq = 0;

export function createMock() {
  let seq = 100;
  const now = () => Date.now();
  const listeners = new Set<(e: PlutoEvent) => void>();
  // Events carry the same fields as the real ones where the UI reads them (the pet does).
  const emit = (type: string, fields: Record<string, unknown> = {}) => listeners.forEach((l) => l({ ...fields, type, at: now() }));
  const fail = (code: AppError["code"], message: string): never => {
    throw { code, message } satisfies AppError;
  };

  const labels: Label[] = [
    { id: "l1", name: "product" }, { id: "l2", name: "ideas" }, { id: "l3", name: "work" },
    { id: "l4", name: "home" }, { id: "l5", name: "pet" },
  ];
  const diary = { password: null as string | null, recovery: "ABCD-EFGH-JKLM-NPQR-STUV-WXYZ-2345-6789", open: false, entries: new Map<string, string>() };
  const courses: { id: string; name: string }[] = [];
  const exams: { id: string; course_id: string | null; title: string; on_date: string }[] = [];
  const taskCourse = new Map<string, string>();
  const noteCourse = new Map<string, string>();
  const L = (...ids: string[]) => labels.filter((l) => ids.includes(l.id));
  const n = (
    id: string, title: string, body: string, color: NoteColor, hoursAgo: number,
    ls: Label[], pinned = false,
  ): Note => ({
    id, title, body, color, pinned, archived: false, labels: ls, project: null,
    created_at: now() - hoursAgo * HOUR, edited_at: now() - hoursAgo * HOUR,
  });
  const notes: Note[] = [
    n("n1", "Pluto v1 scope", "Notes, tasks, meetings and daily logs.\nEverything stays on this computer — no account, no cloud.", "orange", 2, L("l1"), true),
    n("n2", "Shortcuts I want", "Ctrl+N  new note\nCtrl+1…5  jump between views\nEsc  close the editor", "blue", 5, L("l2"), true),
    n("n3", "Bear animations", "Idle bob, a wave when a note is saved, sleepy after ten idle minutes. Keep every sprite under 64 KB.", "teal", 9, L("l5"), true),
    n("n4", "Groceries", "Oats\nPeanut butter\nBananas\nCoffee beans\nHoney", "yellow", 26, L("l4")),
    n("n5", "Friday prep", "Questions for the review:\n- budget for Q4\n- who owns the roadmap\n- launch timeline", "purple", 30, L("l3")),
    n("n6", "Reading list", "Designing Data-Intensive Applications\nThe Pragmatic Programmer\nA Philosophy of Software Design", "default", 50, []),
    n("n7", "Idea: weekly review", "A note that builds itself from the week's daily logs: what shipped, what slipped, what to carry over.", "green", 70, L("l2")),
    n("n8", "Release checklist", "☐ changelog\n☐ sign installer\n☐ smoke test on Windows\n☐ smoke test on macOS", "red", 96, L("l1", "l3")),
    n("n9", "Poem for the bear", "Round as a pluto and warm as toast,\nthe quiet one who guards your most\nimportant thoughts.", "default", 120, [], false),
  ];
  const bin: BinNote[] = [
    { ...summary(n("b1", "Old grocery list", "Milk, eggs, bread", "default", 30, [])), deleted_at: now() - 26 * HOUR },
    { ...summary(n("b2", "Draft: release notes", "v0.1 — first public build", "blue", 80, L("l1"))), deleted_at: now() - 3 * 24 * HOUR },
  ];
  const reminders: Reminder[] = [
    { id: "r0", title: "Review the sync PR", remind_at: now() - 40 * 60_000, note_id: null, fired_at: now() - 40 * 60_000, done_at: null, created_at: now(), starred: false, task_id: null, lead_min: 0, sound: true },
    { id: "r1", title: "Stand-up", remind_at: now() + 1.5 * HOUR, note_id: null, fired_at: null, done_at: null, created_at: now(), starred: false, task_id: null, lead_min: 0, sound: true },
    { id: "r2", title: "Call the dentist", remind_at: now() + 20 * HOUR, note_id: null, fired_at: null, done_at: null, created_at: now(), starred: false, task_id: null, lead_min: 0, sound: true },
    { id: "r3", title: "Send invoice", remind_at: now() + 3 * 24 * HOUR, note_id: null, fired_at: null, done_at: null, created_at: now(), starred: false, task_id: null, lead_min: 0, sound: true },
  ];
  const today = ymd(new Date());
  const inDays = (n: number) => ymd(daysAgo(-n));
  const tasks: Task[] = [
    t("t1", "Design the notes screen", "HIGH", today, null, "IN_PROGRESS"),
    t("t2", "Finish the sync spec", "MEDIUM", null, "2000-01-01", "PLANNED"),
    t("t3", "Refactor the tray menu", "LOW", today, null, "PLANNED"),
    t("t4", "Reply to Sam", null, today, null, "COMPLETED"),
    t("t9", "Plan the sprint", "MEDIUM", inDays(-2), null, "COMPLETED"),
    t("t10", "Review pull requests", "LOW", inDays(-1), null, "COMPLETED"),
    t("t5", "Send research notes to Mara", "HIGH", inDays(1), inDays(1), "PLANNED"),
    t("t6", "Prepare weekly planning", "MEDIUM", inDays(2), null, "PLANNED"),
    t("t7", "Write release notes", "LOW", inDays(3), inDays(4), "PLANNED"),
    t("t8", "Clear design feedback", "LOW", inDays(4), null, "PLANNED"),
  ];
  // A sample project (owner 2026-10-25: tasks and notes per project).
  for (const id of ["t1", "t3", "t7"]) { const x = tasks.find((k) => k.id === id); if (x) x.project = "Pluto v1"; }
  for (const id of ["n1", "n8"]) { const x = notes.find((k) => k.id === id); if (x) x.project = "Pluto v1"; }
  // Meetings: stored without task status; `withStatus` reads it from the tasks, like the core.
  type StoredMeeting = Omit<Meeting, "action_items"> & { action_items: { id: string; body: string; assignee: string | null; task_id: string | null }[] };
  const meetings: StoredMeeting[] = [
    {
      id: "m1", title: "Sprint planning", starts_at: now() - 26 * HOUR, notes: "Scope for the next two weeks.\n\n- Notes polish\n- Meetings", follow_up_date: inDays(3),
      participants: ["Mara", "Sam", "Ben"],
      decisions: [{ id: "d1", body: "Ship meetings before the pet polish" }, { id: "d2", body: "Freeze scope on Wednesday" }],
      action_items: [{ id: "a1", body: "Write the meetings spec", assignee: "Mara", task_id: null }, { id: "a2", body: "Book the demo slot", assignee: "Sam", task_id: null }],
      created_at: now() - 26 * HOUR, updated_at: now() - 26 * HOUR,
    },
    {
      id: "m2", title: "Design review", starts_at: now() - 4 * 24 * HOUR, notes: "", follow_up_date: null,
      participants: ["Ben", "Ava"], decisions: [{ id: "d3", body: "Keep one accent colour" }], action_items: [],
      created_at: now() - 4 * 24 * HOUR, updated_at: now() - 4 * 24 * HOUR,
    },
  ];
  const withStatus = (m: StoredMeeting): Meeting => ({
    ...m,
    action_items: m.action_items.map((a) => {
      const task = tasks.find((x) => x.id === a.task_id);
      return { ...a, task_id: task ? task.id : null, task_status: task ? task.status : null };
    }),
  });
  const findMeeting = (id: unknown) => meetings.find((m) => m.id === id) ?? fail("NOT_FOUND", "That meeting no longer exists.");
  const meetingParts = (input: MeetingInput, keep?: StoredMeeting) => {
    const title = input.title.trim();
    if (!title) fail("VALIDATION", "Give the meeting a title.");
    const people: string[] = [];
    for (const p of input.participants.map((x) => x.trim()).filter(Boolean)) if (!people.some((q) => q.toLowerCase() === p.toLowerCase())) people.push(p);
    return {
      title, notes: input.notes, follow_up_date: input.follow_up_date, participants: people,
      decisions: input.decisions.filter((d) => d.body.trim()).map((d) => ({ id: d.id ?? `d${++seq}`, body: d.body.trim() })),
      action_items: input.action_items.filter((a) => a.body.trim()).map((a) => ({
        id: a.id ?? `a${++seq}`, body: a.body.trim(), assignee: a.assignee?.trim() || null,
        task_id: keep?.action_items.find((o) => o.id === a.id)?.task_id ?? null,
      })),
    };
  };
  const summarizeMeeting = (m: StoredMeeting): MeetingSummary => ({
    id: m.id, title: m.title, starts_at: m.starts_at, participants: m.participants, follow_up_date: m.follow_up_date,
    decisions: m.decisions.length, action_items: m.action_items.length,
    open_action_items: withStatus(m).action_items.filter((a) => a.task_status !== "COMPLETED" && a.task_status !== "CANCELLED").length,
  });
  function t(id: string, title: string, priority: Task["priority"], planned: string | null, due: string | null, status: TaskStatus): Task {
    return { id, title, description: "", status, priority, project: null, planned_date: planned, due_date: due, note_id: null, source_action_item_id: null, created_at: now(), updated_at: now(), started_at: null, completed_at: status === "COMPLETED" ? now() : null, cancelled_at: null, starred: false, repeat: null, repeat_until: null, days: [], links: [] };
  }

  const taskEvents = new Map<string, TaskEvent[]>();
  // Focus sessions (FR-05): like the core, one runs at a time.
  const focus: FocusSession[] = [];
  const running = () => focus.find((f) => f.ended_at === null) ?? null;
  const endFocus = (f: FocusSession, at: number, outcome: "DONE" | "STOPPED") => {
    if (f.paused_at !== null) { f.paused_ms += at - f.paused_at; f.paused_at = null; }
    f.ended_at = at; f.outcome = outcome; emit("FocusEnded", { session: { ...f } }); return f;
  };
  const ranOut = (f: FocusSession) => f.paused_at === null && remainingMs(f, now()) === 0;
  // Auto-delete (FR-46): binned task ids; the mock's time history has no stored sessions to age.
  const binned = new Set<string>();
  const retention = (r: { time_days: number; tasks_days: number }, apply: boolean) => {
    if (![r.time_days, r.tasks_days].every((d) => [0, 30, 90, 180, 365].includes(d))) fail("VALIDATION", "Pick Never, 30 days, 90 days, 6 months or 1 year.");
    const old = r.tasks_days ? tasks.filter((x) => x.status === "COMPLETED" && !binned.has(x.id) && (x.completed_at ?? 0) < now() - r.tasks_days * 86_400_000) : [];
    const counts = { time_sessions: 0, tasks_binned: old.length, tasks_purged: 0 };
    if (apply && old.length) { old.forEach((x) => binned.add(x.id)); emit("RetentionApplied", { counts }); }
    return counts;
  };
  // Repeating-task ticks (FR-109): "taskId day".
  const ticks = new Set<string>();
  const findTask = (id: unknown) => tasks.find((y) => y.id === id) ?? fail("NOT_FOUND", "That task no longer exists.");
  const workUpdates: WorkUpdate[] = [
    { id: "w1", task_id: "t1", body: "Sketched the layout; waiting on the colour tokens.", created_at: now() - 3 * HOUR },
  ];
  function log(id: string, kind: string, extra: Partial<TaskEvent> = {}) {
    const list = taskEvents.get(id) ?? [];
    list.push({ kind, from_status: null, to_status: null, from_date: null, to_date: null, at: now(), local_date: today, ...extra });
    taskEvents.set(id, list);
  }
  for (const x of tasks) log(x.id, "CREATED");

  function summary(x: Note): NoteSummary {
    const { body, ...rest } = x;
    return { ...rest, excerpt: body.slice(0, 200) };
  }
  const find = (id: unknown) => notes.find((x) => x.id === id) ?? fail("NOT_FOUND", "That note no longer exists.");

  const defaultSettings = (): Record<string, unknown> => ({
    "general.autostart": false, "general.theme": "dark", "general.font_size": "M",
    "pet.visible": true, "pet.size": "M", "pet.opacity": 100, "pet.always_on_top": true, "pet.typing": false, "pet.hover_card": true, "pet.hide_fullscreen": true, "pet.media": false, "pet.pack": null,
    "shortcuts.global_open": null, "shortcuts.global_new_note": null, "shortcuts.global_new_task": null, "shortcuts.global_pet": null,
    "tracking.apps": true, "tracking.exclude_apps": [], "tracking.sites": false, "tracking.safari": false,
    "advanced.log_level": "info",
  });
  const settings = defaultSettings();

  // Open tabs: a small fixed demo set, so the panel has something to show and close in the
  // mock shell. Real tabs are never stored; this is the design-preview stand-in only.
  let openTabs: { browser: string; window_index: number; tab_index: number; title: string | null; domain: string | null; active: boolean }[] = [
    { browser: "Google Chrome", window_index: 0, tab_index: 0, title: "Pluto — Today", domain: "localhost", active: true },
    { browser: "Google Chrome", window_index: 0, tab_index: 1, title: "GitHub", domain: "github.com", active: false },
    { browser: "Google Chrome", window_index: 0, tab_index: 2, title: "Stack Overflow", domain: "stackoverflow.com", active: false },
  ];
  /** Deterministic sample history so the Time screen has something to show. */
  function sampleLog(date: string, back: number): DailyLog | null {
    if (back === 0) return liveLog(date);
    const seed = (back * 7 + 3) % 9;
    const planned = 4 + (seed % 5);
    const completed = Math.max(1, planned - (seed % 4));
    const hours = [9, 10, 11, 13, 14, 15, 16, 17, 20];
    const entry = (i: number, done: boolean): LogEntry => {
      const d = new Date(date + "T00:00:00");
      d.setHours(hours[(i + seed) % hours.length] ?? 10, (i * 17) % 60);
      return {
        id: `${date}-${i}`, title: ["Write the spec", "Review pull request", "Plan the week", "Inbox zero", "Fix the tray bug", "Draft release notes", "Prep the demo", "Refactor the store"][(i + seed) % 8] ?? "Task",
        priority: (["HIGH", "MEDIUM", "LOW"] as const)[(i + seed) % 3] ?? null,
        status_at_eod: done ? "COMPLETED" : "PLANNED", ...(done ? { completed_at: d.getTime() } : {}),
      };
    };
    const done = Array.from({ length: completed }, (_, i) => entry(i, true));
    // Yesterday's leftovers are real open tasks, so "Yesterday: … still open" has somewhere to go.
    const real = (id: string): LogEntry | null => {
      const x = tasks.find((y) => y.id === id);
      return x ? { id: x.id, title: x.title, priority: x.priority, status_at_eod: "PLANNED" } : null;
    };
    const left = back === 1
      ? ["t2", "t3"].map(real).filter((e): e is LogEntry => e !== null)
      : Array.from({ length: planned - completed }, (_, i) => entry(i + 20, false));
    return {
      date, live: false, reconstructed: back === 4,
      snapshot: {
        date, planned: left, in_progress: [], completed: done, pending: [], cancelled: [], overdue: [],
        stats: { planned_count: back === 1 ? completed + left.length : planned, completed_count: completed, completion_ratio: Math.round((completed / planned) * 100) / 100, tasks_created: planned, notes_created: seed % 3, notes_edited: (seed + 1) % 4, meetings: seed % 2 },
        activity: null,
      },
    };
  }
  function liveLog(date: string): DailyLog {
    const open = tasks.filter((x) => x.status !== "COMPLETED" && x.status !== "CANCELLED");
    const done = tasks.filter((x) => x.status === "COMPLETED");
    const asEntry = (x: Task): LogEntry => ({ id: x.id, title: x.title, priority: x.priority, status_at_eod: x.status, ...(x.completed_at ? { completed_at: x.completed_at } : {}) });
    const total = open.length + done.length;
    return {
      date, live: true, reconstructed: false,
      snapshot: {
        date, planned: open.filter((x) => x.status === "PLANNED").map(asEntry), in_progress: open.filter((x) => x.status === "IN_PROGRESS").map(asEntry),
        completed: done.map(asEntry), pending: [], cancelled: [], overdue: open.filter((x) => x.due_date && x.due_date < date).map(asEntry),
        stats: { planned_count: total, completed_count: done.length, completion_ratio: total ? Math.round((done.length / total) * 100) / 100 : null, tasks_created: total, notes_created: notes.filter((x) => x.created_at > now() - 12 * HOUR).length, notes_edited: 2, meetings: 0 },
        activity: null,
      },
    };
  }


  // ---- sample time tracking ------------------------------------------------------------------
  interface Sess { app: string; category: string; is_browser: boolean; domain?: string; start: number; end: number }
  const APPS: { app: string; category: string; is_browser: boolean; spans: [number, number][] }[] = [
    { app: "VS Code", category: "Coding", is_browser: false, spans: [[9, 11.5], [13.5, 16.25]] },
    { app: "Figma", category: "Design", is_browser: false, spans: [[11.5, 12.5], [16.25, 17]] },
    { app: "Google Chrome", category: "Research", is_browser: true, spans: [[8.5, 9], [12.5, 13.5], [17, 18.25]] },
    { app: "Slack", category: "Communication", is_browser: false, spans: [[9.9, 10.1], [13.2, 13.5], [15.1, 15.3]] },
    { app: "Notion", category: "Notes", is_browser: false, spans: [[16.4, 16.9]] },
    { app: "Windows Terminal", category: "Coding", is_browser: false, spans: [[11, 11.4], [14.2, 14.6]] },
  ];
  const DOMAINS: { domain: string; category: string; share: number }[] = [
    { domain: "github.com", category: "Coding", share: 0.4 },
    { domain: "stackoverflow.com", category: "Research", share: 0.25 },
    { domain: "youtube.com", category: "Entertainment", share: 0.2 },
    { domain: "docs.google.com", category: "Notes", share: 0.15 },
  ];
  function sessionsFor(from: number, to: number): Sess[] {
    const out: Sess[] = [];
    const first = new Date(from); first.setHours(0, 0, 0, 0);
    for (let day = first.getTime(); day < to; day += 86_400_000) {
      const d = new Date(day);
      const k = (d.getDate() * 7 + d.getMonth() * 3) % 5;
      // Today's sample day is squeezed into the hours that have passed, so the demo data (and the
      // tests that read it) look the same at any time of day. Earlier days use the real hours.
      const squeeze = Math.min(1, ((now() - day) / 3_600_000) / 18.5);
      for (const a of APPS) {
        a.spans.forEach(([s, e], i) => {
          const trim = ((k + i) % 3) * 0.12;
          const start = day + s * squeeze * 3_600_000, end = Math.min(day + (e - trim) * squeeze * 3_600_000, now());
          if (end <= start) return;
          out.push({ app: a.app, category: a.category, is_browser: a.is_browser, start, end });
          if (a.is_browser) {
            let cursor = start;
            for (const dm of DOMAINS) {
              const len = (end - start) * dm.share;
              out.push({ app: a.app, category: dm.category, is_browser: true, domain: dm.domain, start: cursor, end: cursor + len });
              cursor += len;
            }
          }
        });
      }
    }
    return out.filter((x) => x.end > from && x.start < to);
  }
  const clip = (x: Sess, from: number, to: number) => Math.max(0, Math.min(x.end, to) - Math.max(x.start, from));
  function summarize(from: number, to: number): UsageSummary {
    const list = sessionsFor(from, to);
    const apps = new Map<string, AppUsage>();
    const domains = new Map<string, DomainUsage>();
    const hourly = new Array<number>(24).fill(0);
    for (const x of list) {
      const sec = clip(x, from, to) / 1000;
      if (x.domain) {
        const cur = domains.get(x.domain) ?? { domain: x.domain, browser: x.app, category: x.category, total_seconds: 0, sessions: 0, is_tracker: /(^|\.)(doubleclick\.net|google-analytics\.com|facebook\.net)$/.test(x.domain) };
        cur.total_seconds += sec; cur.sessions += 1; domains.set(x.domain, cur);
      } else {
        const cur = apps.get(x.app) ?? { app: x.app, category: x.category, is_browser: x.is_browser, total_seconds: 0, sessions: 0 };
        cur.total_seconds += sec; cur.sessions += 1; apps.set(x.app, cur);
        hourly[new Date(x.start).getHours()] = (hourly[new Date(x.start).getHours()] ?? 0) + sec;
      }
    }
    const round = <T extends { total_seconds: number }>(v: T): T => ({ ...v, total_seconds: Math.round(v.total_seconds) });
    const a = [...apps.values()].map(round).sort((p, q) => q.total_seconds - p.total_seconds);
    return { total_seconds: a.reduce((n, v) => n + v.total_seconds, 0), apps: a, domains: [...domains.values()].map(round).sort((p, q) => q.total_seconds - p.total_seconds), hourly_seconds: hourly.map(Math.round) };
  }
  function ranges(pred: (x: Sess) => boolean, from: number, to: number): TimeRange[] {
    return sessionsFor(from, to).filter(pred).map((x) => ({ started_at: Math.max(x.start, from), ended_at: Math.min(x.end, to) })).sort((p, q) => p.started_at - q.started_at);
  }
  const prefs = new Map<string, unknown>([[
    "ui.shortcuts",
    [
      { id: "s1", url: "https://github.com/yokshith09/desktop-pet", label: "Pluto repo" },
      { id: "s2", url: "https://notion.so/pluto", label: "Notion" },
      { id: "s3", url: "https://mail.google.com", label: "Gmail" },
    ],
  ]]);

  const handlers: Record<string, (a: Record<string, unknown>) => unknown> = {
    settings_get_all: () => settings,
    setting_set: (a) => { settings[String(a.key)] = a.value; emit("SettingChanged"); return null; },
    prefs_get: (a) => prefs.get(String(a.key)) ?? null,
    prefs_set: (a) => { prefs.set(String(a.key), a.value); return null; },
    usage_summary: (a) => summarize(Number(a.from), Number(a.to)),
    usage_app_sessions: (a) => ranges((x) => x.app === a.app && !x.domain, Number(a.from), Number(a.to)),
    usage_domain_sessions: (a) => ranges((x) => x.domain === a.domain, Number(a.from), Number(a.to)),
    tracking_status: () => ({ supported: true, running: settings["tracking.apps"] === true, enabled: settings["tracking.apps"] === true }),
    pets_list: () => [],
    pets_import: () => null,
    pets_import_zip: () => null,
    pets_remove: () => null,
    pets_sheet: () => fail("NOT_FOUND", "No pets in the browser preview."),
    pet_open_tabs: () => (settings["tracking.apps"] === true && settings["tracking.sites"] === true ? openTabs : []),
    pet_close_tab: (a) => {
      const before = openTabs.length;
      openTabs = openTabs.filter((t) => !(t.browser === a.browser && t.window_index === a.windowIndex && t.tab_index === a.tabIndex));
      return openTabs.length < before;
    },
    usage_delete: () => null,
    open_url: (a) => { window.open(String(a.url), "_blank", "noopener"); return null; },

    notes_list: (a) => {
      const sorted = notes
        .filter((x) => x.archived === a.archived && (!a.labelId || x.labels.some((l) => l.id === a.labelId)))
        .sort((p, q) => Number(!a.archived && q.pinned) - Number(!a.archived && p.pinned) || q.edited_at - p.edited_at);
      return sorted.map(summary);
    },
    note_get: (a) => find(a.id),
    note_window_open: (a) => { window.open(`/main.html?note=${encodeURIComponent(String(a.id))}`, `note-${String(a.id)}`, "width=720,height=640"); return null; },
    note_create: (a) => {
      const i = (a.input ?? {}) as Partial<Note> & { label_ids?: string[] };
      const x = n(`n${++seq}`, i.title ?? "", i.body ?? "", i.color ?? "default", 0, L(...(i.label_ids ?? [])));
      x.project = i.project?.trim() || null;
      notes.unshift(x);
      emit("NoteCreated");
      return x;
    },
    note_update: (a) => {
      const x = find(a.id);
      const p = a.patch as Partial<Note> & { label_ids?: string[] };
      if (p.title !== undefined) x.title = p.title;
      if (p.body !== undefined) x.body = p.body;
      if (p.color !== undefined) x.color = p.color;
      if (p.label_ids) x.labels = L(...p.label_ids);
      if (p.project !== undefined && p.project !== null) x.project = p.project.trim() || null;
      x.edited_at = now();
      emit("NoteUpdated");
      return x;
    },
    note_set_pinned: (a) => { const x = find(a.id); x.pinned = a.pinned as boolean; emit("NotePinnedChanged"); return x; },
    note_set_archived: (a) => { const x = find(a.id); x.archived = a.archived as boolean; emit("NoteArchivedChanged"); return x; },
    note_delete: (a) => {
      const x = find(a.id); notes.splice(notes.indexOf(x), 1);
      bin.unshift({ ...summary(x), deleted_at: now() }); emit("NoteDeleted"); return x;
    },
    note_restore: (a) => {
      const i = bin.findIndex((b) => b.id === a.id);
      if (i < 0) fail("NOT_FOUND", "That note is no longer in the Bin.");
      const [b] = bin.splice(i, 1);
      const x: Note = { ...(b as BinNote), body: (b as BinNote).excerpt, archived: false, project: null };
      notes.unshift(x); emit("NoteRestored"); return x;
    },
    note_discard_if_empty: (a) => {
      const x = find(a.id);
      if (x.title || x.body) return false;
      notes.splice(notes.indexOf(x), 1); emit("NoteDeleted"); return true;
    },
    notes_search: (a) => {
      const q = String(a.query).trim().toLowerCase();
      if (!q) return [];
      return notes
        .filter((x) => x.archived === a.archived && (x.title + " " + x.body).toLowerCase().includes(q))
        .sort((p, r) => r.edited_at - p.edited_at).map(summary);
    },
    bin_list: () => bin,
    note_purge: (a) => { const i = bin.findIndex((b) => b.id === a.id); if (i >= 0) bin.splice(i, 1); emit("NotePurged"); return null; },
    bin_empty: () => { const c = bin.length; bin.length = 0; emit("NotePurged"); return c; },

    reminders_list: (a) =>
      reminders.filter((r) => a.includeDone || !r.done_at).sort((p, q) => Number(!!p.done_at) - Number(!!q.done_at) || Number(q.starred) - Number(p.starred) || p.remind_at - q.remind_at),
    reminder_create: (a) => {
      const i = a.input as { title: string; remind_at: number; task_id?: string | null; lead_min?: number | null; sound?: boolean | null };
      if (!i.title.trim()) fail("VALIDATION", "A reminder needs a title.");
      const r: Reminder = { id: `r${++seq}`, title: i.title.trim(), remind_at: i.remind_at, note_id: null, fired_at: null, done_at: null, created_at: now(), starred: false, task_id: i.task_id ?? null, lead_min: i.lead_min ?? 0, sound: i.sound ?? true };
      reminders.push(r); emit("ReminderCreated", { reminder: r }); return r;
    },
    reminder_update: (a) => {
      const r = reminders.find((x) => x.id === a.id) ?? fail("NOT_FOUND", "That reminder no longer exists.");
      const patch = a.patch as { title?: string | null; remind_at?: number; lead_min?: number | null; sound?: boolean | null };
      // Like the core: a new time is a new reminder as far as firing goes.
      if (patch.remind_at !== undefined && patch.remind_at !== r.remind_at) r.fired_at = null;
      if (patch.title) r.title = patch.title;
      if (patch.remind_at !== undefined) r.remind_at = patch.remind_at;
      if (patch.lead_min != null) r.lead_min = patch.lead_min;
      if (patch.sound != null) r.sound = patch.sound;
      emit("ReminderUpdated", { reminder: r }); return r;
    },
    reminder_set_done: (a) => {
      const r = reminders.find((x) => x.id === a.id) ?? fail("NOT_FOUND", "That reminder no longer exists.");
      r.done_at = a.done ? now() : null; emit("ReminderUpdated"); return r;
    },
    reminder_star: (a) => {
      const r = reminders.find((x) => x.id === a.id) ?? fail("NOT_FOUND", "That reminder no longer exists.");
      r.starred = !!a.starred; emit("ReminderUpdated", { reminder: r }); return r;
    },
    reminder_delete: (a) => { const i = reminders.findIndex((x) => x.id === a.id); if (i >= 0) reminders.splice(i, 1); emit("ReminderDeleted"); return null; },

    diary_status: () => ({ configured: diary.password !== null, unlocked: diary.password !== null && diary.open }),
    diary_setup: (a) => {
      const p = String(a.password);
      if (p.length < 6) fail("VALIDATION", "Use at least 6 characters.");
      if (diary.password !== null) fail("CONFLICT", "The diary already has a password.");
      diary.password = p; diary.open = true; return diary.recovery;
    },
    diary_unlock: (a) => {
      if (diary.password === null) fail("NOT_FOUND", "The diary has no password yet.");
      if (diary.password !== a.password) fail("VALIDATION", "That is not the right password.");
      diary.open = true; return null;
    },
    diary_recover: (a) => {
      if (diary.password === null) fail("NOT_FOUND", "The diary has no password yet.");
      if (String(a.recoveryKey).replace(/[^A-Za-z0-9]/g, "").toUpperCase() !== diary.recovery.replace(/-/g, "")) fail("VALIDATION", "That is not the right recovery key.");
      if (String(a.password).length < 6) fail("VALIDATION", "Use at least 6 characters.");
      diary.password = String(a.password); diary.open = true; return null;
    },
    diary_lock: () => { diary.open = false; return null; },
    diary_days: () => { if (!diary.open) fail("INVALID_TRANSITION", "The diary is locked."); return [...diary.entries.keys()].sort().reverse(); },
    diary_get: (a) => { if (!diary.open) fail("INVALID_TRANSITION", "The diary is locked."); return diary.entries.get(String(a.day)) ?? ""; },
    diary_search: (a) => {
      if (!diary.open) fail("INVALID_TRANSITION", "The diary is locked.");
      const q = String(a.query).trim().toLowerCase();
      if (!q) return [];
      return [...diary.entries].filter(([, t]) => t.toLowerCase().includes(q)).sort((x, y) => y[0].localeCompare(x[0])).map(([day, t]) => ({ day, snippet: t.replace(/\n/g, " ").slice(0, 60) }));
    },
    diary_save: (a) => {
      if (!diary.open) fail("INVALID_TRANSITION", "The diary is locked.");
      const t = String(a.text);
      if (t.trim()) diary.entries.set(String(a.day), t); else diary.entries.delete(String(a.day));
      return null;
    },
    diary_erase: () => { diary.password = null; diary.open = false; diary.entries.clear(); return null; },
    courses_list: () => [...courses].sort((x, y) => x.name.localeCompare(y.name)),
    course_create: (a) => {
      const name = String(a.name).trim();
      if (!name) fail("VALIDATION", "Give the course a name.");
      return courses.find((c) => c.name.toLowerCase() === name.toLowerCase()) ?? (() => { const c = { id: `c${++seq}`, name }; courses.push(c); return c; })();
    },
    course_rename: (a) => {
      const c = courses.find((x) => x.id === a.id) ?? fail("NOT_FOUND", "That course no longer exists.");
      const name = String(a.name).trim();
      if (!name) fail("VALIDATION", "Give the course a name.");
      if (courses.some((x) => x.id !== c.id && x.name.toLowerCase() === name.toLowerCase())) fail("CONFLICT", "A course with that name already exists.");
      c.name = name; return c;
    },
    course_delete: (a) => {
      const i = courses.findIndex((x) => x.id === a.id);
      if (i < 0) fail("NOT_FOUND", "That course no longer exists.");
      courses.splice(i, 1);
      for (const m of [taskCourse, noteCourse]) for (const [k, v] of m) if (v === a.id) m.delete(k);
      for (let j = exams.length - 1; j >= 0; j--) if (exams[j]?.course_id === a.id) exams.splice(j, 1);
      return null;
    },
    exams_from: (a) => exams.filter((e) => e.on_date >= String(a.from)).sort((x, y) => x.on_date.localeCompare(y.on_date) || x.title.localeCompare(y.title)),
    exam_add: (a) => {
      const title = String(a.title).trim();
      if (!title) fail("VALIDATION", "Give the exam a name.");
      if (!/^\d{4}-\d{2}-\d{2}$/.test(String(a.onDate))) fail("VALIDATION", "Pick a date.");
      const e = { id: `e${++seq}`, course_id: (a.courseId as string | null) ?? null, title, on_date: String(a.onDate) };
      exams.push(e); return e;
    },
    exam_delete: (a) => { const i = exams.findIndex((x) => x.id === a.id); if (i >= 0) exams.splice(i, 1); return null; },
    task_set_course: (a) => { if (a.courseId) taskCourse.set(String(a.taskId), String(a.courseId)); else taskCourse.delete(String(a.taskId)); return null; },
    note_set_course: (a) => { if (a.courseId) noteCourse.set(String(a.noteId), String(a.courseId)); else noteCourse.delete(String(a.noteId)); return null; },
    course_links: () => ({
      tasks: [...taskCourse].map(([item_id, course_id]) => ({ item_id, course_id })),
      notes: [...noteCourse].map(([item_id, course_id]) => ({ item_id, course_id })),
    }),
    weekly_report: (a) => {
      const from = String(a.from);
      const to = new Date(Date.parse(`${from}T00:00:00Z`) + 6 * 86_400_000).toISOString().slice(0, 10);
      const rows = courses.map((course) => ({ course, focus_ms: 0, tasks_done: 0 }));
      return { from, to, courses: rows, total_focus_ms: 0, total_tasks_done: 0 };
    },
    labels_list: () => labels.map((label) => ({ label, count: notes.filter((x) => !x.archived && x.labels.some((l) => l.id === label.id)).length })),
    label_create: (a) => {
      const name = String(a.name).trim();
      if (!name) fail("VALIDATION", "A label needs a name.");
      const l = { id: `l${++seq}`, name }; labels.push(l); emit("LabelsChanged"); return l;
    },
    label_rename: (a) => {
      const l = labels.find((x) => x.id === a.id) ?? fail("NOT_FOUND", "That label no longer exists.");
      const name = String(a.name).trim();
      if (!name) fail("VALIDATION", "A label needs a name.");
      if (labels.some((x) => x.id !== l.id && x.name.toLowerCase() === name.toLowerCase())) fail("CONFLICT", "You already have a label with that name.");
      l.name = name; for (const n of notes) for (const nl of n.labels) if (nl.id === l.id) nl.name = name;
      emit("LabelsChanged"); return l;
    },
    label_delete: (a) => {
      const i = labels.findIndex((x) => x.id === a.id);
      if (i < 0) fail("NOT_FOUND", "That label no longer exists.");
      labels.splice(i, 1); for (const n of notes) n.labels = n.labels.filter((l) => l.id !== a.id);
      emit("LabelsChanged"); return null;
    },

    tasks_query: (a) => {
      const open = (x: Task) => x.status !== "CANCELLED" && x.status !== "COMPLETED";
      const f = (a.filters ?? {}) as { status?: TaskStatus | null; priority?: Task["priority"]; project?: string | null; from?: string | null; to?: string | null; note_id?: string | null };
      let rows: Task[];
      switch (a.view) {
        case "all": rows = [...tasks].sort((p, q) => Number(!open(p)) - Number(!open(q))); break;
        case "upcoming": rows = tasks.filter((x) => open(x) && !!x.planned_date && x.planned_date > today && x.planned_date <= inDays(7)); break;
        case "pending": rows = tasks.filter((x) => x.status === "PENDING"); break;
        case "completed": rows = tasks.filter((x) => x.status === "COMPLETED").sort((p, q) => (q.completed_at ?? 0) - (p.completed_at ?? 0)); break;
        default: rows = tasks.filter((x) => open(x) && (x.repeat
          ? taskOccurs(x, today) && !ticks.has(`${x.id} ${today}`)
          : x.status === "IN_PROGRESS" || x.planned_date === today || x.days.includes(today) || (!!x.due_date && x.due_date < today)));
      }
      if (a.view === "upcoming") rows = [...rows, ...tasks.filter((x) => open(x) && !rows.includes(x) && x.days.some((d) => d > today && d <= inDays(7)))];
      // Starred first (FR-107), keeping the view's own order otherwise; finished ones stay last in All.
      rows = rows.map((x, i) => [x, i] as const).sort(([p, i], [q, j]) => (a.view === "all" ? Number(!open(p)) - Number(!open(q)) : 0) || Number(q.starred) - Number(p.starred) || i - j).map(([x]) => x);
      rows = rows.filter((x) => !binned.has(x.id));
      if (f.priority) rows = rows.filter((x) => x.priority === f.priority);
      if (f.project) rows = rows.filter((x) => x.project === f.project);
      if (f.note_id) rows = rows.filter((x) => x.note_id === f.note_id);
      if (f.status && a.view === "all") rows = rows.filter((x) => x.status === f.status);
      const day = (x: Task) => a.view === "completed" ? new Date(x.completed_at ?? 0).toISOString().slice(0, 10) : (x.planned_date ?? "");
      if (f.from) rows = rows.filter((x) => day(x) >= (f.from ?? ""));
      if (f.to) rows = rows.filter((x) => day(x) <= (f.to ?? ""));
      return rows.map<TaskRow>((task) => ({ task, overdue: !task.repeat && !!task.due_date && task.due_date < today && open(task), done_today: !!task.repeat && ticks.has(`${task.id} ${today}`) }));
    },
    task_quick_add: (a) => { const x = t(`t${++seq}`, String(a.title), null, today, null, "PLANNED"); tasks.push(x); log(x.id, "CREATED"); emit("TaskCreated", { task: x }); return x; },
    task_get: (a) => tasks.find((y) => y.id === a.id) ?? fail("NOT_FOUND", "That task no longer exists."),
    task_history: (a) => { if (!tasks.some((y) => y.id === a.id)) fail("NOT_FOUND", "That task no longer exists."); return taskEvents.get(String(a.id)) ?? []; },
    task_defer: (a) => {
      const x = tasks.find((y) => y.id === a.id) ?? fail("NOT_FOUND", "That task no longer exists.");
      if (x.status === "COMPLETED" || x.status === "CANCELLED") fail("INVALID_TRANSITION", "A finished task can't be deferred.");
      const to = String(a.to);
      if (to <= (x.planned_date ?? today)) fail("VALIDATION", "Pick a day after the current one.");
      log(x.id, "DEFERRED", { from_date: x.planned_date, to_date: to });
      x.planned_date = to; x.updated_at = now(); emit("TaskUpdated"); return x;
    },
    task_set_links: (a) => {
      const x = findTask(a.id);
      const links = (a.links as { url: string; title: string }[]).map((l) => {
        const url = l.url.trim();
        const full = /^[a-z][a-z0-9+.-]*:/i.test(url) && !/^[^:]*\./.test(url.split(":")[0] ?? "") ? url : `https://${url}`;
        if (!/^(https?:\/\/[^\s/?#@]+|mailto:[^\s/])/i.test(full)) fail("VALIDATION", "Pluto can open web links (http or https) and email links (mailto) only.");
        return { url: full, title: l.title.trim() };
      }).filter((l, i, all) => all.findIndex((o) => o.url === l.url) === i);
      if (links.length > 20) fail("VALIDATION", "A task can have up to 20 links.");
      x.links = links.map((l) => ({ id: `k${++seq}`, ...l })); x.updated_at = now(); emit("TaskUpdated", { task: x }); return x;
    },
    task_star: (a) => { const x = findTask(a.id); x.starred = !!a.starred; emit("TaskUpdated", { task: x }); return x; },
    task_set_plan: (a) => {
      const x = findTask(a.id);
      const p = a.plan as { days?: string[]; repeat?: Task["repeat"]; repeat_until?: string | null };
      const days = [...new Set(p.days ?? [])].sort();
      const repeat = p.repeat ?? null;
      if (repeat && days.length) fail("VALIDATION", "A repeating task can't also have chosen days. Pick one or the other.");
      if (repeat?.kind === "weekly" && repeat.days.length === 0) fail("VALIDATION", "Pick at least one day of the week.");
      const start = x.planned_date ?? today;
      if (p.repeat_until && p.repeat_until < start) fail("VALIDATION", "The last day can't be before the first.");
      const planned = days[0] ?? (repeat ? start : x.planned_date);
      if (planned !== x.planned_date) log(x.id, "EDITED", { from_date: x.planned_date, to_date: planned });
      Object.assign(x, { days, repeat, repeat_until: repeat ? p.repeat_until ?? null : null, planned_date: planned, updated_at: now() });
      emit("TaskUpdated", { task: x }); return x;
    },
    task_tick: (a) => {
      const x = findTask(a.id);
      const day = String(a.day);
      if (!x.repeat) fail("VALIDATION", "Only a repeating task is ticked day by day.");
      if (day > today) fail("VALIDATION", "A day that hasn't come yet can't be ticked.");
      if (!taskOccurs(x, day)) fail("VALIDATION", "This task doesn't repeat on that day.");
      if (a.done) ticks.add(`${x.id} ${day}`); else ticks.delete(`${x.id} ${day}`);
      emit("TaskTicked", { id: x.id, day, done: !!a.done }); emit("TaskUpdated", { task: x }); return x;
    },
    task_repeat_days: (a) => {
      const x = findTask(a.id);
      if (!x.repeat || !x.planned_date) return [];
      const from = [inDays(-60), x.planned_date].sort()[1] ?? x.planned_date;
      return repeatDays(x.repeat, x.planned_date, x.repeat_until, from, today).reverse().slice(0, 14).map((day) => ({ day, ticked: ticks.has(`${x.id} ${day}`) }));
    },
    focus_start: (a) => {
      const i = a.input as { kind: "FOCUS" | "BREAK"; minutes: number; task_id?: string | null };
      if (!(i.minutes >= 1 && i.minutes <= 240)) fail("VALIDATION", "Pick 1 to 240 minutes.");
      const r = running();
      if (r) { if (ranOut(r)) endFocus(r, r.started_at + r.paused_ms + r.planned_ms, "DONE"); else fail("CONFLICT", "A focus session is already running. Stop it first."); }
      const label = i.task_id ? findTask(i.task_id).title : "";
      const f: FocusSession = { id: `f${++seq}`, kind: i.kind, task_id: i.task_id ?? null, label, planned_ms: i.minutes * 60_000, started_at: now(), paused_at: null, paused_ms: 0, ended_at: null, outcome: null, local_date: today };
      focus.push(f); emit("FocusChanged", { session: { ...f } }); return { ...f };
    },
    focus_task_total: (a) => focus.filter((f) => f.task_id === a.taskId && f.kind === "FOCUS" && f.ended_at !== null).reduce((sum, f) => sum + Math.max(0, (f.ended_at ?? 0) - f.started_at - f.paused_ms), 0),
    focus_current: () => {
      const r = running();
      if (r && ranOut(r)) { endFocus(r, r.started_at + r.paused_ms + r.planned_ms, "DONE"); return null; }
      return r ? { ...r } : null;
    },
    focus_pause: (a) => {
      const r = running() ?? fail("INVALID_TRANSITION", "No focus session is running.");
      if (a.paused && r.paused_at === null) r.paused_at = now();
      else if (!a.paused && r.paused_at !== null) { r.paused_ms += now() - r.paused_at; r.paused_at = null; }
      emit("FocusChanged", { session: { ...r } }); return { ...r };
    },
    focus_extend: (a) => {
      const r = running() ?? fail("INVALID_TRANSITION", "No focus session is running.");
      r.planned_ms = Math.min(r.planned_ms + Number(a.minutes) * 60_000, 240 * 60_000);
      emit("FocusChanged", { session: { ...r } }); return { ...r };
    },
    focus_stop: () => ({ ...endFocus(running() ?? fail("INVALID_TRANSITION", "No focus session is running."), now(), "STOPPED") }),
    focus_finish: (a) => {
      const f = focus.find((x) => x.id === a.id) ?? fail("NOT_FOUND", "That focus session no longer exists.");
      if (f.ended_at !== null) return { ...f };
      if (f.paused_at !== null || remainingMs(f, now()) > 2000) fail("INVALID_TRANSITION", "This session still has time left.");
      return { ...endFocus(f, Math.min(f.started_at + f.paused_ms + f.planned_ms, now()), "DONE") };
    },
    retention_preview: (a) => retention(a.rules as { time_days: number; tasks_days: number }, false),
    retention_apply: (a) => retention(a.rules as { time_days: number; tasks_days: number }, true),
    tasks_binned: () => tasks.filter((x) => binned.has(x.id)),
    task_restore: (a) => { if (!binned.delete(String(a.id))) fail("NOT_FOUND", "That task isn't in the Bin."); const x = findTask(a.id); emit("TaskUpdated", { task: x }); return x; },
    task_delete: (a) => {
      const i = tasks.findIndex((y) => y.id === a.id);
      if (i < 0) fail("NOT_FOUND", "That task no longer exists.");
      const x = tasks[i]; if (x && x.status !== "COMPLETED" && x.status !== "CANCELLED") fail("INVALID_TRANSITION", "Complete or cancel this task before deleting it.");
      tasks.splice(i, 1); taskEvents.delete(String(a.id)); emit("TaskDeleted", { id: a.id }); return null;
    },
    work_updates_list: (a) => { if (!tasks.some((y) => y.id === a.taskId)) fail("NOT_FOUND", "That task no longer exists."); return workUpdates.filter((u) => u.task_id === a.taskId).sort((p, q) => q.created_at - p.created_at); },
    work_update_add: (a) => {
      const body = String(a.body).trim();
      if (!body) fail("VALIDATION", "Write a short update first.");
      const u: WorkUpdate = { id: `w${++seq}`, task_id: String(a.taskId), body, created_at: now() };
      workUpdates.push(u); emit("TaskUpdated"); return u;
    },
    work_update_delete: (a) => {
      const i = workUpdates.findIndex((u) => u.id === a.id);
      const u = workUpdates[i];
      if (!u) return fail("NOT_FOUND", "That update no longer exists.");
      if (now() - u.created_at > 5 * 60_000) fail("CONFLICT", "Updates can only be deleted in the first 5 minutes.");
      workUpdates.splice(i, 1); emit("TaskUpdated"); return null;
    },
    daily_log_get: (a) => {
      const d = new Date(String(a.date) + "T00:00:00");
      const back = Math.round((new Date(today + "T00:00:00").getTime() - d.getTime()) / 86_400_000);
      return back < 0 || back > 20 ? null : sampleLog(String(a.date), back);
    },
    pet_plan: () => ({ pet: "right", card: "above" }),
    pet_card: () => null,
    pet_hover: (a) => (a.expanded ? { pet: "right", card: "above" } : null),
    pet_click: () => null,
    pet_command: () => null,
    pet_double_click: () => null,
    pet_menu: () => null,
    notification_test: () => null,
    app_paths: () => ({ data_dir: "C:\\Users\\you\\AppData\\Roaming\\app.pluto.desktop", database: "C:\\Users\\you\\AppData\\Roaming\\app.pluto.desktop\\pluto.db", logs: "C:\\Users\\you\\AppData\\Local\\app.pluto.desktop\\logs" }),
    folder_open: () => null,
    settings_reset: () => {
      const defaults = defaultSettings();
      for (const [key, value] of Object.entries(defaults)) {
        if (JSON.stringify(settings[key]) !== JSON.stringify(value)) { settings[key] = value; emit("SettingChanged", { key, value }); }
      }
      return null;
    },
    meetings_list: (a) => {
      const f = (a.filters ?? {}) as { participant?: string | null; from?: number | null; to?: number | null };
      const who = f.participant?.trim().toLowerCase();
      return meetings
        .filter((m) => (f.from == null || m.starts_at >= f.from) && (f.to == null || m.starts_at < f.to) && (!who || m.participants.some((p) => p.toLowerCase() === who)))
        .sort((x, y) => y.starts_at - x.starts_at).map(summarizeMeeting);
    },
    meeting_get: (a) => withStatus(findMeeting(a.id)),
    meeting_create: (a) => {
      const input = a.input as MeetingInput;
      const m: StoredMeeting = { id: `m${++seq}`, starts_at: input.starts_at ?? now(), created_at: now(), updated_at: now(), ...meetingParts(input) };
      meetings.push(m); emit("MeetingCreated", { meeting: withStatus(m) }); return withStatus(m);
    },
    meeting_update: (a) => {
      const m = findMeeting(a.id); const input = a.input as MeetingInput;
      Object.assign(m, meetingParts(input, m), { starts_at: input.starts_at ?? m.starts_at, updated_at: now() });
      emit("MeetingUpdated", { meeting: withStatus(m) }); return withStatus(m);
    },
    meeting_delete: (a) => { meetings.splice(meetings.indexOf(findMeeting(a.id)), 1); emit("MeetingDeleted", { id: a.id }); return null; },
    action_item_convert: (a) => {
      const m = meetings.find((x) => x.action_items.some((i) => i.id === a.actionItemId)) ?? fail("NOT_FOUND", "That action item no longer exists.");
      const item = m.action_items.find((i) => i.id === a.actionItemId) ?? fail("NOT_FOUND", "That action item no longer exists.");
      if (item.task_id && tasks.some((x) => x.id === item.task_id)) fail("CONFLICT", "This action item is already a task.");
      const x = t(`t${++seq}`, item.body, null, today, m.follow_up_date, "PLANNED");
      x.project = m.title; x.source_action_item_id = item.id;
      x.description = `${item.assignee ? `Assignee: ${item.assignee}\n` : ""}From the meeting “${m.title}”.`;
      tasks.push(x); item.task_id = x.id; log(x.id, "CREATED");
      emit("TaskCreated", { task: x }); emit("MeetingUpdated", { meeting: withStatus(m) }); return x;
    },
    participants_suggest: (a) => {
      const pre = String(a.prefix).trim().toLowerCase();
      if (!pre) return [];
      const count = new Map<string, { name: string; n: number }>();
      for (const p of meetings.flatMap((m) => m.participants)) {
        const k = p.toLowerCase(); if (!k.startsWith(pre)) continue;
        count.set(k, { name: count.get(k)?.name ?? p, n: (count.get(k)?.n ?? 0) + 1 });
      }
      return [...count.values()].sort((x, y) => y.n - x.n || x.name.localeCompare(y.name)).slice(0, 8).map((v) => v.name);
    },
    notification_settings_open: () => null,
    data_export: () => ({ path: "C:\\Users\\you\\Documents\\pluto-export.json", bytes: 48_213 }),
    data_import: () => {
      if (notes.length || tasks.length || reminders.length) fail("CONFLICT", "Import only works into an empty Pluto. Delete all data first, then import.");
      emit("DataImported"); return { notes: 0 };
    },
    data_delete_all: (a) => {
      if (a.confirm !== "DELETE") fail("VALIDATION", "Type DELETE to confirm.");
      notes.length = 0; bin.length = 0; tasks.length = 0; reminders.length = 0; labels.length = 0; workUpdates.length = 0; taskEvents.clear(); meetings.length = 0;
      // Like the core: preferences and settings go too (so the welcome screen shows again). The
      // core's defaults have app tracking off; the demo turns it on.
      prefs.clear();
      Object.assign(settings, defaultSettings(), { "tracking.apps": false });
      emit("AllDataDeleted"); return null;
    },
    task_update: (a) => {
      const x = tasks.find((y) => y.id === a.id) ?? fail("NOT_FOUND", "That task no longer exists.");
      const e = a.edit as { title: string; description?: string; project?: string | null; priority?: Task["priority"]; planned_date?: string | null; due_date?: string | null; note_id?: string | null };
      if (!e.title.trim()) fail("VALIDATION", "Give the task a title.");
      x.title = e.title.trim(); x.description = e.description ?? ""; x.project = e.project?.trim() || null; x.priority = e.priority ?? null;
      x.planned_date = e.planned_date ?? null; x.due_date = e.due_date ?? null; x.note_id = e.note_id ?? null; x.updated_at = now();
      log(x.id, "EDITED"); emit("TaskUpdated"); return x;
    },
    task_transition: (a) => {
      const x = tasks.find((y) => y.id === a.id) ?? fail("NOT_FOUND", "That task no longer exists.");
      const to = a.to as TaskStatus;
      const moves: Record<TaskStatus, TaskStatus[]> = { PLANNED: ["IN_PROGRESS", "COMPLETED", "CANCELLED"], IN_PROGRESS: ["PENDING", "COMPLETED", "CANCELLED"], PENDING: ["IN_PROGRESS", "COMPLETED", "CANCELLED"], COMPLETED: ["PLANNED"], CANCELLED: ["PLANNED"] };
      if (!moves[x.status].includes(to)) fail("INVALID_TRANSITION", "That change isn't allowed for this task.");
      log(x.id, "STATUS", { from_status: x.status, to_status: to });
      const from = x.status;
      x.status = to; x.completed_at = to === "COMPLETED" ? now() : null; if (to === "IN_PROGRESS" && !x.started_at) x.started_at = now();
      emit("TaskStatusChanged", { id: x.id, from, to }); return x;
    },
  };

  // Voice and meeting-AI commands (T-006) live in their own file and load on first use, so the
  // base mock starts exactly as fast as before.
  const VOICE_COMMAND = /^(voice_|privacy_log_|audio_push$|commitments_list$|meeting_(rec_|import$|audio_pick$|job_|transcript_|speakers_|extract|items_|recall$|recap(_text)?$))/;
  let voiceLoaded: Promise<void> | null = null;
  const loadVoice = () =>
    (voiceLoaded ??= import("./mockVoice").then(({ createVoiceMock }) => {
      Object.assign(handlers, createVoiceMock({
        now,
        applyItems: (meetingId, decisions, actions) => {
          const m = findMeeting(meetingId);
          for (const body of decisions) if (!m.decisions.some((d) => d.body === body)) m.decisions.push({ id: `d${++seq}`, body });
          for (const a of actions) {
            let item = m.action_items.find((x) => x.body === a.body);
            if (!item) { item = { id: `a${++seq}`, body: a.body, assignee: a.assignee, task_id: null }; m.action_items.push(item); }
            if (a.createTask && !item.task_id) void handlers.action_item_convert?.({ actionItemId: item.id });
          }
          m.updated_at = now(); emit("MeetingUpdated", { meeting: withStatus(m) });
          return withStatus(m);
        },
        createMeeting: (title, participants) => {
          const m: StoredMeeting = { id: `mi${++audioMeetingSeq}`, starts_at: now(), created_at: now(), updated_at: now(), ...meetingParts({ title: title.trim() || "Imported meeting", starts_at: null, notes: "", follow_up_date: null, participants, decisions: [], action_items: [] }) };
          meetings.push(m); emit("MeetingCreated", { meeting: withStatus(m) }); return m.id;
        },
      }));
    }));

  return {
    call: async (cmd: string, args: Args) => {
      if (!handlers[cmd] && VOICE_COMMAND.test(cmd)) await loadVoice();
      const h = handlers[cmd] ?? (() => fail("INTERNAL", `Unknown command ${cmd}`));
      return structuredClone(h(args ?? {}));
    },
    subscribe: (cb: (e: PlutoEvent) => void) => { listeners.add(cb); return () => void listeners.delete(cb); },
  };
}
