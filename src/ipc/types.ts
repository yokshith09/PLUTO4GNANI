// Wire types come from Rust (ts-rs): run `pnpm gen:types` after changing a type in pluto-core.
import type { NoteColor, NoteInput as GeneratedNoteInput, NotePatch as GeneratedNotePatch, ReminderInput as GeneratedReminderInput, ReminderPatch as GeneratedReminderPatch } from "./generated";

export type {
  AppError, BinNote, DailyLog, ErrorCode, Label, LogEntry, LabelCount, Note, NoteColor, NoteSort, NoteSummary, Priority,
  ActionItem, ActionItemInput, Decision, DecisionInput, Meeting, MeetingFilters, MeetingInput, MeetingSummary,
  Reminder, Snapshot, Stats, Task, TaskEdit, TaskLink, LinkInput, Repeat, RepeatDay, TaskPlan, FocusSession, FocusStart, FocusKind, FocusOutcome, RetentionRules, RetentionCounts, TaskEvent, TaskFilters, TaskRow, TaskStatus, TaskView, WorkUpdate, AppUsage, DomainUsage, DomainSetting, TimeRange, TrackingStatus, UsageSummary, OpenTab, PetPack, PetSource, PetSpecial,
  DiaryHit, DiaryStatus, Course, CourseLinks, CourseWeek, Exam, ItemCourse, WeeklyReport,
} from "./generated";

export const NOTE_COLORS: readonly NoteColor[] = [
  "default", "red", "orange", "yellow", "green", "teal", "blue", "purple", "gray",
];

// The core fills missing fields with defaults, so callers may send only what they set.
export type NoteInput = Partial<GeneratedNoteInput>;
export type NotePatch = Partial<GeneratedNotePatch>;
export type ReminderInput = GeneratedReminderInput;
export type ReminderPatch = Partial<GeneratedReminderPatch>;

/** One bus event as forwarded by the shell; only the tag matters to the UI. */
export interface PlutoEvent { type: string; at: number; [field: string]: unknown }

export interface Shortcut { id: string; url: string; label: string }
