// Typed IPC client (F0-07): the only place that knows command names and argument shapes.
import { call } from "./transport";
import type {
  UsageSummary, TimeRange, TrackingStatus, OpenTab, PetPack, BinNote, DailyLog, Reminder, ReminderInput, ReminderPatch, Label, LabelCount, Note, NoteInput, NotePatch, NoteSort, NoteSummary, Task, TaskEdit, TaskEvent, TaskFilters, TaskRow,
  TaskStatus, TaskView, WorkUpdate, LinkInput, TaskPlan, RepeatDay, FocusSession, FocusStart, RetentionRules, RetentionCounts, Meeting, MeetingFilters, MeetingInput, MeetingSummary,
  DiaryHit, DiaryStatus,
} from "./types";
import type { Settings, SettingsKey } from "./settings";

export const ipc = {
  settingsGetAll: () => call<Settings>("settings_get_all"),
  settingSet: <K extends SettingsKey>(key: K, value: Settings[K]) =>
    call<unknown>("setting_set", { key, value }),
  prefsGet: <T = unknown>(key: string) => call<T | null>("prefs_get", { key }),
  prefsSet: <T>(key: string, value: T) => call<unknown>("prefs_set", { key, value }),

  notesList: (archived: boolean, labelId: string | null, sort: NoteSort = "last_edited") =>
    call<NoteSummary[]>("notes_list", { archived, labelId, sort }),
  noteGet: (id: string) => call<Note>("note_get", { id }),
  /** Open a note in a window of its own (owner 2026-10-25). */
  noteWindowOpen: (id: string) => call<null>("note_window_open", { id }),
  noteCreate: (input: NoteInput = {}) => call<Note>("note_create", { input }),
  noteUpdate: (id: string, patch: NotePatch) => call<Note>("note_update", { id, patch }),
  noteSetPinned: (id: string, pinned: boolean) => call<Note>("note_set_pinned", { id, pinned }),
  noteSetArchived: (id: string, archived: boolean) =>
    call<Note>("note_set_archived", { id, archived }),
  noteDelete: (id: string) => call<Note>("note_delete", { id }),
  noteRestore: (id: string) => call<Note>("note_restore", { id }),
  notesSearch: (query: string, archived: boolean) => call<NoteSummary[]>("notes_search", { query, archived }),
  binList: () => call<BinNote[]>("bin_list"),
  notePurge: (id: string) => call<unknown>("note_purge", { id }),
  binEmpty: () => call<number>("bin_empty"),

  remindersList: (includeDone: boolean) => call<Reminder[]>("reminders_list", { includeDone }),
  reminderCreate: (input: ReminderInput) => call<Reminder>("reminder_create", { input }),
  reminderUpdate: (id: string, patch: ReminderPatch) => call<Reminder>("reminder_update", { id, patch }),
  reminderSetDone: (id: string, done: boolean) => call<Reminder>("reminder_set_done", { id, done }),
  reminderStar: (id: string, starred: boolean) => call<Reminder>("reminder_star", { id, starred }),
  reminderDelete: (id: string) => call<unknown>("reminder_delete", { id }),
  noteDiscardIfEmpty: (id: string) => call<boolean>("note_discard_if_empty", { id }),

  diaryStatus: () => call<DiaryStatus>("diary_status"),
  diarySetup: (password: string) => call<string>("diary_setup", { password }),
  diaryRecover: (recoveryKey: string, password: string) => call<unknown>("diary_recover", { recoveryKey, password }),
  diaryUnlock: (password: string) => call<unknown>("diary_unlock", { password }),
  diaryLock: () => call<unknown>("diary_lock"),
  diaryDays: () => call<string[]>("diary_days"),
  diaryGet: (day: string) => call<string>("diary_get", { day }),
  diarySearch: (query: string) => call<DiaryHit[]>("diary_search", { query }),
  diarySave: (day: string, text: string) => call<unknown>("diary_save", { day, text }),
  diaryErase: () => call<unknown>("diary_erase"),
  labelsList: () => call<LabelCount[]>("labels_list"),
  labelCreate: (name: string) => call<Label>("label_create", { name }),
  labelRename: (id: string, name: string) => call<Label>("label_rename", { id, name }),
  labelDelete: (id: string) => call<unknown>("label_delete", { id }),

  tasksQuery: (view: TaskView, filters?: TaskFilters) => call<TaskRow[]>("tasks_query", { view, filters: filters ?? null }),
  taskQuickAdd: (title: string) => call<Task>("task_quick_add", { title }),
  dailyLogGet: (date: string) => call<DailyLog | null>("daily_log_get", { date }),
  usageSummary: (from: number, to: number) => call<UsageSummary>("usage_summary", { from, to }),
  usageAppSessions: (app: string, from: number, to: number) => call<TimeRange[]>("usage_app_sessions", { app, from, to }),
  usageDomainSessions: (domain: string, from: number, to: number) => call<TimeRange[]>("usage_domain_sessions", { domain, from, to }),
  trackingStatus: () => call<TrackingStatus>("tracking_status"),
  petOpenTabs: () => call<OpenTab[]>("pet_open_tabs"),
  petsList: () => call<PetPack[]>("pets_list"),
  petsImport: () => call<PetPack | null>("pets_import"),
  petsImportZip: () => call<PetPack | null>("pets_import_zip"),
  petsRemove: (id: string) => call<unknown>("pets_remove", { id }),
  petsSheet: (id: string) => call<string>("pets_sheet", { id }),
  petCloseTab: (browser: string, windowIndex: number, tabIndex: number) =>
    call<boolean>("pet_close_tab", { browser, windowIndex, tabIndex }),
  usageDelete: (from: number | null, to: number | null) => call<unknown>("usage_delete", { from, to }),
  openUrl: (url: string) => call<unknown>("open_url", { url }),
  taskUpdate: (id: string, edit: TaskEdit) => call<Task>("task_update", { id, edit }),
  taskGet: (id: string) => call<Task>("task_get", { id }),
  taskHistory: (id: string) => call<TaskEvent[]>("task_history", { id }),
  taskDefer: (id: string, to: string) => call<Task>("task_defer", { id, to }),
  taskDelete: (id: string) => call<unknown>("task_delete", { id }),
  /** Replace the links shown next to a task (FR-106). */
  taskSetLinks: (id: string, links: LinkInput[]) => call<Task>("task_set_links", { id, links }),
  taskStar: (id: string, starred: boolean) => call<Task>("task_star", { id, starred }),
  /** Chosen days (FR-108) or a repeat rule (FR-109). */
  taskSetPlan: (id: string, plan: TaskPlan) => call<Task>("task_set_plan", { id, plan }),
  /** Tick or untick one day (`YYYY-MM-DD`) of a repeating task. */
  taskTick: (id: string, day: string, done: boolean) => call<Task>("task_tick", { id, day, done }),
  taskRepeatDays: (id: string) => call<RepeatDay[]>("task_repeat_days", { id }),
  /** Focus sessions (FR-05). One runs at a time. */
  focusStart: (input: FocusStart) => call<FocusSession>("focus_start", { input }),
  focusTaskTotal: (taskId: string) => call<number>("focus_task_total", { taskId }),
  focusCurrent: () => call<FocusSession | null>("focus_current"),
  focusPause: (paused: boolean) => call<FocusSession>("focus_pause", { paused }),
  focusExtend: (minutes: number) => call<FocusSession>("focus_extend", { minutes }),
  focusStop: () => call<FocusSession>("focus_stop"),
  focusFinish: (id: string) => call<FocusSession>("focus_finish", { id }),
  /** Auto-delete (FR-46): what would go, and doing it. */
  retentionPreview: (rules: RetentionRules) => call<RetentionCounts>("retention_preview", { rules }),
  retentionApply: (rules: RetentionRules) => call<RetentionCounts>("retention_apply", { rules }),
  tasksBinned: () => call<Task[]>("tasks_binned"),
  taskRestore: (id: string) => call<Task>("task_restore", { id }),
  workUpdatesList: (taskId: string) => call<WorkUpdate[]>("work_updates_list", { taskId }),
  workUpdateAdd: (taskId: string, body: string) => call<WorkUpdate>("work_update_add", { taskId, body }),
  workUpdateDelete: (id: string) => call<unknown>("work_update_delete", { id }),
  /** Opens the system "save" dialog and writes the export there. `null` if cancelled. */
  dataExport: () => call<{ path: string; bytes: number } | null>("data_export"),
  /** Opens the system "open" dialog and restores that export into an empty Pluto. `null` if cancelled. */
  dataImport: () => call<Record<string, number> | null>("data_import"),
  dataDeleteAll: (confirm: string) => call<unknown>("data_delete_all", { confirm }),
  petHover: (expanded: boolean, cardWidth: number, cardHeight: number, below?: boolean) =>
    call<{ pet: "left" | "right"; card: "above" | "below" } | null>("pet_hover", { expanded, cardWidth, cardHeight, below: below ?? null }),
  /**
   * Which parts of the pet window are drawn (left, top, width, height in CSS pixels from its top-left)
   * and whether a drag is on. Clicks go through the rest of the window to the app underneath.
   */
  petSolid: (rects: number[][], holding: boolean) => call<unknown>("pet_solid", { rects, holding }),
  /** A button on a question the pet is asking was pressed. */
  petProposalAnswer: (id: number, choice: string) => call<unknown>("pet_proposal_answer", { id, choice }),
  autoMeetingAnswer: (id: number, choice: string) => call<unknown>("auto_meeting_answer", { id, choice }),
  /** The Stop button on the pet while a Google Meet call is being recorded. */
  autoMeetingStop: () => call<unknown>("auto_meeting_stop"),
  autoMeetingState: () => call<{ session_id: string; meeting_id: string; started_at: number } | null>("auto_meeting_state"),
  autoMeetingCaptureFailed: (message: string) => call<unknown>("auto_meeting_capture_failed", { message }),
  petClick: () => call<unknown>("pet_click"),
  /** Hover-card buttons: open the dashboard and start a new note or task (FR-111). */
  petCommand: (what: "new-note" | "new-task" | "focus" | "tasks") => call<unknown>("pet_command", { what }),
  /** Media keys under the pet (Windows only, Unverified). */
  petMedia: (action: "play-pause" | "next" | "previous") => call<unknown>("pet_media", { action }),
  petDoubleClick: () => call<unknown>("pet_double_click"),
  petMenu: () => call<unknown>("pet_menu"),
  meetingsList: (filters: MeetingFilters = { participant: null, from: null, to: null }) => call<MeetingSummary[]>("meetings_list", { filters }),
  meetingGet: (id: string) => call<Meeting>("meeting_get", { id }),
  meetingCreate: (input: MeetingInput) => call<Meeting>("meeting_create", { input }),
  meetingUpdate: (id: string, input: MeetingInput) => call<Meeting>("meeting_update", { id, input }),
  meetingDelete: (id: string) => call<null>("meeting_delete", { id }),
  actionItemConvert: (actionItemId: string) => call<Task>("action_item_convert", { actionItemId }),
  participantsSuggest: (prefix: string) => call<string[]>("participants_suggest", { prefix }),
  appPaths: () => call<{ data_dir: string; database: string; logs: string }>("app_paths"),
  folderOpen: (which: "data" | "logs") => call<null>("folder_open", { which }),
  settingsReset: () => call<null>("settings_reset"),
  notificationTest: () => call<null>("notification_test"),
  notificationSettingsOpen: () => call<null>("notification_settings_open"),
  taskTransition: (id: string, to: TaskStatus) => call<Task>("task_transition", { id, to }),
};
