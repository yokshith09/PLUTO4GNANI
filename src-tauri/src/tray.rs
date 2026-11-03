//! Tray icon, menu registry and window show/hide (F0-08).
//!
//! The menu is built from a registry: Open and Quit ship now; later features register
//! their own entries, and unbuilt actions are hidden, not greyed (PRD R1-84).

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

pub const ID_OPEN: &str = "open";
pub const ID_QUIT: &str = "quit";
/// Handled by the pet module (`pet::on_menu`), like the pet's own menu.
pub const ID_PET: &str = "pet:toggle";
/// Puts the pet back at the bottom-right of the main screen and shows it (bug B6).
pub const ID_PET_BACK: &str = "pet:back";

/// Where an entry sits. Groups are separated by a single separator. `Create`, `Pet` and
/// `Settings` are claimed by F1-01, F1-22 and F1-25 when those land.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    Open,
    Create,
    Pet,
    Settings,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: &'static str,
    pub label: String,
    pub group: Group,
    pub order: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Slot {
    Item(Entry),
    Separator,
}

#[derive(Default)]
pub struct MenuRegistry {
    entries: Vec<Entry>,
}

impl MenuRegistry {
    /// The entries that exist today (F0-08).
    pub fn base() -> Self {
        let mut r = Self::default();
        r.register(Entry {
            id: ID_OPEN,
            label: "Open Pluto".into(),
            group: Group::Open,
            order: 0,
        });
        r.register(Entry {
            id: ID_PET,
            label: "Show or hide pet".into(),
            group: Group::Pet,
            order: 0,
        });
        r.register(Entry {
            id: ID_PET_BACK,
            label: "Bring the pet back".into(),
            group: Group::Pet,
            order: 1,
        });
        r.register(Entry {
            id: ID_QUIT,
            label: "Quit Pluto".into(),
            group: Group::Quit,
            order: 0,
        });
        r
    }

    /// Add or replace (by id) an entry.
    pub fn register(&mut self, entry: Entry) {
        self.entries.retain(|e| e.id != entry.id);
        self.entries.push(entry);
    }

    /// Entries sorted by group then order, with one separator between non-empty groups
    /// and none at the start or end.
    pub fn slots(&self) -> Vec<Slot> {
        let mut sorted = self.entries.clone();
        sorted.sort_by_key(|e| (e.group, e.order));
        let mut out = Vec::with_capacity(sorted.len() * 2);
        let mut last: Option<Group> = None;
        for e in sorted {
            if last.is_some_and(|g| g != e.group) {
                out.push(Slot::Separator);
            }
            last = Some(e.group);
            out.push(Slot::Item(e));
        }
        out
    }
}

fn build_menu<R: Runtime>(app: &AppHandle<R>, registry: &MenuRegistry) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    for slot in registry.slots() {
        match slot {
            Slot::Item(e) => {
                menu.append(&MenuItem::with_id(app, e.id, e.label, true, None::<&str>)?)?
            }
            Slot::Separator => menu.append(&PredefinedMenuItem::separator(app)?)?,
        }
    }
    Ok(menu)
}

/// Create the tray icon. Windows: left-click opens Pluto, right-click shows the menu.
/// macOS: click shows the menu.
pub fn setup_tray<R: Runtime>(
    app: &AppHandle<R>,
    registry: &MenuRegistry,
    on_quit: impl Fn(&AppHandle<R>) + Send + Sync + 'static,
) -> tauri::Result<()> {
    let menu = build_menu(app, registry)?;
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("Pluto")
        .menu(&menu)
        .show_menu_on_left_click(cfg!(target_os = "macos"))
        .on_menu_event(move |app, event| match event.id.as_ref() {
            ID_OPEN => show_main(app),
            ID_QUIT => on_quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if cfg!(not(target_os = "macos")) {
                    show_main(tray.app_handle());
                }
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

/// Show, unminimize and focus the main window.
pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &'static str, group: Group, order: u8) -> Entry {
        Entry {
            id,
            label: id.into(),
            group,
            order,
        }
    }

    fn ids(slots: &[Slot]) -> Vec<&str> {
        slots
            .iter()
            .map(|s| match s {
                Slot::Item(e) => e.id,
                Slot::Separator => "-",
            })
            .collect()
    }

    #[test]
    fn base_menu_is_open_pet_quit_with_separators() {
        assert_eq!(
            ids(&MenuRegistry::base().slots()),
            ["open", "-", ID_PET, ID_PET_BACK, "-", "quit"]
        );
    }

    #[test]
    fn registered_items_are_ordered_by_group_then_order() {
        let mut r = MenuRegistry::base();
        r.register(entry("new_task", Group::Create, 1));
        r.register(entry("new_note", Group::Create, 0));
        r.register(entry("settings", Group::Settings, 0));
        r.register(entry("pet", Group::Pet, 0));
        assert_eq!(
            ids(&r.slots()),
            [
                "open",
                "-",
                "new_note",
                "new_task",
                "-",
                ID_PET,
                "pet",
                ID_PET_BACK,
                "-",
                "settings",
                "-",
                "quit"
            ]
        );
    }

    #[test]
    fn unregistered_groups_leave_no_stray_separators() {
        let mut r = MenuRegistry::default();
        r.register(entry("quit", Group::Quit, 0));
        assert_eq!(ids(&r.slots()), ["quit"]);
        assert!(MenuRegistry::default().slots().is_empty());
    }

    #[test]
    fn registering_the_same_id_replaces_it() {
        let mut r = MenuRegistry::base();
        r.register(Entry {
            label: "Quit now".into(),
            ..entry("quit", Group::Quit, 0)
        });
        let slots = r.slots();
        assert_eq!(slots.len(), 6);
        assert!(matches!(&slots[5], Slot::Item(e) if e.label == "Quit now"));
    }
}
