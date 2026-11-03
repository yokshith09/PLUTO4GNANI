import { useEffect } from "react";
import { useMutation, useQueries, useQuery, useQueryClient } from "@tanstack/react-query";
import { daysAgo, ymd } from "@/lib/dates";
import { notify } from "@/lib/notify";
import { errorMessage, ipc, onEvent, type NoteInput, type NotePatch, type NoteSort, type TaskEdit, type LinkInput, type TaskPlan, type TaskFilters, type TaskStatus, type TaskView, type MeetingFilters, type MeetingInput } from "@/ipc";

export const keys = {
  notes: (archived: boolean, labelId: string | null, sort: NoteSort) => ["notes", archived, labelId, sort] as const,
  note: (id: string) => ["note", id] as const,
  labels: ["labels"] as const,
  tasks: ["tasks", "today"] as const,
  search: (q: string, archived: boolean) => ["notes", "search", q, archived] as const,
  bin: ["bin"] as const,
  log: (date: string) => ["log", date] as const,
  reminders: ["reminders"] as const,
  settings: ["settings"] as const,
};

/** Bus events -> cache invalidation. A lag notice (`ResyncRequired`) refetches everything. */
export function useBusSync() {
  const qc = useQueryClient();
  useEffect(
    () =>
      onEvent((e) => {
        // Importing or erasing replaces everything: refetch it all (and the settings with it).
        if (e.type === "ResyncRequired" || e.type === "DataImported" || e.type === "AllDataDeleted" || e.type === "RetentionApplied") return void qc.invalidateQueries();
        if (e.type.startsWith("Reminder")) void qc.invalidateQueries({ queryKey: keys.reminders });
        if (e.type === "UsageUpdated") void qc.invalidateQueries({ queryKey: ["usage"] });
        if (e.type === "SettingChanged") void qc.invalidateQueries({ queryKey: ["tracking"] });
        if (e.type.startsWith("Note") || e.type === "LabelsChanged") {
          void qc.invalidateQueries({ queryKey: keys.bin });
          void qc.invalidateQueries({ queryKey: ["notes"] });
          void qc.invalidateQueries({ queryKey: ["note"] });
          void qc.invalidateQueries({ queryKey: keys.labels });
        }
        if (e.type.startsWith("Task") || e.type.startsWith("Note") || e.type === "DayRolledOver" || e.type === "DailyLogFrozen") {
          void qc.invalidateQueries({ queryKey: ["log"] });
        }
        if (e.type.startsWith("Task")) {
          void qc.invalidateQueries({ queryKey: ["tasks"] });
          void qc.invalidateQueries({ queryKey: ["task"] });
          void qc.invalidateQueries({ queryKey: ["repeat-days"] });
        }
        if (e.type.startsWith("Focus")) void qc.invalidateQueries({ queryKey: ["focus"] });
        // Meetings show their action items' task status live (R1-34), so task changes count too.
        if (e.type.startsWith("Meeting") || e.type.startsWith("Task")) {
          void qc.invalidateQueries({ queryKey: ["meetings"] });
          void qc.invalidateQueries({ queryKey: ["meeting"] });
        }
        if (e.type === "SettingChanged") void qc.invalidateQueries({ queryKey: keys.settings });
      }),
    [qc],
  );
}

export const useNotes = (archived: boolean, labelId: string | null, sort: NoteSort) =>
  useQuery({ queryKey: keys.notes(archived, labelId, sort), queryFn: () => ipc.notesList(archived, labelId, sort) });
export const useSearch = (q: string, archived: boolean) =>
  useQuery({ queryKey: keys.search(q, archived), queryFn: () => ipc.notesSearch(q, archived), enabled: q.trim().length > 0 });
export const useDailyLog = (date: string) =>
  useQuery({ queryKey: keys.log(date), queryFn: () => ipc.dailyLogGet(date) });

/** Logs for the last `n` days, oldest first (today is last). Missing days are null. */
export function useRecentLogs(n: number) {
  const days = Array.from({ length: n }, (_, i) => daysAgo(n - 1 - i));
  const results = useQueries({ queries: days.map((d) => ({ queryKey: keys.log(ymd(d)), queryFn: () => ipc.dailyLogGet(ymd(d)) })) });
  return days.map((d, i) => ({ date: d, log: results[i]?.data ?? null }));
}
export const useBin = () => useQuery({ queryKey: keys.bin, queryFn: ipc.binList });
export const useReminders = (includeDone = false) =>
  useQuery({ queryKey: [...keys.reminders, includeDone], queryFn: () => ipc.remindersList(includeDone) });
export const useLabels = () => useQuery({ queryKey: keys.labels, queryFn: ipc.labelsList });
export const useTodayTasks = () => useQuery({ queryKey: keys.tasks, queryFn: () => ipc.tasksQuery("today") });
export const useTasksQuery = (view: TaskView, filters?: TaskFilters) =>
  useQuery({ queryKey: ["tasks", "view", view, filters ?? null], queryFn: () => ipc.tasksQuery(view, filters) });
export const useTask = (id: string | null) =>
  useQuery({ queryKey: ["task", id], queryFn: () => ipc.taskGet(id ?? ""), enabled: id !== null });
export const useTaskHistory = (id: string | null) =>
  useQuery({ queryKey: ["task", "history", id], queryFn: () => ipc.taskHistory(id ?? ""), enabled: id !== null });
export const useWorkUpdates = (id: string | null) =>
  useQuery({ queryKey: ["task", "updates", id], queryFn: () => ipc.workUpdatesList(id ?? ""), enabled: id !== null });
export const useUpcomingTasks = () => useQuery({ queryKey: ["tasks", "upcoming"], queryFn: () => ipc.tasksQuery("upcoming") });
export const useTrackingStatus = () => useQuery({ queryKey: ["tracking", "status"], queryFn: ipc.trackingStatus });
// Open tabs are live, never stored — short-lived, refetched on demand rather than cached long.
export const useOpenTabs = (enabled: boolean) =>
  useQuery({ queryKey: ["tabs", "open"], queryFn: ipc.petOpenTabs, enabled, staleTime: 0 });
export const useUsage = (from: number, to: number) =>
  useQuery({ queryKey: ["usage", from, to], queryFn: () => ipc.usageSummary(from, to),
    // The stretch in progress grows between saves, so look again now and then while the window is open.
    refetchInterval: 30_000 });
export const useAllTasks = () => useQuery({ queryKey: ["tasks", "all"], queryFn: () => ipc.tasksQuery("all") });
export const useSettings = () => useQuery({ queryKey: keys.settings, queryFn: ipc.settingsGetAll });

function useAction<A extends unknown[], R>(fn: (...a: A) => Promise<R>, invalidate: readonly (readonly string[])[]) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (a: A) => fn(...a),
    onSuccess: () => invalidate.forEach((k) => void qc.invalidateQueries({ queryKey: k })),
    onError: (e) => notify.error(errorMessage(e)),
  });
}

const NOTE_KEYS = [["notes"], ["note"], ["labels"], ["bin"]] as const;

export function useNoteActions() {
  const create = useAction((input: NoteInput) => ipc.noteCreate(input), NOTE_KEYS);
  const update = useAction((id: string, patch: NotePatch) => ipc.noteUpdate(id, patch), NOTE_KEYS);
  const pin = useAction((id: string, pinned: boolean) => ipc.noteSetPinned(id, pinned), NOTE_KEYS);
  const archive = useAction((id: string, archived: boolean) => ipc.noteSetArchived(id, archived), NOTE_KEYS);
  const remove = useAction((id: string) => ipc.noteDelete(id), NOTE_KEYS);
  const restore = useAction((id: string) => ipc.noteRestore(id), NOTE_KEYS);
  const purge = useAction((id: string) => ipc.notePurge(id), NOTE_KEYS);
  const emptyBin = useAction(() => ipc.binEmpty(), NOTE_KEYS);
  const discard = useAction((id: string) => ipc.noteDiscardIfEmpty(id), NOTE_KEYS);
  const createLabel = useAction((name: string) => ipc.labelCreate(name), [["labels"]]);
  const renameLabel = useAction((id: string, name: string) => ipc.labelRename(id, name), NOTE_KEYS);
  const deleteLabel = useAction((id: string) => ipc.labelDelete(id), NOTE_KEYS);
  return {
    create: (input: NoteInput = {}) => create.mutateAsync([input]),
    update: (id: string, patch: NotePatch) => update.mutateAsync([id, patch]),
    pin: (id: string, pinned: boolean) => pin.mutateAsync([id, pinned]),
    archive: (id: string, archived: boolean) => archive.mutateAsync([id, archived]),
    /** Deletes, then offers Undo (restores the exact snapshot). */
    remove: async (id: string) => {
      const gone = await remove.mutateAsync([id]);
      notify.info("Moved to Bin", { action: { label: "Undo", onClick: () => void restore.mutateAsync([gone.id]) } });
    },
    restore: (id: string) => restore.mutateAsync([id]),
    purge: (id: string) => purge.mutateAsync([id]),
    emptyBin: () => emptyBin.mutateAsync([]),
    discardIfEmpty: (id: string) => discard.mutateAsync([id]),
    createLabel: (name: string) => createLabel.mutateAsync([name]),
    renameLabel: (id: string, name: string) => renameLabel.mutateAsync([id, name]),
    deleteLabel: (id: string) => deleteLabel.mutateAsync([id]),
  };
}

export function useTaskActions() {
  const refresh = [["tasks"], ["task"]] as const;
  const quickAdd = useAction((title: string) => ipc.taskQuickAdd(title), refresh);
  const move = useAction((id: string, to: TaskStatus) => ipc.taskTransition(id, to), refresh);
  const update = useAction((id: string, edit: TaskEdit) => ipc.taskUpdate(id, edit), refresh);
  const defer = useAction((id: string, to: string) => ipc.taskDefer(id, to), refresh);
  const remove = useAction((id: string) => ipc.taskDelete(id), refresh);
  const addUpdate = useAction((id: string, body: string) => ipc.workUpdateAdd(id, body), refresh);
  const removeUpdate = useAction((id: string) => ipc.workUpdateDelete(id), refresh);
  const star = useAction((id: string, on: boolean) => ipc.taskStar(id, on), refresh);
  const links = useAction((id: string, l: LinkInput[]) => ipc.taskSetLinks(id, l), refresh);
  const plan = useAction((id: string, p: TaskPlan) => ipc.taskSetPlan(id, p), [...refresh, ["repeat-days"]]);
  const tick = useAction((id: string, day: string, done: boolean) => ipc.taskTick(id, day, done), [...refresh, ["repeat-days"]]);
  return {
    update: (id: string, edit: TaskEdit) => update.mutateAsync([id, edit]),
    /** Saves the edit and says so (FR-41). */
    save: async (id: string, edit: TaskEdit) => {
      const t = await update.mutateAsync([id, edit]);
      notify.success("Task saved");
      return t;
    },
    quickAdd: async (title: string) => {
      const t = await quickAdd.mutateAsync([title]);
      notify.success("Task added", { description: t.title });
      return t;
    },
    transition: async (id: string, to: TaskStatus) => {
      const t = await move.mutateAsync([id, to]);
      if (to === "COMPLETED") notify.success("Task done", { description: t.title });
      return t;
    },
    setDone: (id: string, done: boolean) => move.mutateAsync([id, done ? "COMPLETED" : "PLANNED"]),
    defer: (id: string, to: string) => defer.mutateAsync([id, to]),
    remove: (id: string) => remove.mutateAsync([id]),
    /** Delete any task. An unfinished one is cancelled first (the core only deletes finished tasks, so history stays honest). */
    removeAny: async (task: { id: string; status: TaskStatus }) => {
      if (task.status !== "COMPLETED" && task.status !== "CANCELLED") await move.mutateAsync([task.id, "CANCELLED"]);
      await remove.mutateAsync([task.id]);
    },
    addUpdate: (id: string, body: string) => addUpdate.mutateAsync([id, body]),
    removeUpdate: (id: string) => removeUpdate.mutateAsync([id]),
    /** FR-107. */
    star: (id: string, on: boolean) => star.mutateAsync([id, on]),
    /** FR-106: replace the task's links. */
    setLinks: (id: string, l: LinkInput[]) => links.mutateAsync([id, l]),
    /** FR-108/109: chosen days or a repeat rule. */
    setPlan: (id: string, p: TaskPlan) => plan.mutateAsync([id, p]),
    /** FR-109: tick (or untick) one day of a repeating task. */
    tick: (id: string, day: string, done: boolean) => tick.mutateAsync([id, day, done]),
  };
}

/** The last repeat days of a repeating task, newest first (FR-109). */
export const useRepeatDays = (id: string | null, repeats: boolean) =>
  useQuery({ queryKey: ["repeat-days", id], queryFn: () => ipc.taskRepeatDays(id ?? ""), enabled: !!id && repeats });

export function useReminderActions() {
  const keysToRefresh = [["reminders"]] as const;
  const create = useAction((title: string, remindAt: number) => ipc.reminderCreate({ title, remind_at: remindAt }), keysToRefresh);
  const done = useAction((id: string, d: boolean) => ipc.reminderSetDone(id, d), keysToRefresh);
  const remove = useAction((id: string) => ipc.reminderDelete(id), keysToRefresh);
  const edit = useAction((id: string, title: string, remindAt: number) => ipc.reminderUpdate(id, { title, remind_at: remindAt }), keysToRefresh);
  const star = useAction((id: string, on: boolean) => ipc.reminderStar(id, on), keysToRefresh);
  return {
    star: (id: string, on: boolean) => star.mutateAsync([id, on]),
    create: (title: string, remindAt: number) => create.mutateAsync([title, remindAt]),
    update: (id: string, title: string, remindAt: number) => edit.mutateAsync([id, title, remindAt]),
    setDone: (id: string, d: boolean) => done.mutateAsync([id, d]),
    remove: (id: string) => remove.mutateAsync([id]),
  };
}

// ---- meetings --------------------------------------------------------------------------------

const MEETING_KEYS = [["meetings"], ["meeting"], ["tasks"], ["task"]] as const;

export const useMeetings = (filters: MeetingFilters) =>
  useQuery({ queryKey: ["meetings", filters], queryFn: () => ipc.meetingsList(filters) });
export const useMeeting = (id: string | null) =>
  useQuery({ queryKey: ["meeting", id], queryFn: () => ipc.meetingGet(id ?? ""), enabled: id !== null });
export const useParticipantSuggestions = (prefix: string) =>
  useQuery({ queryKey: ["meetings", "participants", prefix], queryFn: () => ipc.participantsSuggest(prefix), enabled: prefix.trim().length > 0 });

export function useMeetingActions() {
  const create = useAction((input: MeetingInput) => ipc.meetingCreate(input), MEETING_KEYS);
  const update = useAction((id: string, input: MeetingInput) => ipc.meetingUpdate(id, input), MEETING_KEYS);
  const remove = useAction((id: string) => ipc.meetingDelete(id), MEETING_KEYS);
  const convert = useAction((actionItemId: string) => ipc.actionItemConvert(actionItemId), MEETING_KEYS);
  return {
    create: (input: MeetingInput) => create.mutateAsync([input]),
    update: (id: string, input: MeetingInput) => update.mutateAsync([id, input]),
    remove: (id: string) => remove.mutateAsync([id]),
    convert: (actionItemId: string) => convert.mutateAsync([actionItemId]),
  };
}
