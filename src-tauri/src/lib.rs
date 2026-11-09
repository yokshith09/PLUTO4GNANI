//! Pluto desktop shell. Thin by design: windows, tray and OS integration live
//! here; logic lives in `pluto-core` (ADR-002).

mod auto_meeting;
mod autostart;
mod dictation;
mod earlier_app;
mod ipc;
mod logs;
mod media;
mod meeting_ai;
mod native_mic;
mod note_window;
mod notify;
mod pet;
mod pets;
mod shortcuts;
mod sites;
mod tabs;
mod tracking;
mod tray;
mod typing;
mod voice;

use std::sync::Arc;

use pluto_core::bus::{EventBus, Recv};
use pluto_core::clock::{Clock, SystemClock};
use pluto_core::db::Database;
use pluto_core::events::Event;
use pluto_core::notes;
use pluto_core::reminders::{self, ReminderScheduler};
use pluto_core::scheduler::{DbLastSeen, Scheduler};
use pluto_core::settings_service::SettingsService;
use pluto_core::usage::{self, UsageService};
use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

/// What the dashboard window remembers between runs: size, place, maximized. Not whether it was
/// showing (Pluto decides that at startup: hidden for `--hidden`, shown otherwise).
fn window_state_flags() -> StateFlags {
    StateFlags::all() - StateFlags::VISIBLE
}

/// Putting the dashboard away (closing it, the pet's double click, the shortcut) locks the diary,
/// and tells the page so it drops what it shows. The text never stays readable behind a hidden window.
pub(crate) fn lock_diary<R: tauri::Runtime>(app: &AppHandle<R>) {
    if let Some(core) = app.try_state::<Core>() {
        pluto_core::diary::lock(&core.diary);
    }
    let _ = app.emit_to("main", "pluto://command", "diary-locked");
}

/// What the shell needs at shutdown.
pub(crate) struct Core {
    pub(crate) db: Arc<Database>,
    pub(crate) bus: EventBus,
    pub(crate) clock: Arc<SystemClock>,
    /// The diary key, in memory only while unlocked.
    pub(crate) diary: pluto_core::diary::Diary,
}

/// Background tasks that live as long as the app. They park on the bus or a timer between
/// events, so they cost nothing while idle. Dropping the schedulers stops them.
#[allow(dead_code)] // held only so the tasks are not dropped
struct Background {
    day: Scheduler,
    reminders: ReminderScheduler,
}

/// Quit path (R0-03): announce, let the writer flush, then exit every process.
pub(crate) fn quit(app: &AppHandle) {
    let _ = app.save_window_state(window_state_flags());
    if let Some(core) = app.try_state::<Core>() {
        core.bus.publish(Event::AppShuttingDown {
            at: core.clock.now_ms(),
        });
        // The collector closes the open session and writes it; that must land before the
        // database writer stops.
        if let Some(tracking) = app.try_state::<tracking::Tracking>() {
            tauri::async_runtime::block_on(tracking.finish());
        }
        tauri::async_runtime::block_on(core.db.shutdown());
    }
    app.exit(0);
}

/// Bus -> every webview (TRD §3). A lagging subscriber tells the UI to refetch.
fn forward_events(app: AppHandle, bus: &EventBus) {
    let mut events = bus.subscribe();
    tauri::async_runtime::spawn(async move {
        loop {
            match events.recv().await {
                Recv::Event(event) => {
                    if let Event::ReminderDue { reminder, .. } = &event {
                        notify::reminder(&app, reminder);
                    }
                    let _ = app.emit("pluto://event", &event);
                }
                Recv::Resync { .. } => {
                    let at = SystemClock::new().now_ms();
                    let _ = app.emit("pluto://event", &Event::ResyncRequired { at });
                }
                Recv::Closed => break,
            }
        }
    });
}

/// WebView2 ignores `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` once the app passes its own browser
/// arguments, and wry always does. The end-to-end driver hands its debugging flags over that
/// variable, so on Windows we append it to each window's arguments ourselves (R11). Unset in
/// normal use, so this changes nothing for users.
fn forward_webview_args(context: &mut tauri::Context) {
    #[cfg(windows)]
    if let Some(extra) = std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS")
        .ok()
        .filter(|v| !v.trim().is_empty())
    {
        for window in &mut context.config_mut().app.windows {
            window.additional_browser_args = Some(format!(
                "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection {extra}"
            ));
        }
    }
    #[cfg(not(windows))]
    let _ = context;
}

pub fn run() {
    let mut context = tauri::generate_context!();
    forward_webview_args(&mut context);
    let app = tauri::Builder::default()
        // A second launch focuses the existing window and exits (R0-01).
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // A second login-item launch (`--hidden`) must not pop the window up.
            if !autostart::is_hidden_launch(&args) {
                tray::show_main(app);
            }
        }))
        // F0-09: the login item starts Pluto with `--hidden` (quiet start in the tray).
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![autostart::HIDDEN_FLAG]),
        ))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        // Only used from Rust: the file dialogs for export and import. The webview never gets a path.
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        // The dashboard opens at its last size and place (first run: the size in tauri.conf.json,
        // not maximized). The pet places itself.
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_denylist(&[pet::LABEL])
                .with_filter(|label| !label.starts_with(note_window::PREFIX))
                .with_state_flags(window_state_flags())
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            ipc::settings_get_all,
            ipc::setting_set,
            ipc::prefs_get,
            ipc::prefs_set,
            ipc::notes_list,
            ipc::note_get,
            ipc::note_create,
            ipc::note_update,
            ipc::note_set_pinned,
            ipc::note_set_archived,
            ipc::note_delete,
            ipc::note_restore,
            ipc::notes_search,
            ipc::bin_list,
            ipc::note_purge,
            ipc::bin_empty,
            ipc::note_discard_if_empty,
            ipc::reminders_list,
            ipc::reminder_create,
            ipc::reminder_update,
            ipc::reminder_set_done,
            ipc::reminder_star,
            ipc::reminder_delete,
            ipc::diary_status,
            ipc::diary_setup,
            ipc::diary_unlock,
            ipc::diary_recover,
            ipc::diary_lock,
            ipc::diary_days,
            ipc::diary_get,
            ipc::diary_save,
            ipc::diary_search,
            ipc::diary_erase,
            ipc::labels_list,
            ipc::label_create,
            ipc::label_rename,
            ipc::label_delete,
            ipc::tasks_query,
            ipc::task_quick_add,
            ipc::task_transition,
            ipc::task_update,
            ipc::task_get,
            ipc::task_history,
            ipc::task_defer,
            ipc::task_delete,
            ipc::task_set_links,
            ipc::task_star,
            ipc::task_set_plan,
            ipc::task_tick,
            ipc::task_repeat_days,
            ipc::focus_start,
            ipc::focus_current,
            ipc::focus_task_total,
            ipc::focus_pause,
            ipc::focus_extend,
            ipc::focus_stop,
            ipc::focus_finish,
            ipc::retention_preview,
            ipc::retention_apply,
            ipc::tasks_binned,
            ipc::task_restore,
            ipc::work_updates_list,
            ipc::work_update_add,
            ipc::work_update_delete,
            ipc::daily_log_get,
            ipc::usage_summary,
            ipc::usage_app_sessions,
            ipc::usage_domain_sessions,
            ipc::tracking_status,
            ipc::usage_delete,
            ipc::usage_domains,
            ipc::domain_set_tracked,
            ipc::domain_set_category,
            ipc::usage_record_domain,
            tabs::pet_open_tabs,
            pets::pets_list,
            pets::pets_import,
            pets::pets_import_zip,
            pets::pets_remove,
            pets::pets_sheet,
            tabs::pet_close_tab,
            ipc::data_export,
            ipc::data_import,
            ipc::data_delete_all,
            ipc::open_url,
            ipc::meetings_list,
            ipc::meeting_get,
            ipc::meeting_create,
            ipc::meeting_update,
            ipc::meeting_delete,
            ipc::action_item_convert,
            ipc::participants_suggest,
            pet::pet_hover,
            pet::pet_solid,
            pet::pet_click,
            pet::pet_double_click,
            pet::pet_command,
            media::pet_media,
            pet::pet_menu,
            notify::notification_test,
            note_window::note_window_open,
            ipc::app_paths,
            ipc::folder_open,
            ipc::settings_reset,
            notify::notification_settings_open,
            // Voice and meeting-AI commands (API_CONTRACTS §4.2, §4.3), registered once at T-007.
            // Later additions go through a contract PR (WORKING_AGREEMENT §3).
            voice::voice_settings_get,
            voice::voice_settings_set,
            voice::voice_key_set,
            voice::voice_key_clear,
            voice::voice_status,
            voice::voice_ptt_start,
            voice::voice_ptt_stop,
            voice::voice_cancel,
            voice::audio_push,
            voice::voice_proposal_respond,
            voice::voice_speak,
            voice::voice_stop_speaking,
            voice::voice_usage,
            voice::voice_history,
            voice::voice_history_clear,
            voice::voice_hands_free_toggle,
            voice::voice_text_turn,
            voice::voice_wake_status,
            voice::voice_wake_teach_begin,
            voice::voice_wake_clip_start,
            voice::voice_wake_clip_stop,
            voice::voice_wake_teach_finish,
            voice::voice_wake_forget,
            voice::privacy_log_list,
            voice::privacy_log_clear,
            meeting_ai::meeting_rec_start,
            meeting_ai::meeting_rec_pause,
            meeting_ai::meeting_rec_resume,
            meeting_ai::meeting_rec_stop,
            auto_meeting::auto_meeting_answer,
            auto_meeting::auto_meeting_stop,
            auto_meeting::auto_meeting_state,
            auto_meeting::auto_meeting_capture_failed,
            meeting_ai::meeting_audio_pick,
            meeting_ai::meeting_import,
            meeting_ai::meeting_job_get,
            meeting_ai::meeting_job_retry,
            meeting_ai::meeting_transcript_get,
            meeting_ai::meeting_speakers_set,
            meeting_ai::meeting_extract,
            meeting_ai::meeting_extraction_apply,
            meeting_ai::meeting_items_get,
            meeting_ai::meeting_recall,
            meeting_ai::meeting_recap_text,
            meeting_ai::meeting_recap,
            meeting_ai::commitments_list,
        ])
        .on_window_event(|window, event| {
            // Closing the main window hides it; the process keeps running (R0-03).
            if window.label() == pet::LABEL {
                match event {
                    WindowEvent::Moved(p) => pet::on_moved(window.app_handle(), p.x, p.y),
                    // B1: an OS close (Alt+F4, sign-out) only hides the pet for now; it never
                    // turns "Show the pet" off, so it comes back on the next start.
                    WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                    // A display scale change can leave the pet the wrong size or off-screen.
                    WindowEvent::ScaleFactorChanged { .. } => pet::reapply(window.app_handle()),
                    _ => {}
                }
                return;
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    lock_diary(window.app_handle());
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            // `PLUTO_DATA_DIR` keeps data and logs in one folder of your choice: the end-to-end
            // tests use it so they never touch the real data. Unset, the OS's usual folders.
            let (data_dir, log_dir, own_folder) = match std::env::var_os("PLUTO_DATA_DIR") {
                Some(dir) if !dir.is_empty() => {
                    let dir = std::path::PathBuf::from(dir);
                    (dir.clone(), dir.join("logs"), false)
                }
                _ => (app.path().app_data_dir()?, app.path().app_log_dir()?, true),
            };
            // Logs first, so everything after (including a failed start) is recorded.
            let logs = logs::init(&log_dir).map(Arc::new);
            #[cfg(windows)]
            {
                tracking::win::trace::set_folder(&data_dir);
                let own: Vec<_> = [pet::LABEL, "main"]
                    .into_iter()
                    .filter_map(|label| {
                        let hwnd = app.get_webview_window(label)?.hwnd().ok()?;
                        Some((hwnd.0, label))
                    })
                    .collect();
                tracking::win::set_own_windows(&own);
            }
            // Pluto starts with its own empty database. The Loaf app's data (`app.loaf.desktop`) is
            // never read or copied: the two apps keep separate data.
            let _ = own_folder;
            std::fs::create_dir_all(&data_dir)?;
            app.manage(ipc::AppPaths {
                data_dir: data_dir.clone(),
                database: data_dir.join("pluto.db"),
                logs: log_dir,
            });

            let bus = EventBus::default();
            let clock = Arc::new(SystemClock::new());
            let db = Arc::new(Database::open(&data_dir.join("pluto.db"), bus.clone())?);

            let settings = Arc::new(SettingsService::new(db.clone(), clock.clone()));
            app.manage(settings.clone());
            let usage_service = Arc::new(UsageService::new(db.clone(), clock.clone()));
            app.manage(usage_service.clone());
            forward_events(app.handle().clone(), &bus);
            let voice_shell = voice::VoiceShell::new(
                app.handle().clone(),
                db.clone(),
                clock.clone(),
                data_dir.clone(),
            )?;
            app.manage(voice_shell.meeting_jobs(
                app.handle().clone(),
                db.clone(),
                clock.clone(),
                data_dir.join("meetings"),
            )?);
            app.manage(voice_shell);
            let recorder = Arc::new(pluto_intel::meeting::recorder::Recorder::new(
                db.clone(),
                clock.clone(),
                data_dir.join("meetings").join("recordings"),
            ));
            // Recordings the app stopped in the middle of are repaired at startup (T-207).
            let recovering = recorder.clone();
            tauri::async_runtime::spawn(async move {
                if let Ok(found) = recovering.recover().await {
                    if !found.is_empty() {
                        tracing::info!(count = found.len(), "repaired unfinished recordings");
                    }
                }
            });
            app.manage(meeting_ai::MeetingShell::new(recorder));
            // Always-on listening starts with the app (when it was left on and the wake word is
            // taught), not only when a setting changes.
            app.state::<voice::VoiceShell>().refresh_wake();
            // Google Meet auto mode: looks for a call only while it is switched on in Settings.
            app.manage(auto_meeting::AutoMeetShell::new());
            auto_meeting::start(app.handle().clone());
            app.manage(Core {
                db: db.clone(),
                bus: bus.clone(),
                clock: clock.clone(),
                diary: pluto_core::diary::Diary::default(),
            });

            // Everything below subscribes to the bus before anything can publish to it.
            let dyn_clock: Arc<dyn Clock> = clock.clone();
            let background = tauri::async_runtime::block_on(async {
                // The Bin is swept at startup and on every day rollover (no timer of its own).
                notes::spawn_bin_janitor(db.clone(), dyn_clock.clone(), &bus);
                let (sweep_db, sweep_clock) = (db.clone(), dyn_clock.clone());
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = notes::purge_expired(&sweep_db, &*sweep_clock).await {
                        tracing::warn!(code = ?error.code, "couldn't clear old notes out of the Bin at startup");
                    }
                });
                // Reminders missed while Pluto was closed fire once, right after start.
                let reminders = reminders::spawn_scheduler(db.clone(), dyn_clock.clone(), &bus);
                // Old usage is purged the same way: at startup and on every day rollover.
                usage::spawn_janitor(usage_service.clone(), &bus);
                let sweep = usage_service.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = sweep.purge_expired().await {
                        tracing::warn!(code = ?error.code, "couldn't clear old usage data at startup");
                    }
                });
                // App tracking is opt-in: this only starts the OS source if `tracking.apps` is on.
                // Website time from the address bar: only while `tracking.apps` and `tracking.sites` are on.
                sites::start(usage_service.clone(), settings.clone(), dyn_clock.clone(), &bus);
                let tracking = tracking::start(usage_service, settings.clone(), dyn_clock.clone(), &bus)
                    .unwrap_or_else(|_unsupported| tracking::Tracking::unsupported());
                let day = Scheduler::spawn(dyn_clock, DbLastSeen(db.clone()), bus.clone());
                (Background { day, reminders }, tracking)
            });
            let (background, tracking) = background;
            app.manage(background);
            app.manage(tracking);

            tray::setup_tray(app.handle(), &tray::MenuRegistry::base(), quit)?;
            autostart::start(app.handle().clone(), settings.clone(), &bus);
            // A Mac has no installer step: offer to clear away an earlier build, once.
            #[cfg(target_os = "macos")]
            earlier_app::offer_removal(app.handle().clone(), settings.clone());
            app.manage(Arc::new(pet::PetState::default()));
            app.on_menu_event(|app, event| pet::on_menu(app, event.id().as_ref()));
            pet::start(app.handle().clone(), settings.clone(), &bus);
            shortcuts::start(app.handle().clone(), settings.clone(), &bus);
            typing::start(app.handle().clone(), settings.clone(), &bus);
            if let Some(logs) = logs {
                logs::follow_setting(logs, settings.clone(), &bus);
            }
            // The window is created hidden; a normal launch shows it, a login launch does not.
            if !autostart::is_hidden_launch(std::env::args()) {
                tray::show_main(app.handle());
            }
            Ok(())
        })
        .build(context)
        .expect("failed to build Pluto");

    app.run(|_app, event| {
        // The last window closing must not end the process; only `quit` does (code is Some).
        if let RunEvent::ExitRequested {
            api, code: None, ..
        } = event
        {
            api.prevent_exit();
        }
    });
}
