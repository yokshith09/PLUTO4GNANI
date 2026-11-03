//! IPC command handlers. Thin: deserialize, call `pluto-core`, return. Errors cross the boundary
//! as `AppError` (`{ code, message, field? }`).

use std::collections::BTreeMap;
use std::sync::Arc;

use pluto_core::clock::Clock;
use pluto_core::courses::{self, Course, CourseLinks, Exam, WeeklyReport};
use pluto_core::daily_log::{self, DailyLog};
use pluto_core::data;
use pluto_core::diary::{self, DiaryHit, DiaryStatus};
use pluto_core::error::{AppError, Result};
use pluto_core::events::Event;
use pluto_core::focus::{self, FocusSession, FocusStart};
use pluto_core::labels::{self, Label, LabelCount};
use pluto_core::links;
use pluto_core::meetings::{self, Meeting, MeetingFilters, MeetingInput, MeetingSummary};
use pluto_core::notes::{self, BinNote, Note, NoteInput, NotePatch, NoteSort, NoteSummary};
use pluto_core::reminders::{self, Reminder, ReminderInput, ReminderPatch};
use pluto_core::retention::{self, RetentionCounts, RetentionRules};
use pluto_core::settings::SettingsKey;
use pluto_core::settings_service::SettingsService;
use pluto_core::task_plans::{self, LinkInput, RepeatDay, TaskPlan};
use pluto_core::tasks::{
    self, Task, TaskEdit, TaskEvent, TaskFilters, TaskRow, TaskStatus, TaskView, WorkUpdate,
};
use pluto_core::usage::{DomainSetting, TimeRange, TrackingStatus, UsageService, UsageSummary};
use serde_json::Value;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::tracking::Tracking;
use crate::Core;

// ---- settings and preferences ---------------------------------------------------------------

#[tauri::command]
pub async fn settings_get_all(
    service: State<'_, Arc<SettingsService>>,
) -> Result<BTreeMap<String, Value>> {
    service.get_all().await
}

#[tauri::command]
pub async fn setting_set(
    key: String,
    value: Value,
    service: State<'_, Arc<SettingsService>>,
) -> Result<()> {
    service.set(&key, value).await
}

#[tauri::command]
pub async fn prefs_get(
    key: String,
    service: State<'_, Arc<SettingsService>>,
) -> Result<Option<Value>> {
    service.get_pref(&key).await
}

#[tauri::command]
pub async fn prefs_set(
    key: String,
    value: Value,
    service: State<'_, Arc<SettingsService>>,
) -> Result<()> {
    service.set_pref(&key, value).await
}

// ---- notes ----------------------------------------------------------------------------------

#[tauri::command]
pub async fn notes_list(
    archived: bool,
    label_id: Option<String>,
    sort: Option<NoteSort>,
    core: State<'_, Core>,
) -> Result<Vec<NoteSummary>> {
    notes::list(
        &core.db,
        archived,
        label_id.as_deref(),
        sort.unwrap_or_default(),
    )
    .await
}

#[tauri::command]
pub async fn note_get(id: String, core: State<'_, Core>) -> Result<Note> {
    notes::get(&core.db, &id).await
}

#[tauri::command]
pub async fn note_create(input: NoteInput, core: State<'_, Core>) -> Result<Note> {
    notes::create(&core.db, &*core.clock, input).await
}

#[tauri::command]
pub async fn note_update(id: String, patch: NotePatch, core: State<'_, Core>) -> Result<Note> {
    notes::update(&core.db, &*core.clock, &id, patch).await
}

#[tauri::command]
pub async fn note_set_pinned(id: String, pinned: bool, core: State<'_, Core>) -> Result<Note> {
    notes::set_pinned(&core.db, &*core.clock, &id, pinned).await
}

#[tauri::command]
pub async fn note_set_archived(id: String, archived: bool, core: State<'_, Core>) -> Result<Note> {
    notes::set_archived(&core.db, &*core.clock, &id, archived).await
}

/// Moves the note to the Bin and returns it, so the UI can offer Undo via `note_restore`.
#[tauri::command]
pub async fn note_delete(id: String, core: State<'_, Core>) -> Result<Note> {
    notes::delete(&core.db, &*core.clock, &id).await
}

/// Takes a note out of the Bin.
#[tauri::command]
pub async fn note_restore(id: String, core: State<'_, Core>) -> Result<Note> {
    notes::restore_from_bin(&core.db, &*core.clock, &id).await
}

#[tauri::command]
pub async fn notes_search(
    query: String,
    archived: bool,
    core: State<'_, Core>,
) -> Result<Vec<NoteSummary>> {
    notes::search(&core.db, &query, archived).await
}

#[tauri::command]
pub async fn bin_list(core: State<'_, Core>) -> Result<Vec<BinNote>> {
    notes::list_bin(&core.db).await
}

/// Deletes one binned note for good.
#[tauri::command]
pub async fn note_purge(id: String, core: State<'_, Core>) -> Result<()> {
    notes::purge(&core.db, &*core.clock, &id).await
}

/// Empties the Bin; returns how many notes were removed.
#[tauri::command]
pub async fn bin_empty(core: State<'_, Core>) -> Result<u64> {
    notes::empty_bin(&core.db, &*core.clock).await
}

#[tauri::command]
pub async fn note_discard_if_empty(id: String, core: State<'_, Core>) -> Result<bool> {
    notes::discard_if_empty(&core.db, &*core.clock, &id).await
}

// ---- meetings (TRD §4.3) --------------------------------------------------------------------

#[tauri::command]
pub async fn meetings_list(
    filters: MeetingFilters,
    core: State<'_, Core>,
) -> Result<Vec<MeetingSummary>> {
    meetings::list(&core.db, filters).await
}

#[tauri::command]
pub async fn meeting_get(id: String, core: State<'_, Core>) -> Result<Meeting> {
    meetings::get(&core.db, &id).await
}

#[tauri::command]
pub async fn meeting_create(input: MeetingInput, core: State<'_, Core>) -> Result<Meeting> {
    meetings::create(&core.db, &*core.clock, input).await
}

#[tauri::command]
pub async fn meeting_update(
    id: String,
    input: MeetingInput,
    core: State<'_, Core>,
) -> Result<Meeting> {
    meetings::update(&core.db, &*core.clock, &id, input).await
}

#[tauri::command]
pub async fn meeting_delete(id: String, core: State<'_, Core>) -> Result<()> {
    meetings::delete(&core.db, &*core.clock, &id).await
}

#[tauri::command]
pub async fn action_item_convert(action_item_id: String, core: State<'_, Core>) -> Result<Task> {
    meetings::convert_action_item(&core.db, &*core.clock, &action_item_id).await
}

#[tauri::command]
pub async fn participants_suggest(prefix: String, core: State<'_, Core>) -> Result<Vec<String>> {
    meetings::suggest_participants(&core.db, &prefix).await
}

// ---- reminders ------------------------------------------------------------------------------

#[tauri::command]
pub async fn reminders_list(include_done: bool, core: State<'_, Core>) -> Result<Vec<Reminder>> {
    reminders::list(&core.db, include_done).await
}

#[tauri::command]
pub async fn reminder_create(input: ReminderInput, core: State<'_, Core>) -> Result<Reminder> {
    reminders::create(&core.db, &*core.clock, input).await
}

#[tauri::command]
pub async fn reminder_update(
    id: String,
    patch: ReminderPatch,
    core: State<'_, Core>,
) -> Result<Reminder> {
    reminders::update(&core.db, &*core.clock, &id, patch).await
}

#[tauri::command]
pub async fn reminder_set_done(id: String, done: bool, core: State<'_, Core>) -> Result<Reminder> {
    reminders::set_done(&core.db, &*core.clock, &id, done).await
}

/// Star or unstar a reminder (FR-107).
#[tauri::command]
pub async fn reminder_star(id: String, starred: bool, core: State<'_, Core>) -> Result<Reminder> {
    reminders::set_starred(&core.db, &*core.clock, &id, starred).await
}

#[tauri::command]
pub async fn reminder_delete(id: String, core: State<'_, Core>) -> Result<()> {
    reminders::delete(&core.db, &*core.clock, &id).await
}

// ---- diary (FR-86) ---------------------------------------------------------------------------

#[tauri::command]
pub async fn diary_status(core: State<'_, Core>) -> Result<DiaryStatus> {
    diary::status(&core.db, &core.diary).await
}

#[tauri::command]
pub async fn diary_setup(password: String, core: State<'_, Core>) -> Result<String> {
    diary::setup(&core.db, &core.diary, &*core.clock, &password).await
}

#[tauri::command]
pub async fn diary_unlock(password: String, core: State<'_, Core>) -> Result<()> {
    diary::unlock(&core.db, &core.diary, &password).await
}

#[tauri::command]
pub async fn diary_recover(
    recovery_key: String,
    password: String,
    core: State<'_, Core>,
) -> Result<()> {
    diary::recover(&core.db, &core.diary, &recovery_key, &password).await
}

#[tauri::command]
pub fn diary_lock(core: State<'_, Core>) {
    diary::lock(&core.diary);
}

#[tauri::command]
pub async fn diary_days(core: State<'_, Core>) -> Result<Vec<String>> {
    diary::days(&core.db, &core.diary).await
}

#[tauri::command]
pub async fn diary_get(day: String, core: State<'_, Core>) -> Result<String> {
    diary::day_get(&core.db, &core.diary, &day).await
}

#[tauri::command]
pub async fn diary_search(query: String, core: State<'_, Core>) -> Result<Vec<DiaryHit>> {
    diary::search(&core.db, &core.diary, &query).await
}

#[tauri::command]
pub async fn diary_save(day: String, text: String, core: State<'_, Core>) -> Result<()> {
    diary::day_save(&core.db, &core.diary, &*core.clock, &day, &text).await
}

#[tauri::command]
pub async fn diary_erase(core: State<'_, Core>) -> Result<()> {
    diary::erase(&core.db, &core.diary).await
}

// ---- courses (student pack) -------------------------------------------------------------------

#[tauri::command]
pub async fn courses_list(core: State<'_, Core>) -> Result<Vec<Course>> {
    courses::list(&core.db).await
}

#[tauri::command]
pub async fn course_create(name: String, core: State<'_, Core>) -> Result<Course> {
    courses::create(&core.db, &*core.clock, &name).await
}

#[tauri::command]
pub async fn course_rename(id: String, name: String, core: State<'_, Core>) -> Result<Course> {
    courses::rename(&core.db, &id, &name).await
}

#[tauri::command]
pub async fn course_delete(id: String, core: State<'_, Core>) -> Result<()> {
    courses::delete(&core.db, &id).await
}

#[tauri::command]
pub async fn exams_from(from: String, core: State<'_, Core>) -> Result<Vec<Exam>> {
    courses::exams_from(&core.db, &from).await
}

#[tauri::command]
pub async fn exam_add(
    course_id: Option<String>,
    title: String,
    on_date: String,
    core: State<'_, Core>,
) -> Result<Exam> {
    courses::exam_add(&core.db, &*core.clock, course_id, &title, &on_date).await
}

#[tauri::command]
pub async fn exam_delete(id: String, core: State<'_, Core>) -> Result<()> {
    courses::exam_delete(&core.db, &id).await
}

#[tauri::command]
pub async fn task_set_course(
    task_id: String,
    course_id: Option<String>,
    core: State<'_, Core>,
) -> Result<()> {
    courses::set_task_course(&core.db, &task_id, course_id).await
}

#[tauri::command]
pub async fn note_set_course(
    note_id: String,
    course_id: Option<String>,
    core: State<'_, Core>,
) -> Result<()> {
    courses::set_note_course(&core.db, &note_id, course_id).await
}

#[tauri::command]
pub async fn course_links(core: State<'_, Core>) -> Result<CourseLinks> {
    courses::links(&core.db).await
}

#[tauri::command]
pub async fn weekly_report(from: String, core: State<'_, Core>) -> Result<WeeklyReport> {
    courses::weekly_report(&core.db, &*core.clock, &from).await
}

// ---- labels ---------------------------------------------------------------------------------

#[tauri::command]
pub async fn labels_list(core: State<'_, Core>) -> Result<Vec<LabelCount>> {
    labels::list(&core.db).await
}

#[tauri::command]
pub async fn label_create(name: String, core: State<'_, Core>) -> Result<Label> {
    labels::create(&core.db, &*core.clock, &name).await
}

#[tauri::command]
pub async fn label_rename(id: String, name: String, core: State<'_, Core>) -> Result<Label> {
    labels::rename(&core.db, &*core.clock, &id, &name).await
}

#[tauri::command]
pub async fn label_delete(id: String, core: State<'_, Core>) -> Result<()> {
    labels::delete(&core.db, &*core.clock, &id).await
}

// ---- tasks (the Today card; the full Tasks screen is CP1 UI) --------------------------------

#[tauri::command]
pub async fn tasks_query(
    view: TaskView,
    filters: Option<TaskFilters>,
    core: State<'_, Core>,
) -> Result<Vec<TaskRow>> {
    tasks::query(&core.db, &*core.clock, view, filters.unwrap_or_default()).await
}

#[tauri::command]
pub async fn task_quick_add(title: String, core: State<'_, Core>) -> Result<Task> {
    tasks::quick_add(&core.db, &*core.clock, &title).await
}

#[tauri::command]
pub async fn task_transition(id: String, to: TaskStatus, core: State<'_, Core>) -> Result<Task> {
    tasks::transition(&core.db, &*core.clock, &id, to).await
}

/// Edit a task's title, priority, planned day and deadline in one go.
#[tauri::command]
pub async fn task_update(id: String, edit: TaskEdit, core: State<'_, Core>) -> Result<Task> {
    tasks::update(&core.db, &*core.clock, &id, edit.into()).await
}

#[tauri::command]
pub async fn task_get(id: String, core: State<'_, Core>) -> Result<Task> {
    tasks::get(&core.db, &id).await
}

/// Everything that happened to a task, oldest first.
#[tauri::command]
pub async fn task_history(id: String, core: State<'_, Core>) -> Result<Vec<TaskEvent>> {
    tasks::history(&core.db, &id).await
}

/// Move the planned day later. `to` is a local day `YYYY-MM-DD`.
#[tauri::command]
pub async fn task_defer(id: String, to: String, core: State<'_, Core>) -> Result<Task> {
    let day = chrono::NaiveDate::parse_from_str(&to, "%Y-%m-%d")
        .map_err(|_| AppError::validation("to", "Dates look like 2026-10-04."))?;
    tasks::defer(&core.db, &*core.clock, &id, day).await
}

/// Only finished (completed or cancelled) tasks can be deleted.
#[tauri::command]
pub async fn task_delete(id: String, core: State<'_, Core>) -> Result<()> {
    tasks::delete(&core.db, &*core.clock, &id).await
}

fn parse_day(field: &str, s: &str) -> Result<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| AppError::validation(field, "Dates look like 2026-10-04."))
}

/// Replace the links shown next to a task (FR-106).
#[tauri::command]
pub async fn task_set_links(
    id: String,
    links: Vec<LinkInput>,
    core: State<'_, Core>,
) -> Result<Task> {
    task_plans::set_links(&core.db, &*core.clock, &id, links).await
}

/// Star or unstar a task (FR-107).
#[tauri::command]
pub async fn task_star(id: String, starred: bool, core: State<'_, Core>) -> Result<Task> {
    task_plans::star(&core.db, &*core.clock, &id, starred).await
}

/// Chosen days (FR-108) or a repeat rule (FR-109).
#[tauri::command]
pub async fn task_set_plan(id: String, plan: TaskPlan, core: State<'_, Core>) -> Result<Task> {
    task_plans::set_plan(&core.db, &*core.clock, &id, plan).await
}

/// Tick or untick one day of a repeating task. `day` is `YYYY-MM-DD`.
#[tauri::command]
pub async fn task_tick(id: String, day: String, done: bool, core: State<'_, Core>) -> Result<Task> {
    let day = parse_day("day", &day)?;
    task_plans::tick(&core.db, &*core.clock, &id, day, done).await
}

/// The last repeat days of a task, newest first, each ticked or not.
#[tauri::command]
pub async fn task_repeat_days(id: String, core: State<'_, Core>) -> Result<Vec<RepeatDay>> {
    task_plans::history(&core.db, &*core.clock, &id, 14).await
}

#[tauri::command]
pub async fn work_updates_list(task_id: String, core: State<'_, Core>) -> Result<Vec<WorkUpdate>> {
    tasks::work_updates(&core.db, &task_id).await
}

#[tauri::command]
pub async fn work_update_add(
    task_id: String,
    body: String,
    core: State<'_, Core>,
) -> Result<WorkUpdate> {
    tasks::add_work_update(&core.db, &*core.clock, &task_id, &body).await
}

#[tauri::command]
pub async fn work_update_delete(id: String, core: State<'_, Core>) -> Result<()> {
    tasks::delete_work_update(&core.db, &*core.clock, &id).await
}

// ---- focus sessions (FR-05) ----------------------------------------------------------------

#[tauri::command]
pub async fn focus_start(input: FocusStart, core: State<'_, Core>) -> Result<FocusSession> {
    focus::start(&core.db, &*core.clock, input).await
}

/// The running session, if any (one that ran out while Pluto was closed is finished here).
#[tauri::command]
pub async fn focus_task_total(task_id: String, core: State<'_, Core>) -> Result<i64> {
    focus::task_total_ms(&core.db, &task_id).await
}

#[tauri::command]
pub async fn focus_current(core: State<'_, Core>) -> Result<Option<FocusSession>> {
    focus::current(&core.db, &*core.clock).await
}

#[tauri::command]
pub async fn focus_pause(paused: bool, core: State<'_, Core>) -> Result<FocusSession> {
    focus::set_paused(&core.db, &*core.clock, paused).await
}

#[tauri::command]
pub async fn focus_extend(minutes: u32, core: State<'_, Core>) -> Result<FocusSession> {
    focus::extend(&core.db, &*core.clock, minutes).await
}

#[tauri::command]
pub async fn focus_stop(core: State<'_, Core>) -> Result<FocusSession> {
    focus::stop(&core.db, &*core.clock).await
}

/// The time is up; both windows may call this, the second call changes nothing.
#[tauri::command]
pub async fn focus_finish(id: String, core: State<'_, Core>) -> Result<FocusSession> {
    focus::finish(&core.db, &*core.clock, &id).await
}

// ---- auto-delete and the task Bin (FR-46) ---------------------------------------------------

#[tauri::command]
pub async fn retention_preview(
    rules: RetentionRules,
    core: State<'_, Core>,
) -> Result<RetentionCounts> {
    retention::preview(&core.db, &*core.clock, rules).await
}

#[tauri::command]
pub async fn retention_apply(
    rules: RetentionRules,
    core: State<'_, Core>,
) -> Result<RetentionCounts> {
    retention::apply(&core.db, &*core.clock, rules).await
}

#[tauri::command]
pub async fn tasks_binned(core: State<'_, Core>) -> Result<Vec<Task>> {
    retention::binned_tasks(&core.db).await
}

#[tauri::command]
pub async fn task_restore(id: String, core: State<'_, Core>) -> Result<Task> {
    retention::restore_task(&core.db, &*core.clock, &id).await
}

// ---- daily logs (the Time screen) -----------------------------------------------------------

/// The log for a local day `YYYY-MM-DD`: live for today, frozen for earlier days, `null` if none.
#[tauri::command]
pub async fn daily_log_get(date: String, core: State<'_, Core>) -> Result<Option<DailyLog>> {
    let day = chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d")
        .map_err(|_| AppError::validation("date", "Dates look like 2026-10-04."))?;
    daily_log::get(&core.db, &*core.clock, day).await
}

// ---- usage: "where my time went" (opt-in, local only) ----------------------------------------

/// Time per app, per site and per local hour inside `[from, to)` (ms since the epoch, UTC).
#[tauri::command]
pub async fn usage_summary(
    from: i64,
    to: i64,
    usage: State<'_, Arc<UsageService>>,
) -> Result<UsageSummary> {
    usage.summary(from, to).await
}

/// The exact active spans of one app (by the `app` name in `usage_summary`), clipped to the range.
#[tauri::command]
pub async fn usage_app_sessions(
    app: String,
    from: i64,
    to: i64,
    usage: State<'_, Arc<UsageService>>,
) -> Result<Vec<TimeRange>> {
    usage.app_sessions(&app, from, to).await
}

/// The exact active spans on one site, clipped to the range.
#[tauri::command]
pub async fn usage_domain_sessions(
    domain: String,
    from: i64,
    to: i64,
    usage: State<'_, Arc<UsageService>>,
) -> Result<Vec<TimeRange>> {
    usage.domain_sessions(&domain, from, to).await
}

#[tauri::command]
pub async fn tracking_status(
    tracking: State<'_, Tracking>,
    settings: State<'_, Arc<SettingsService>>,
) -> Result<TrackingStatus> {
    let all = settings.get_all().await?;
    let enabled = all
        .get(SettingsKey::TrackingApps.as_str())
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Ok(TrackingStatus {
        supported: tracking.supported(),
        running: tracking.is_running(),
        enabled,
    })
}

/// Deletes usage overlapping `[from, to)`; a missing bound is open-ended, so both missing deletes
/// everything. Returns how many sessions were removed.
#[tauri::command]
pub async fn usage_delete(
    from: Option<i64>,
    to: Option<i64>,
    usage: State<'_, Arc<UsageService>>,
) -> Result<u64> {
    match (from, to) {
        (None, None) => usage.delete_all().await,
        (from, to) => {
            usage
                .delete_range(from.unwrap_or(0), to.unwrap_or(i64::MAX))
                .await
        }
    }
}

/// Every site seen, with its category and whether it is tracked.
#[tauri::command]
pub async fn usage_domains(usage: State<'_, Arc<UsageService>>) -> Result<Vec<DomainSetting>> {
    usage.domains().await
}

/// "Do not track" for a site; turning it off also deletes what was recorded for it.
#[tauri::command]
pub async fn domain_set_tracked(
    domain: String,
    tracked: bool,
    usage: State<'_, Arc<UsageService>>,
) -> Result<()> {
    usage.set_domain_tracked(&domain, tracked).await
}

#[tauri::command]
pub async fn domain_set_category(
    domain: String,
    category: String,
    usage: State<'_, Arc<UsageService>>,
) -> Result<()> {
    usage.set_domain_category(&domain, &category).await
}

/// Ingestion point for the browser extensions (PRD 4.7), via the native-messaging bridge.
///
/// `domain` must be a bare host name (never a URL, path or title) and **incognito/private
/// windows are the extension's responsibility: it must never call this for them.** The core
/// cannot tell, so it also refuses everything while `tracking.apps` is off. Returns whether the
/// session was stored (false: too short, or the site is marked "do not track").
#[tauri::command]
pub async fn usage_record_domain(
    browser: String,
    domain: String,
    started_at: i64,
    ended_at: i64,
    usage: State<'_, Arc<UsageService>>,
    settings: State<'_, Arc<SettingsService>>,
) -> Result<bool> {
    let all = settings.get_all().await?;
    let enabled = all
        .get(SettingsKey::TrackingApps.as_str())
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !enabled {
        return Ok(false);
    }
    usage
        .record_domain_session(&browser, &domain, started_at, ended_at)
        .await
}

// ---- your data: export, import, delete all (F0-11) --------------------------------------------

/// Where an export went.
#[derive(serde::Serialize)]
pub struct Exported {
    pub path: String,
    pub bytes: u64,
}

/// Ask where to save, then write everything there. `None` if the person cancelled the dialog.
/// The path comes only from the system dialog, never from the webview.
#[tauri::command]
pub async fn data_export(app: AppHandle, core: State<'_, Core>) -> Result<Option<Exported>> {
    let version = app.package_info().version.to_string();
    let document = data::export(&core.db, &*core.clock, &version).await?;
    let name = format!("pluto-export-{}.json", core.clock.today());
    let picker = app.clone();
    let chosen = tauri::async_runtime::spawn_blocking(move || {
        picker
            .dialog()
            .file()
            .set_file_name(&name)
            .add_filter("Pluto export", &["json"])
            .blocking_save_file()
    })
    .await
    .map_err(|_| AppError::internal("The save dialog stopped unexpectedly."))?;
    let Some(chosen) = chosen else {
        return Ok(None);
    };
    let path = chosen
        .into_path()
        .map_err(|_| AppError::io("Pluto can't save to that place."))?;
    let bytes = serde_json::to_vec_pretty(&document)
        .map_err(|_| AppError::internal("Pluto couldn't write the export."))?;
    let size = bytes.len() as u64;
    let target = path.clone();
    tauri::async_runtime::spawn_blocking(move || std::fs::write(target, bytes))
        .await
        .map_err(|_| AppError::internal("Saving stopped unexpectedly."))?
        .map_err(|_| AppError::io("Pluto couldn't save the file there."))?;
    let shown = path.to_string_lossy().into_owned();
    core.bus.publish(Event::DataExported {
        at: core.clock.now_ms(),
        path: shown.clone(),
        bytes: size,
    });
    Ok(Some(Exported {
        path: shown,
        bytes: size,
    }))
}

/// Ask for an export file and restore it into an empty Pluto. `None` if the dialog was cancelled.
#[tauri::command]
pub async fn data_import(
    app: AppHandle,
    core: State<'_, Core>,
) -> Result<Option<std::collections::BTreeMap<String, u64>>> {
    const MAX_BYTES: u64 = 1 << 30;
    let picker = app.clone();
    let chosen = tauri::async_runtime::spawn_blocking(move || {
        picker
            .dialog()
            .file()
            .add_filter("Pluto export", &["json"])
            .blocking_pick_file()
    })
    .await
    .map_err(|_| AppError::internal("The open dialog stopped unexpectedly."))?;
    let Some(chosen) = chosen else {
        return Ok(None);
    };
    let path = chosen
        .into_path()
        .map_err(|_| AppError::io("Pluto can't open that file."))?;
    let document = tauri::async_runtime::spawn_blocking(move || {
        let size = std::fs::metadata(&path)?.len();
        if size > MAX_BYTES {
            return Ok(Err(AppError::validation(
                "file",
                "That file is too large to be a Pluto export.",
            )));
        }
        let bytes = std::fs::read(&path)?;
        Ok::<_, std::io::Error>(
            serde_json::from_slice::<serde_json::Value>(&bytes)
                .map_err(|_| AppError::validation("file", "That file isn't a Pluto export.")),
        )
    })
    .await
    .map_err(|_| AppError::internal("Reading stopped unexpectedly."))?
    .map_err(|_| AppError::io("Pluto couldn't read that file."))??;
    data::import(&core.db, &*core.clock, document)
        .await
        .map(Some)
}

/// Erase everything. `confirm` must be exactly `DELETE`.
#[tauri::command]
pub async fn data_delete_all(confirm: String, core: State<'_, Core>) -> Result<()> {
    data::delete_all(&core.db, &*core.clock, &confirm).await
}

// ---- folders and reset (Settings › Data, Advanced) -----------------------------------------

/// Where Pluto keeps its files, shown read-only in Settings (R1-80).
#[derive(Debug, Clone, serde::Serialize)]
pub struct AppPaths {
    pub data_dir: std::path::PathBuf,
    pub database: std::path::PathBuf,
    pub logs: std::path::PathBuf,
}

#[tauri::command]
pub fn app_paths(paths: State<'_, AppPaths>) -> AppPaths {
    paths.inner().clone()
}

/// Open the data folder or the logs folder in the file manager. Only these two.
#[tauri::command]
pub fn folder_open(which: String, paths: State<'_, AppPaths>, app: AppHandle) -> Result<()> {
    let dir = match which.as_str() {
        "data" => &paths.data_dir,
        "logs" => &paths.logs,
        _ => return Err(AppError::validation("which", "Unknown folder.")),
    };
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|_| AppError::io("Pluto couldn't open that folder."))
}

/// Settings › Advanced › Reset settings. Data and layout preferences stay.
#[tauri::command]
pub async fn settings_reset(settings: State<'_, Arc<SettingsService>>) -> Result<()> {
    settings.reset_all().await
}

// ---- links ----------------------------------------------------------------------------------

/// Opens an `http`, `https` or `mailto` link in the default app. Anything else is refused.
#[tauri::command]
pub async fn open_url(url: String, app: AppHandle) -> Result<()> {
    let url = links::validate_external_url(&url)?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|_| AppError::io("Pluto couldn't open that link."))
}
