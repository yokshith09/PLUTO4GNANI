//! The desktop pet (F1-22): a small transparent, always-on-top window with Pluto the bear.
//!
//! * **Click** opens the dashboard, **double-click** hides it, **right-click** opens a menu,
//!   **drag** moves the pet. Hovering grows the window to show a summary card beside the pet;
//!   the pet itself never moves on screen while the card is open.
//! * The pet's place is remembered (`pet.position` preference, physical pixels) and checked
//!   against the connected displays at startup: a pet on a display that is gone comes back to
//!   the bottom-right of the main display (R1-61).
//! * `pet.visible` and `pet.size` are settings; changing them from Settings, the tray, the menu
//!   or the pet shortcut (if one is set) applies at once.
//!
//! Event-driven: one task parked on the bus, plus a one-shot delayed save after a drag.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use pluto_core::bus::{EventBus, Recv};
use pluto_core::error::{AppError, Result};
use pluto_core::events::Event;
use pluto_core::pet_screen::pet_on_screen;
use pluto_core::settings_service::SettingsService;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Runtime, State, WebviewWindow,
};

pub const LABEL: &str = "pet";
const POSITION_PREF: &str = "pet.position";
/// Space between the pet and the edge of the screen when it is placed by default.
const MARGIN: i32 = 24;
/// Space between the pet and its card.
const GAP: u32 = 8; // logical pixels, as `gap-2` in the page

/// A rectangle in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w as i32 && y < self.y + self.h as i32
    }
}

/// Where the pet sits inside the grown window, so the page can lay the card out around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Anchor {
    /// `"right"`: the pet is at the right edge and the card extends to the left.
    pub pet: &'static str,
    /// `"above"` or `"below"` the pet.
    pub card: &'static str,
}

#[cfg(test)]
/// The window grown to hold the card next to the pet, kept inside the work area. The pet's
/// own square stays exactly where it was.
pub fn expanded(pet: Rect, card_w: u32, card_h: u32, work: Rect) -> (Rect, Anchor) {
    expanded_with(pet, card_w, card_h, GAP, work, false)
}

/// Like [`expanded`], but `prefer_below` keeps the panel under the pet whenever it fits there
/// (the focus timer and media buttons always sit below the pet: owner 2026-10-25, H1/H4).
/// `gap` is the space between pet and panel in physical pixels: the page's 8 CSS pixels times the
/// screen scale. (A fixed 8 made the window shorter than the page at 125% or 150%, so the pet was
/// pushed a few pixels down every time the card opened.)
pub fn expanded_with(
    pet: Rect,
    card_w: u32,
    card_h: u32,
    gap: u32,
    work: Rect,
    prefer_below: bool,
) -> (Rect, Anchor) {
    let w = pet.w.max(card_w);
    let h = pet.h + gap + card_h;
    // Above when it fits there; else below when it fits; else whichever side has more room, so
    // on a short screen the card is cut as little as possible (FR-43). Same for left/right.
    let (room_above, room_below) = (
        pet.y - work.y,
        work.y + work.h as i32 - (pet.y + pet.h as i32),
    );
    let need_v = (card_h + gap) as i32;
    let above = if prefer_below && room_below >= need_v {
        false
    } else {
        room_above >= need_v || (room_below < need_v && room_above >= room_below)
    };
    let (room_left, room_right) = (
        pet.x + pet.w as i32 - work.x,
        work.x + work.w as i32 - pet.x,
    );
    let leftwards = room_left >= w as i32 || (room_right < w as i32 && room_left >= room_right);
    let x = if leftwards {
        pet.x + pet.w as i32 - w as i32
    } else {
        pet.x
    };
    let y = if above {
        pet.y - (card_h + gap) as i32
    } else {
        pet.y
    };
    (
        Rect { x, y, w, h },
        Anchor {
            pet: if leftwards { "right" } else { "left" },
            card: if above { "above" } else { "below" },
        },
    )
}

/// Where the pet's square is now, if the grown window (set at `grown`) was dragged to `(x, y)`.
/// The pet keeps its place inside the grown window, so it moved by the same amount.
pub fn follow(base: Rect, grown: Rect, x: i32, y: i32) -> Rect {
    Rect {
        x: base.x + (x - grown.x),
        y: base.y + (y - grown.y),
        ..base
    }
}

/// Where the pet's square is, for saving. While the window is grown for the card, the window's own
/// corner is not the pet's: the pet is `base` moved by however far the grown window was dragged.
/// (Saving the corner, or skipping the save while grown, sent the pet back to an old spot the next
/// time settings were applied: owner 2026-10-27, "it goes to another place".)
pub fn pet_spot(
    expanded: bool,
    base: Option<Rect>,
    grown: Option<Rect>,
    outer: (i32, i32),
) -> (i32, i32) {
    match (expanded, base) {
        (true, Some(base)) => {
            let r = follow(base, grown.unwrap_or(base), outer.0, outer.1);
            (r.x, r.y)
        }
        _ => outer,
    }
}

/// Bottom-right of the work area.
pub fn default_spot(size: u32, work: Rect) -> (i32, i32) {
    (
        work.x + work.w as i32 - size as i32 - MARGIN,
        work.y + work.h as i32 - size as i32 - MARGIN,
    )
}

/// Pet edge length in logical pixels for `pet.size`.
/// A saved spot is used only if the pet's **centre** lands inside a connected screen's work
/// area; checking one corner let a pet end up mostly off-screen after a layout change (B6).
pub fn spot_fits(spot: (i32, i32), size: u32, monitors: &[Rect]) -> bool {
    let half = (size / 2) as i32;
    monitors
        .iter()
        .any(|m| m.contains(spot.0 + half, spot.1 + half))
}

/// "Show or hide pet" (tray, shortcut): decided by what is **on screen**, not the stored
/// setting, so one press always does the visible thing even when they disagree (B4).
pub fn toggle_target(window_visible: bool) -> bool {
    !window_visible
}

/// The wake watcher's tick and the gap that means the computer slept in between (B2/S1).
pub const WAKE_TICK: Duration = Duration::from_secs(60);
pub fn slept(expected: Duration, elapsed_wall: Duration) -> bool {
    elapsed_wall > expected + Duration::from_secs(60)
}

pub fn logical_size(setting: &str) -> f64 {
    match setting {
        "S" => 96.0,
        "L" => 160.0,
        _ => 128.0,
    }
}

#[derive(Default)]
pub struct PetState {
    expanded: AtomicBool,
    /// The pet's own square, while the window is grown.
    base: Mutex<Option<Rect>>,
    /// The grown window as it was set, to tell whether it was dragged since.
    grown: Mutex<Option<Rect>>,
    save_pending: AtomicBool,
    /// The last spot written, so opening and closing the card does not write the same spot again.
    saved: Mutex<Option<(i32, i32)>>,
    /// The parts of the window that are drawn (the pet, an open card or bubble), in CSS pixels
    /// from the window's top-left. Everywhere else clicks go through to the app underneath.
    /// `None` until the page has said: then the whole window takes the pointer.
    solid: Mutex<Option<Vec<[f64; 4]>>>,
    /// A drag is on: the whole window keeps the pointer until it ends.
    holding: AtomicBool,
}

/// Is the point (CSS pixels from the window's top-left) on a drawn part of the window?
pub fn on_solid(solid: Option<&[[f64; 4]]>, x: f64, y: f64) -> bool {
    match solid {
        None => true,
        Some(rects) => rects
            .iter()
            .any(|[l, t, w, h]| x >= *l && x < l + w && y >= *t && y < t + h),
    }
}

/// How often the pointer is looked at to let clicks through the empty part of the window.
const HIT_TICK: Duration = Duration::from_millis(40);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn internal(_: tauri::Error) -> AppError {
    AppError::internal("The pet window couldn't do that.")
}

fn window<R: Runtime>(app: &AppHandle<R>) -> Option<WebviewWindow<R>> {
    app.get_webview_window(LABEL)
}

fn outer_rect<R: Runtime>(w: &WebviewWindow<R>) -> tauri::Result<Rect> {
    let p = w.outer_position()?;
    let s = w.outer_size()?;
    Ok(Rect {
        x: p.x,
        y: p.y,
        w: s.width,
        h: s.height,
    })
}

fn work_area<R: Runtime>(w: &WebviewWindow<R>) -> Option<Rect> {
    let m = w.current_monitor().ok().flatten()?;
    let a = m.work_area();
    Some(Rect {
        x: a.position.x,
        y: a.position.y,
        w: a.size.width,
        h: a.size.height,
    })
}

// ---- commands ------------------------------------------------------------------------------

/// The page says which parts of the window are drawn (`rects`: left, top, width, height in CSS
/// pixels) and whether a drag is on. The window keeps its size; only these parts take the pointer.
#[tauri::command]
pub fn pet_solid(rects: Vec<[f64; 4]>, holding: bool, state: State<'_, Arc<PetState>>) {
    *lock(&state.solid) = Some(rects);
    state.holding.store(holding, Ordering::SeqCst);
}

/// Grow the window to show the card (`expanded: true`) or shrink it back. Returns where the pet
/// sits in the grown window. Card size is in logical pixels.
#[tauri::command]
pub async fn pet_hover(
    expanded: bool,
    card_width: f64,
    card_height: f64,
    below: Option<bool>,
    app: AppHandle,
    state: State<'_, Arc<PetState>>,
) -> Result<Option<Anchor>> {
    let Some(w) = window(&app) else {
        return Ok(None);
    };
    if expanded {
        let (pet, rect, anchor) = plan(&w, &state, card_width, card_height, below)?;
        *lock(&state.base) = Some(pet);
        *lock(&state.grown) = Some(rect);
        state.expanded.store(true, Ordering::SeqCst);
        set_bounds(&w, rect)?;
        Ok(Some(anchor))
    } else {
        if state.expanded.load(Ordering::SeqCst) {
            if let Some(base) = current_base(&w, &state) {
                let dragged = *lock(&state.base) != Some(base);
                set_bounds(&w, base)?;
                // The pet was dragged while its card was open: remember the new spot.
                if dragged {
                    if let Some(settings) = app.try_state::<Arc<SettingsService>>() {
                        let _ = settings
                            .set_pref(POSITION_PREF, json!({ "x": base.x, "y": base.y }))
                            .await;
                    }
                }
            }
            // Let the move events from shrinking pass before moves count as drags again.
            tokio::time::sleep(Duration::from_millis(150)).await;
            state.expanded.store(false, Ordering::SeqCst);
        }
        Ok(None)
    }
}

/// The pet's square, the grown window and where the pet sits in it.
fn plan<R: Runtime>(
    w: &WebviewWindow<R>,
    state: &PetState,
    card_width: f64,
    card_height: f64,
    below: Option<bool>,
) -> Result<(Rect, Rect, Anchor)> {
    let pet = match current_base(w, state) {
        Some(base) if state.expanded.load(Ordering::SeqCst) => base,
        _ => outer_rect(w).map_err(internal)?,
    };
    let scale = w.scale_factor().map_err(internal)?;
    let work = work_area(w).unwrap_or(pet);
    let (rect, anchor) = expanded_rect(
        pet,
        card_width,
        card_height,
        scale,
        work,
        below.unwrap_or(false),
    );
    Ok((pet, rect, anchor))
}

/// Move and resize in one step. Two steps (size, then position) showed the window for a moment at
/// the new size but the old place, so the pet visibly jumped when the cursor came near it
/// (owner 2026-10-27). Windows has a single call for both; elsewhere it is two calls.
fn set_bounds<R: Runtime>(w: &WebviewWindow<R>, r: Rect) -> Result<()> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOZORDER,
        };
        let hwnd = w.hwnd().map_err(internal)?;
        // SAFETY: the handle is this live window's; the flags keep its z-order and focus.
        let ok = unsafe {
            SetWindowPos(
                hwnd.0 as _,
                std::ptr::null_mut(),
                r.x,
                r.y,
                r.w as i32,
                r.h as i32,
                SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_NOACTIVATE,
            )
        };
        if ok != 0 {
            return Ok(());
        }
    }
    w.set_size(PhysicalSize::new(r.w, r.h)).map_err(internal)?;
    w.set_position(PhysicalPosition::new(r.x, r.y))
        .map_err(internal)
}

/// The pet's square while the window is grown, following any drag of the grown window.
fn current_base<R: Runtime>(w: &WebviewWindow<R>, state: &PetState) -> Option<Rect> {
    let base = (*lock(&state.base))?;
    match (*lock(&state.grown), w.outer_position()) {
        (Some(grown), Ok(p)) => Some(follow(base, grown, p.x, p.y)),
        _ => Some(base),
    }
}

fn expanded_rect(
    pet: Rect,
    card_w: f64,
    card_h: f64,
    scale: f64,
    work: Rect,
    prefer_below: bool,
) -> (Rect, Anchor) {
    expanded_with(
        pet,
        (card_w * scale).round() as u32,
        (card_h * scale).round() as u32,
        (f64::from(GAP) * scale).round() as u32,
        work,
        prefer_below,
    )
}

/// Single click: open the dashboard.
#[tauri::command]
pub fn pet_click(app: AppHandle) {
    crate::tray::show_main(&app);
}

/// A button on the hover card: open the dashboard and do one thing there (FR-111). Only the
/// commands the pet menu already sends are accepted.
#[tauri::command]
pub fn pet_command(what: String, app: AppHandle) {
    // `page:<name>` opens that page (a bubble's click, A-4); the dashboard checks the name.
    let page = what.strip_prefix("page:").is_some_and(|n| {
        !n.is_empty() && n.len() <= 20 && n.bytes().all(|b| b.is_ascii_lowercase())
    });
    if page || matches!(what.as_str(), "new-note" | "new-task" | "focus" | "tasks") {
        command(&app, &what);
    }
}

/// Double click: put the dashboard away.
#[tauri::command]
pub fn pet_double_click(app: AppHandle) {
    hide_main(&app);
}

pub fn hide_main<R: Runtime>(app: &AppHandle<R>) {
    crate::lock_diary(app);
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.hide();
    }
}

/// Right click: the pet's menu, at the cursor.
#[tauri::command]
pub async fn pet_menu(app: AppHandle, settings: State<'_, Arc<SettingsService>>) -> Result<()> {
    let all = settings.get_all().await?;
    let main_visible = app
        .get_webview_window("main")
        .and_then(|m| m.is_visible().ok())
        .unwrap_or(false);
    let Some(w) = window(&app) else { return Ok(()) };
    let menu = build_menu(&app, &menu_spec(main_visible, &all)).map_err(internal)?;
    w.popup_menu(&menu).map_err(internal)
}

/// One line of the pet's right-click menu.
#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    /// `id`, label, and the shortcut shown beside it (only when one is really registered).
    Item(&'static str, &'static str, Option<String>),
    /// A setting that is on or off: `id`, label, current value.
    Check(&'static str, &'static str, bool),
    Sub(&'static str, Vec<Entry>),
    Separator,
}

/// Dashboard pages the menu can open directly, with the keys that open them inside Pluto.
pub const PAGES: [(&str, &str, &str); 7] = [
    ("pet:page:today", "Today", "CmdOrCtrl+1"),
    ("pet:page:tasks", "Tasks", "CmdOrCtrl+2"),
    ("pet:page:notes", "Notes", "CmdOrCtrl+3"),
    ("pet:page:time", "Time", "CmdOrCtrl+4"),
    ("pet:page:meetings", "Meetings", "CmdOrCtrl+5"),
    ("pet:page:projects", "Projects", ""),
    ("pet:page:character", "Character", ""),
];

/// Settings worth one click from the pet, with their defaults.
pub const QUICK_SETTINGS: [(&str, &str, &str, bool); 5] = [
    (
        "pet:set:tracking.apps",
        "Track app time",
        "tracking.apps",
        false,
    ),
    (
        "pet:set:pet.always_on_top",
        "Keep the pet on top",
        "pet.always_on_top",
        true,
    ),
    (
        "pet:set:pet.hover_card",
        "Card when I hover the pet",
        "pet.hover_card",
        true,
    ),
    (
        "pet:set:pet.hide_fullscreen",
        "Get out of the way in full screen",
        "pet.hide_fullscreen",
        true,
    ),
    (
        "pet:set:pet.typing",
        "Type along with me",
        "pet.typing",
        false,
    ),
];

/// The menu, decided from the settings alone so it can be tested without a window (owner
/// 2026-10-27: direct navigation, important settings, the real shortcuts).
pub fn menu_spec(
    main_visible: bool,
    settings: &std::collections::BTreeMap<String, Value>,
) -> Vec<Entry> {
    let combos = crate::shortcuts::combos(settings);
    let key = |action: crate::shortcuts::Action| {
        combos
            .iter()
            .find(|(a, _)| *a == action)
            .map(|(_, c)| c.clone())
    };
    let open = if main_visible {
        ("pet:hide-main", "Hide Pluto")
    } else {
        ("pet:open", "Open Pluto")
    };
    let flag = |k: &str, default: bool| settings.get(k).and_then(Value::as_bool).unwrap_or(default);
    vec![
        Entry::Item(
            open.0,
            open.1,
            key(crate::shortcuts::Action::ToggleDashboard),
        ),
        Entry::Sub(
            "Go to",
            PAGES
                .iter()
                .map(|(id, label, keys)| {
                    Entry::Item(id, label, (!keys.is_empty()).then(|| (*keys).to_owned()))
                })
                .collect(),
        ),
        Entry::Separator,
        Entry::Item("pet:new-task", "New task", None),
        Entry::Item(
            "pet:new-note",
            "New note",
            key(crate::shortcuts::Action::NewNote),
        ),
        Entry::Item("pet:focus", "Focus…", None),
        Entry::Item("pet:nap", "Sleep or wake", None),
        Entry::Separator,
        Entry::Sub(
            "Quick settings",
            QUICK_SETTINGS
                .iter()
                .map(|(id, label, k, default)| Entry::Check(id, label, flag(k, *default)))
                .collect(),
        ),
        Entry::Item(
            "pet:settings",
            "All settings…",
            Some("CmdOrCtrl+,".to_owned()),
        ),
        Entry::Separator,
        Entry::Item(
            "pet:hide-pet",
            "Hide pet",
            key(crate::shortcuts::Action::TogglePet),
        ),
        Entry::Item("pet:quit", "Quit Pluto", None),
    ]
}

fn build_menu<R: Runtime>(app: &AppHandle<R>, spec: &[Entry]) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    for entry in spec {
        append(app, &menu, entry)?;
    }
    Ok(menu)
}

fn append<R: Runtime, M: MenuAppend<R>>(
    app: &AppHandle<R>,
    to: &M,
    entry: &Entry,
) -> tauri::Result<()> {
    match entry {
        Entry::Item(id, label, keys) => {
            // A shortcut the menu can't show (an unusual custom one) must not cost the whole menu.
            let item = MenuItem::with_id(app, *id, *label, true, keys.as_deref())
                .or_else(|_| MenuItem::with_id(app, *id, *label, true, None::<&str>))?;
            to.add(&item)
        }
        Entry::Check(id, label, on) => to.add(&CheckMenuItem::with_id(
            app,
            *id,
            *label,
            true,
            *on,
            None::<&str>,
        )?),
        Entry::Separator => to.add(&PredefinedMenuItem::separator(app)?),
        Entry::Sub(label, items) => {
            let sub = Submenu::new(app, *label, true)?;
            for item in items {
                append(app, &sub, item)?;
            }
            to.add(&sub)
        }
    }
}

/// `Menu` and `Submenu` both take items; this lets one function fill either.
trait MenuAppend<R: Runtime> {
    fn add(&self, item: &dyn tauri::menu::IsMenuItem<R>) -> tauri::Result<()>;
}
impl<R: Runtime> MenuAppend<R> for Menu<R> {
    fn add(&self, item: &dyn tauri::menu::IsMenuItem<R>) -> tauri::Result<()> {
        self.append(item)
    }
}
impl<R: Runtime> MenuAppend<R> for Submenu<R> {
    fn add(&self, item: &dyn tauri::menu::IsMenuItem<R>) -> tauri::Result<()> {
        self.append(item)
    }
}

/// Menu clicks from the pet menu and the tray's "Show or hide pet".
pub fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "pet:open" => crate::tray::show_main(app),
        "pet:hide-main" => hide_main(app),
        "pet:new-note" => command(app, "new-note"),
        "pet:new-task" => command(app, "new-task"),
        "pet:settings" => command(app, "settings"),
        "pet:focus" => command(app, "focus"),
        "pet:nap" => {
            let _ = app.emit_to(LABEL, "pet://menu", "nap");
        }
        id if id.starts_with("pet:page:") => command(app, &id["pet:".len()..]),
        id if id.starts_with("pet:set:") => {
            if let Some((_, _, key, default)) = QUICK_SETTINGS.iter().find(|(i, ..)| *i == id) {
                flip_setting(app, key, *default);
            }
        }
        "pet:hide-pet" => set_visible(app, false),
        crate::tray::ID_PET => toggle_pet(app),
        crate::tray::ID_PET_BACK => bring_back(app),
        "pet:quit" => crate::quit(app),
        _ => {}
    }
}

/// Bring the dashboard up and ask it to do something (`new-note`, `new-task`, `focus`, `tasks`,
/// `settings`).
pub fn command<R: Runtime>(app: &AppHandle<R>, what: &str) {
    crate::tray::show_main(app);
    let _ = app.emit_to("main", "pluto://command", what);
}

/// Set `pet.visible` and apply it now, even if the stored value was already that (the window
/// may have disappeared while the setting still said "on": B2/B4).
pub fn set_visible<R: Runtime>(app: &AppHandle<R>, visible: bool) {
    let Some(settings) = app
        .try_state::<Arc<SettingsService>>()
        .map(|s| Arc::clone(&s))
    else {
        return;
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = settings.set("pet.visible", json!(visible)).await {
            tracing::warn!(code = ?error.code, "couldn't change pet.visible");
        }
        apply(&app, &settings, false).await;
    });
}

/// Re-place the pet with the current settings (display changes).
pub fn reapply<R: Runtime>(app: &AppHandle<R>) {
    let Some(settings) = app
        .try_state::<Arc<SettingsService>>()
        .map(|s| Arc::clone(&s))
    else {
        return;
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move { apply(&app, &settings, false).await });
}

/// "Show or hide pet": the opposite of what is on screen right now (B4).
pub fn toggle_pet<R: Runtime>(app: &AppHandle<R>) {
    let on_screen = window(app)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    set_visible(app, toggle_target(on_screen));
}

/// "Bring the pet back": forget the saved spot, show it at the default place (B6).
pub fn bring_back<R: Runtime>(app: &AppHandle<R>) {
    let Some(settings) = app
        .try_state::<Arc<SettingsService>>()
        .map(|s| Arc::clone(&s))
    else {
        return;
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = settings.set_pref(POSITION_PREF, Value::Null).await;
        if let Err(error) = settings.set("pet.visible", json!(true)).await {
            tracing::warn!(code = ?error.code, "couldn't show the pet");
        }
        apply(&app, &settings, true).await;
    });
}

/// Flip a boolean setting; the bus listener applies it.
pub fn flip_setting<R: Runtime>(app: &AppHandle<R>, key: &'static str, default: bool) {
    let Some(settings) = app
        .try_state::<Arc<SettingsService>>()
        .map(|s| Arc::clone(&s))
    else {
        return;
    };
    tauri::async_runtime::spawn(async move {
        let current = settings
            .get_all()
            .await
            .ok()
            .and_then(|all| all.get(key).and_then(Value::as_bool))
            .unwrap_or(default);
        if let Err(error) = settings.set(key, json!(!current)).await {
            tracing::warn!(code = ?error.code, key, "couldn't change a setting from the pet");
        }
    });
}

// ---- placement, visibility, saving ---------------------------------------------------------

/// A drag ended somewhere: remember it (debounced, so a drag is one write).
pub fn on_moved<R: Runtime>(app: &AppHandle<R>, x: i32, y: i32) {
    let Some(state) = app.try_state::<Arc<PetState>>().map(|s| Arc::clone(&s)) else {
        return;
    };
    // Moves while the window is grown are saved too (as the pet's own spot, see `pet_spot`).
    if !state.expanded.load(Ordering::SeqCst) {
        *lock(&state.base) = None;
    }
    if state.save_pending.swap(true, Ordering::SeqCst) {
        return;
    }
    let (Some(settings), Some(w)) = (
        app.try_state::<Arc<SettingsService>>()
            .map(|s| Arc::clone(&s)),
        window(app),
    ) else {
        state.save_pending.store(false, Ordering::SeqCst);
        return;
    };
    let _ = (x, y);
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(600)).await;
        state.save_pending.store(false, Ordering::SeqCst);
        let Ok(p) = w.outer_position() else {
            return;
        };
        let spot = pet_spot(
            state.expanded.load(Ordering::SeqCst),
            *lock(&state.base),
            *lock(&state.grown),
            (p.x, p.y),
        );
        if lock(&state.saved).replace(spot) == Some(spot) {
            return;
        }
        let _ = settings
            .set_pref(POSITION_PREF, json!({ "x": spot.0, "y": spot.1 }))
            .await;
    });
}

async fn apply<R: Runtime>(app: &AppHandle<R>, settings: &SettingsService, reset_position: bool) {
    let Some(w) = window(app) else { return };
    let all = settings.get_all().await.unwrap_or_default();
    let visible = all
        .get("pet.visible")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let size_name = all
        .get("pet.size")
        .and_then(Value::as_str)
        .unwrap_or("M")
        .to_owned();
    let scale = w.scale_factor().unwrap_or(1.0);
    let size = (logical_size(&size_name) * scale).round() as u32;
    // Back to the pet's own square at its spot; the page lays the window out again after
    // `pet://placed`.
    if let Some(state) = app.try_state::<Arc<PetState>>() {
        state.expanded.store(false, Ordering::SeqCst);
        *lock(&state.base) = None;
        *lock(&state.grown) = None;
        *lock(&state.solid) = None;
    }
    warn_on("set_size", w.set_size(PhysicalSize::new(size, size)));
    // Character › "Always on top" (on by default).
    let on_top = all
        .get("pet.always_on_top")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    warn_on("set_always_on_top", w.set_always_on_top(on_top));

    let saved = if reset_position {
        None
    } else {
        settings.get_pref(POSITION_PREF).await.ok().flatten()
    };
    let monitors: Vec<Rect> = w
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let a = m.work_area();
            Rect {
                x: a.position.x,
                y: a.position.y,
                w: a.size.width,
                h: a.size.height,
            }
        })
        .collect();
    let spot = saved
        .and_then(|v| Some((v.get("x")?.as_i64()? as i32, v.get("y")?.as_i64()? as i32)))
        .filter(|&(x, y)| spot_fits((x, y), size, &monitors));
    let spot = spot.or_else(|| {
        let primary = w.primary_monitor().ok().flatten()?;
        let a = primary.work_area();
        Some(default_spot(
            size,
            Rect {
                x: a.position.x,
                y: a.position.y,
                w: a.size.width,
                h: a.size.height,
            },
        ))
    });
    if let Some((x, y)) = spot {
        warn_on("set_position", w.set_position(PhysicalPosition::new(x, y)));
    }
    let on_screen = pet_on_screen(
        visible,
        hide_fullscreen(&all),
        fullscreen::other_app_fills_a_screen(),
    );
    warn_on(
        if on_screen { "show" } else { "hide" },
        if on_screen { w.show() } else { w.hide() },
    );
    let _ = app.emit_to(LABEL, "pet://placed", ());
}

/// `pet.hide_fullscreen` (on by default): step aside while a game, film or presentation is on.
fn hide_fullscreen(all: &std::collections::BTreeMap<String, Value>) -> bool {
    all.get("pet.hide_fullscreen")
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

/// How often the screen is looked at while the pet is out. One cheap call per look; nothing runs
/// while the pet is hidden by its setting (ADR-009).
pub const FULLSCREEN_TICK: Duration = Duration::from_secs(2);

/// Is another app filling a screen? Each platform answers in its own module; anywhere else the
/// answer is "no" and the pet stays as it is.
pub mod fullscreen {
    /// Windows asks the shell the same question notifications do, so a game, an exclusive
    /// full-screen app, a presentation and a shared screen all count.
    #[cfg(windows)]
    pub fn other_app_fills_a_screen() -> bool {
        use windows_sys::Win32::UI::Shell::{
            SHQueryUserNotificationState, QUNS_BUSY, QUNS_PRESENTATION_MODE,
            QUNS_RUNNING_D3D_FULL_SCREEN,
        };
        let mut state = 0;
        // SAFETY: plain FFI call with a valid out pointer; it only reads shell state.
        if unsafe { SHQueryUserNotificationState(&mut state) } != 0 {
            return false;
        }
        matches!(
            state,
            QUNS_BUSY | QUNS_RUNNING_D3D_FULL_SCREEN | QUNS_PRESENTATION_MODE
        )
    }

    /// macOS has no such question, so look for an ordinary window that covers a whole screen.
    /// Pluto's own windows are skipped, and window titles are never read.
    #[cfg(target_os = "macos")]
    pub fn other_app_fills_a_screen() -> bool {
        use core_foundation::base::{CFType, TCFType};
        use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
        use core_foundation::number::CFNumber;
        use core_foundation::string::CFString;
        use core_graphics::display::CGDisplay;
        use core_graphics::window::{
            copy_window_info, kCGNullWindowID, kCGWindowBounds, kCGWindowLayer,
            kCGWindowListExcludeDesktopElements, kCGWindowListOptionOnScreenOnly,
            kCGWindowOwnerPID,
        };
        use pluto_core::pet_screen::covers_screen;

        let Some(list) = copy_window_info(
            kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
            kCGNullWindowID,
        ) else {
            return false;
        };
        let screens: Vec<(i32, i32, i32, i32)> = CGDisplay::active_displays()
            .unwrap_or_default()
            .iter()
            .map(|id| {
                let b = CGDisplay::new(*id).bounds();
                (
                    b.origin.x as i32,
                    b.origin.y as i32,
                    b.size.width as i32,
                    b.size.height as i32,
                )
            })
            .collect();
        if screens.is_empty() {
            return false;
        }
        let me = std::process::id() as i32;
        // SAFETY: these keys are constants exported by CoreGraphics for the whole process.
        let (bounds_key, layer_key, pid_key) = unsafe {
            (
                CFString::wrap_under_get_rule(kCGWindowBounds),
                CFString::wrap_under_get_rule(kCGWindowLayer),
                CFString::wrap_under_get_rule(kCGWindowOwnerPID),
            )
        };
        list.iter().any(|item| {
            // SAFETY: every element of the window list is a CFDictionary owned by the array.
            let dict = unsafe {
                CFDictionary::<CFString, CFType>::wrap_under_get_rule(*item as CFDictionaryRef)
            };
            let number = |key: &CFString| {
                dict.find(key)
                    .and_then(|v| v.downcast::<CFNumber>())
                    .and_then(|n| n.to_i32())
            };
            if number(&layer_key) != Some(0) || number(&pid_key) == Some(me) {
                return false;
            }
            let Some(bounds) = dict.find(&bounds_key) else {
                return false;
            };
            // CGRect arrives as a dictionary of X, Y, Width and Height.
            // SAFETY: the value under kCGWindowBounds is that dictionary, owned by `dict`.
            let bounds = unsafe {
                CFDictionary::<CFString, CFType>::wrap_under_get_rule(
                    bounds.as_CFTypeRef() as CFDictionaryRef
                )
            };
            let part = |name: &str| {
                bounds
                    .find(CFString::new(name))
                    .and_then(|v| v.downcast::<CFNumber>())
                    .and_then(|n| n.to_f64())
                    .unwrap_or(f64::NAN) as i32
            };
            let window = (part("X"), part("Y"), part("Width"), part("Height"));
            screens.iter().any(|s| covers_screen(window, *s))
        })
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    pub fn other_app_fills_a_screen() -> bool {
        false
    }
}

/// Window calls used to fail silently, which made "the pet is gone" impossible to diagnose (B5).
fn warn_on(what: &str, result: tauri::Result<()>) {
    if let Err(error) = result {
        tracing::warn!(%error, what, "pet window call failed");
    }
}

/// The screens' work areas, to notice a monitor being added, removed or rearranged.
fn monitor_layout<R: Runtime>(app: &AppHandle<R>) -> Vec<Rect> {
    window(app)
        .and_then(|w| w.available_monitors().ok())
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let a = m.work_area();
            Rect {
                x: a.position.x,
                y: a.position.y,
                w: a.size.width,
                h: a.size.height,
            }
        })
        .collect()
}

/// Place and show the pet now, and follow the pet settings from here on. Also watches for the
/// computer waking up and for screens changing (B2/S1): the OS gives no portable event for
/// either, so one look a minute (an exception to ADR-009, like `sites.rs`) re-applies the pet
/// and tells every window `pluto://wake` so reminders and the greeting can catch up.
pub fn start<R: Runtime>(app: AppHandle<R>, settings: Arc<SettingsService>, bus: &EventBus) {
    {
        // Clicks go through the window's empty part: the window stays the size of pet and card,
        // so showing the card never resizes it (no flicker), and only the drawn parts take the
        // pointer. Looked at every 40 ms while the pet is out; one call when it changes.
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let mut ignoring: Option<bool> = None;
            loop {
                tokio::time::sleep(HIT_TICK).await;
                let (Some(w), Some(state)) = (window(&app), app.try_state::<Arc<PetState>>())
                else {
                    continue;
                };
                if !w.is_visible().unwrap_or(false) {
                    continue;
                }
                let inside = state.holding.load(Ordering::SeqCst) || {
                    let solid = lock(&state.solid).clone();
                    match (app.cursor_position(), w.outer_position(), w.scale_factor()) {
                        (Ok(c), Ok(p), Ok(scale)) if scale > 0.0 => on_solid(
                            solid.as_deref(),
                            (c.x - f64::from(p.x)) / scale,
                            (c.y - f64::from(p.y)) / scale,
                        ),
                        _ => true,
                    }
                };
                if ignoring != Some(!inside) {
                    ignoring = Some(!inside);
                    warn_on(
                        "set_ignore_cursor_events",
                        w.set_ignore_cursor_events(!inside),
                    );
                }
            }
        });
    }
    {
        let (app, settings) = (app.clone(), Arc::clone(&settings));
        tauri::async_runtime::spawn(async move {
            let mut layout = monitor_layout(&app);
            loop {
                let before = std::time::SystemTime::now();
                tokio::time::sleep(WAKE_TICK).await;
                let wall = before.elapsed().unwrap_or_default();
                let now_layout = monitor_layout(&app);
                let woke = slept(WAKE_TICK, wall);
                if woke || now_layout != layout {
                    tracing::info!(
                        woke,
                        screens_changed = now_layout != layout,
                        "re-applying the pet"
                    );
                    layout = now_layout;
                    apply(&app, &settings, false).await;
                    if woke {
                        let _ = app.emit("pluto://wake", ());
                    }
                }
            }
        });
    }
    {
        // Step aside while something else fills the screen, and come back when it stops
        // (owner 2026-10-28). Only the window is shown or hidden; the setting is left alone, so
        // the pet returns by itself.
        let (app, settings) = (app.clone(), Arc::clone(&settings));
        tauri::async_runtime::spawn(async move {
            let mut was = false;
            loop {
                tokio::time::sleep(FULLSCREEN_TICK).await;
                let Some(w) = window(&app) else { continue };
                let all = settings.get_all().await.unwrap_or_default();
                let wanted = all
                    .get("pet.visible")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                if !wanted || !hide_fullscreen(&all) {
                    was = false;
                    continue;
                }
                let now = fullscreen::other_app_fills_a_screen();
                if now == was {
                    continue;
                }
                was = now;
                tracing::debug!(fullscreen = now, "another app filled the screen");
                warn_on(
                    if now { "hide" } else { "show" },
                    if now { w.hide() } else { w.show() },
                );
            }
        });
    }
    let mut events = bus.subscribe();
    tauri::async_runtime::spawn(async move {
        apply(&app, &settings, false).await;
        loop {
            match events.recv().await {
                Recv::Event(Event::SettingChanged { key, .. })
                    if key == "pet.visible"
                        || key == "pet.size"
                        || key == "pet.always_on_top"
                        || key == "pet.hide_fullscreen" =>
                {
                    apply(&app, &settings, false).await;
                }
                Recv::Event(Event::AllDataDeleted { .. }) => apply(&app, &settings, true).await,
                // Fell behind on events: one may have been a pet setting (B3).
                Recv::Resync { .. } => apply(&app, &settings, false).await,
                Recv::Event(_) => {}
                Recv::Closed => break,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: Rect = Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1040,
    };

    #[test]
    fn clicks_go_through_everywhere_but_the_drawn_parts() {
        let pet = [172.0, 412.0, 128.0, 128.0];
        let card = [0.0, 0.0, 300.0, 404.0];
        assert!(
            on_solid(None, 5.0, 5.0),
            "until the page says, the whole window"
        );
        assert!(on_solid(Some(&[pet]), 200.0, 450.0), "on the pet");
        assert!(
            !on_solid(Some(&[pet]), 50.0, 50.0),
            "where the closed card would be"
        );
        assert!(!on_solid(Some(&[pet]), 50.0, 450.0), "beside the pet");
        assert!(on_solid(Some(&[pet, card]), 50.0, 50.0), "on the open card");
        assert!(!on_solid(Some(&[]), 200.0, 450.0));
    }

    #[test]
    fn the_card_opens_up_and_left_when_the_pet_is_bottom_right() {
        let pet = Rect {
            x: 1768,
            y: 888,
            w: 128,
            h: 128,
        };
        let (r, a) = expanded(pet, 300, 170, WORK);
        assert_eq!(
            a,
            Anchor {
                pet: "right",
                card: "above"
            }
        );
        assert_eq!(
            r,
            Rect {
                x: 1768 + 128 - 300,
                y: 888 - 178,
                w: 300,
                h: 128 + 8 + 170
            }
        );
        // The pet's own square is where it was: bottom-right corner of the grown window.
        assert_eq!(
            (r.x + r.w as i32 - 128, r.y + r.h as i32 - 128),
            (pet.x, pet.y)
        );
    }

    #[test]
    fn the_card_flips_down_and_right_near_the_top_left_corner() {
        let pet = Rect {
            x: 10,
            y: 10,
            w: 128,
            h: 128,
        };
        let (r, a) = expanded(pet, 300, 170, WORK);
        assert_eq!(
            a,
            Anchor {
                pet: "left",
                card: "below"
            }
        );
        assert_eq!(
            (r.x, r.y),
            (10, 10),
            "the pet stays put at the top-left of the window"
        );
    }

    #[test]
    fn when_the_card_fits_neither_way_it_takes_the_side_with_more_room() {
        let short = Rect {
            x: 0,
            y: 0,
            w: 1280,
            h: 500,
        };
        // Pet a little below the middle of a short screen: 300 px above, 72 below, card 352.
        let pet = Rect {
            x: 600,
            y: 300,
            w: 128,
            h: 128,
        };
        let (r, a) = expanded(pet, 300, 352, short);
        assert_eq!(a.card, "above");
        assert_eq!(r.y + r.h as i32 - 128, pet.y, "the pet stays put");
        // Near the top instead: more room below.
        let pet = Rect { y: 60, ..pet };
        assert_eq!(expanded(pet, 300, 352, short).1.card, "below");
        // Narrow screen, pet in the middle: the wider side wins.
        let narrow = Rect {
            x: 0,
            y: 0,
            w: 400,
            h: 1040,
        };
        let pet = Rect {
            x: 230,
            y: 800,
            w: 128,
            h: 128,
        };
        assert_eq!(
            expanded(pet, 380, 100, narrow).1.pet,
            "right",
            "358 px to the left beats 170 to the right"
        );
    }

    #[test]
    fn a_panel_that_prefers_below_goes_below_when_it_fits_and_above_when_it_does_not() {
        // Bottom-right of a tall screen: the card goes above, the timer still goes below if
        // there is room under the pet.
        let pet = Rect {
            x: 1768,
            y: 700,
            w: 128,
            h: 128,
        };
        assert_eq!(expanded(pet, 184, 34, WORK).1.card, "above");
        assert_eq!(expanded_with(pet, 184, 34, GAP, WORK, true).1.card, "below");
        // No room under the pet: it falls back to above rather than being cut off.
        let low = Rect { y: 912, ..pet };
        assert_eq!(expanded_with(low, 184, 34, GAP, WORK, true).1.card, "above");
    }

    #[test]
    fn the_gap_scales_with_the_screen_so_the_pet_keeps_its_place() {
        // 150%: the page's 8 px gap is 12 physical pixels. The window must hold pet + gap + card
        // exactly, and the pet's square must not move.
        let pet = Rect {
            x: 1700,
            y: 900,
            w: 192,
            h: 192,
        };
        let (r, a) = expanded_rect(pet, 300.0, 352.0, 1.5, WORK, false);
        assert_eq!((a.pet, a.card), ("right", "above"));
        assert_eq!(r.h, 192 + 12 + 528);
        assert_eq!(
            (r.x + r.w as i32, r.y + r.h as i32),
            (pet.x + pet.w as i32, pet.y + pet.h as i32)
        );
    }

    #[test]
    fn a_card_narrower_than_the_pet_does_not_shrink_the_window() {
        let pet = Rect {
            x: 800,
            y: 600,
            w: 160,
            h: 160,
        };
        let (r, _) = expanded(pet, 100, 50, WORK);
        assert_eq!(r.w, 160);
    }

    #[test]
    fn a_dragged_grown_window_carries_the_pet_with_it() {
        let base = Rect {
            x: 1000,
            y: 800,
            w: 128,
            h: 128,
        };
        let grown = Rect {
            x: 828,
            y: 500,
            w: 300,
            h: 428,
        };
        assert_eq!(follow(base, grown, 828, 500), base);
        assert_eq!(
            follow(base, grown, 700, 450),
            Rect {
                x: 872,
                y: 750,
                w: 128,
                h: 128
            }
        );
    }

    #[test]
    fn the_saved_spot_is_the_pets_not_the_grown_windows_corner() {
        let base = Rect {
            x: 1000,
            y: 800,
            w: 128,
            h: 128,
        };
        let grown = Rect {
            x: 828,
            y: 500,
            w: 300,
            h: 428,
        };
        // Not grown: the window is the pet.
        assert_eq!(pet_spot(false, None, None, (640, 480)), (640, 480));
        // Grown and not moved: still where the pet was set, not the window's corner.
        assert_eq!(
            pet_spot(true, Some(base), Some(grown), (828, 500)),
            (1000, 800)
        );
        // Grown and dragged 128 left and 50 up: the pet moved by the same amount.
        assert_eq!(
            pet_spot(true, Some(base), Some(grown), (700, 450)),
            (872, 750)
        );
        // Grown flag set but no base yet (between steps): fall back to the window.
        assert_eq!(pet_spot(true, None, None, (10, 20)), (10, 20));
    }

    fn find<'a>(spec: &'a [Entry], id: &str) -> Option<&'a Entry> {
        spec.iter().find_map(|e| match e {
            Entry::Item(i, ..) | Entry::Check(i, ..) if *i == id => Some(e),
            Entry::Sub(_, items) => find(items, id),
            _ => None,
        })
    }

    #[test]
    fn the_menu_opens_every_page_and_flips_the_important_settings() {
        let spec = menu_spec(false, &std::collections::BTreeMap::new());
        for (id, ..) in PAGES {
            assert!(find(&spec, id).is_some(), "{id}");
        }
        for (id, ..) in [
            "pet:new-task",
            "pet:new-note",
            "pet:focus",
            "pet:nap",
            "pet:settings",
            "pet:hide-pet",
            "pet:quit",
        ]
        .map(|i| (i, ()))
        {
            assert!(find(&spec, id).is_some(), "{id}");
        }
        // Each quick setting shows its default until it is changed.
        assert_eq!(
            find(&spec, "pet:set:pet.always_on_top"),
            Some(&Entry::Check(
                "pet:set:pet.always_on_top",
                "Keep the pet on top",
                true
            ))
        );
        assert_eq!(
            find(&spec, "pet:set:tracking.apps"),
            Some(&Entry::Check(
                "pet:set:tracking.apps",
                "Track app time",
                false
            ))
        );
        let on = std::collections::BTreeMap::from([(
            "tracking.apps".to_owned(),
            serde_json::json!(true),
        )]);
        assert_eq!(
            find(&menu_spec(false, &on), "pet:set:tracking.apps"),
            Some(&Entry::Check(
                "pet:set:tracking.apps",
                "Track app time",
                true
            ))
        );
    }

    #[test]
    fn the_menu_shows_the_shortcuts_that_are_really_set() {
        // Defaults: open and new note have keys; the pet's own toggle is off, so no key is shown.
        let spec = menu_spec(true, &std::collections::BTreeMap::new());
        assert_eq!(
            find(&spec, "pet:hide-main"),
            Some(&Entry::Item(
                "pet:hide-main",
                "Hide Pluto",
                Some("CmdOrCtrl+Shift+L".into())
            ))
        );
        assert_eq!(
            find(&spec, "pet:new-note"),
            Some(&Entry::Item(
                "pet:new-note",
                "New note",
                Some("CmdOrCtrl+Shift+N".into())
            ))
        );
        assert_eq!(
            find(&spec, "pet:hide-pet"),
            Some(&Entry::Item("pet:hide-pet", "Hide pet", None))
        );
        // A changed shortcut is shown as changed (the old menu always said Ctrl+Shift+L).
        let custom = std::collections::BTreeMap::from([(
            "shortcuts.global_open".to_owned(),
            serde_json::json!("Alt+F9"),
        )]);
        assert_eq!(
            find(&menu_spec(false, &custom), "pet:open"),
            Some(&Entry::Item(
                "pet:open",
                "Open Pluto",
                Some("Alt+F9".into())
            ))
        );
    }

    #[test]
    fn default_spot_is_bottom_right_with_a_margin() {
        assert_eq!(default_spot(128, WORK), (1920 - 128 - 24, 1040 - 128 - 24));
        let second = Rect {
            x: 1920,
            y: 0,
            w: 1280,
            h: 1000,
        };
        assert_eq!(
            default_spot(96, second),
            (1920 + 1280 - 96 - 24, 1000 - 96 - 24)
        );
    }

    #[test]
    fn sizes_follow_the_setting_with_medium_as_the_fallback() {
        assert_eq!(logical_size("S"), 96.0);
        assert_eq!(logical_size("M"), 128.0);
        assert_eq!(logical_size("L"), 160.0);
        assert_eq!(logical_size("XL"), 128.0);
    }

    #[test]
    fn rect_contains_is_half_open() {
        assert!(WORK.contains(0, 0));
        assert!(!WORK.contains(1920, 0));
        assert!(!WORK.contains(-1, 5));
    }

    #[test]
    fn a_saved_spot_needs_the_pets_centre_on_a_screen() {
        let screen = [Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1040,
        }];
        assert!(spot_fits((1700, 900), 128, &screen));
        // Top-left corner on screen but most of the pet hanging off the right edge.
        assert!(!spot_fits((1900, 500), 128, &screen));
        assert!(!spot_fits((-500, 0), 128, &[]));
    }

    #[test]
    fn show_or_hide_follows_what_is_on_screen() {
        assert!(toggle_target(false)); // invisible (even if the setting says on) -> show
        assert!(!toggle_target(true));
    }

    #[test]
    fn a_long_gap_between_ticks_means_the_computer_slept() {
        assert!(!slept(WAKE_TICK, Duration::from_secs(61)));
        assert!(!slept(WAKE_TICK, Duration::from_secs(119)));
        assert!(slept(WAKE_TICK, Duration::from_secs(600)));
    }
}
