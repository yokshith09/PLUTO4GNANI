//! Carrying data over from the folder an earlier build used, before the app had its present name.
//!
//! A new app name means a new data folder (the operating system derives it from the app's
//! identifier), so without this a person who updated would open an empty app while their tasks,
//! notes, diary and time history sat untouched in the old folder. [`carry_over`] copies them in
//! once, the first time the new folder has no database. It only ever copies: the old folder is
//! left exactly as it was, so nothing can be lost by it.
//!
//! The earlier names appear in this file and in the Windows installer hook
//! (`src-tauri/windows/hooks.nsh`), and nowhere else; they are what the old files are called.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The identifier an earlier build used; also the name of its data folder.
pub const OLD_APP_ID: &str = "app.loaf.desktop";
const OLD_DB: &str = "loaf.db";
const NEW_DB: &str = "pluto.db";
const OLD_PACK_EXTRAS: &str = "loaf.json";
const NEW_PACK_EXTRAS: &str = "pluto.json";

/// What an earlier build left installed on a Mac: its app bundle and its login item.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct EarlierMacApp {
    /// `Loaf.app` in each applications folder where it was found.
    pub bundles: Vec<PathBuf>,
    /// Login items (launch agents) that start it at login.
    pub login_items: Vec<PathBuf>,
}

impl EarlierMacApp {
    pub fn is_empty(&self) -> bool {
        self.bundles.is_empty() && self.login_items.is_empty()
    }
}

/// The name of the earlier program's process, to stop it before it is removed.
pub const OLD_PROCESS_NAME: &str = "loaf";
const OLD_BUNDLE: &str = "Loaf.app";
const OLD_LOGIN_ITEMS: [&str; 2] = ["Loaf.plist", "app.loaf.desktop.plist"];

/// Look for an earlier build in the given applications folders and launch-agent folder. Only
/// existing paths are returned; nothing is touched.
pub fn earlier_mac_app(applications: &[PathBuf], launch_agents: &Path) -> EarlierMacApp {
    EarlierMacApp {
        bundles: applications
            .iter()
            .map(|dir| dir.join(OLD_BUNDLE))
            .filter(|p| p.is_dir())
            .collect(),
        login_items: OLD_LOGIN_ITEMS
            .iter()
            .map(|name| launch_agents.join(name))
            .filter(|p| p.is_file())
            .collect(),
    }
}

/// What was copied.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CarriedOver {
    /// Files copied beside the database (the database itself, its backups and journal).
    pub database_files: usize,
    /// Characters copied from the `pets` folder.
    pub pet_packs: usize,
}

/// The earlier build's data folder, next to `new_dir` (the same parent, the old identifier).
pub fn old_dir_beside(new_dir: &Path) -> Option<PathBuf> {
    Some(new_dir.parent()?.join(OLD_APP_ID))
}

/// Copy the database (with its numbered backups and any write-ahead journal) and the installed
/// characters from `old_dir` into `new_dir`, unless `new_dir` already has a database or `old_dir`
/// has none. Returns what was copied; `Ok(default)` when there was nothing to do.
pub fn carry_over(old_dir: &Path, new_dir: &Path) -> io::Result<CarriedOver> {
    let mut done = CarriedOver::default();
    if new_dir.join(NEW_DB).exists() || !old_dir.join(OLD_DB).is_file() {
        return Ok(done);
    }
    fs::create_dir_all(new_dir)?;

    // Copy to a temporary name first and rename at the end, so an interrupted copy never leaves a
    // half-written database that would be taken for the real one.
    let mut copied = Vec::new();
    for entry in fs::read_dir(old_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(rest) = name.strip_prefix(OLD_DB) else {
            continue; // "<db>", "<db>-wal", "<db>-shm", "<db>.bak-N"
        };
        if !entry.file_type()?.is_file() {
            continue;
        }
        let target = new_dir.join(format!("{NEW_DB}{rest}"));
        let temp = new_dir.join(format!("{NEW_DB}{rest}.copying"));
        fs::copy(entry.path(), &temp)?;
        copied.push((temp, target));
    }
    // The database file itself goes last: its presence is what marks the carry-over as complete.
    copied.sort_by_key(|(_, target)| target.file_name().is_some_and(|n| n == NEW_DB));
    for (temp, target) in copied {
        fs::rename(temp, target)?;
        done.database_files += 1;
    }

    done.pet_packs = copy_pets(&old_dir.join("pets"), &new_dir.join("pets"))?;
    Ok(done)
}

fn copy_pets(from: &Path, to: &Path) -> io::Result<usize> {
    if !from.is_dir() {
        return Ok(0);
    }
    let mut packs = 0;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let dest = to.join(entry.file_name());
        if dest.exists() {
            continue;
        }
        fs::create_dir_all(&dest)?;
        for file in fs::read_dir(entry.path())? {
            let file = file?;
            if !file.file_type()?.is_file() {
                continue;
            }
            let name = file.file_name();
            let name = if name == OLD_PACK_EXTRAS {
                NEW_PACK_EXTRAS.into()
            } else {
                name
            };
            fs::copy(file.path(), dest.join(name))?;
        }
        packs += 1;
    }
    Ok(packs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    #[test]
    fn the_old_folder_is_found_beside_the_new_one() {
        let new = Path::new("/data/app.pluto.desktop");
        assert_eq!(
            old_dir_beside(new),
            Some(PathBuf::from("/data/app.loaf.desktop"))
        );
        assert_eq!(old_dir_beside(Path::new("/")), None);
    }

    #[test]
    fn the_database_its_backups_and_journal_move_over_under_the_new_names() {
        let tmp = tempfile::tempdir().unwrap();
        let (old, new) = (tmp.path().join("old"), tmp.path().join("new"));
        write(&old.join("loaf.db"), "main");
        write(&old.join("loaf.db-wal"), "wal");
        write(&old.join("loaf.db.bak-6"), "bak6");
        write(&old.join("notes.txt"), "not ours");

        let done = carry_over(&old, &new).unwrap();

        assert_eq!(done.database_files, 3);
        assert_eq!(read(&new.join("pluto.db")), "main");
        assert_eq!(read(&new.join("pluto.db-wal")), "wal");
        assert_eq!(read(&new.join("pluto.db.bak-6")), "bak6");
        assert!(!new.join("notes.txt").exists(), "only our files");
        assert!(
            !fs::read_dir(&new).unwrap().any(|e| e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".copying")),
            "no temporary files are left behind"
        );
    }

    #[test]
    fn the_old_folder_is_left_exactly_as_it_was() {
        let tmp = tempfile::tempdir().unwrap();
        let (old, new) = (tmp.path().join("old"), tmp.path().join("new"));
        write(&old.join("loaf.db"), "main");
        write(&old.join("pets/ghosty/pet.json"), "{}");
        carry_over(&old, &new).unwrap();
        assert_eq!(read(&old.join("loaf.db")), "main");
        assert_eq!(read(&old.join("pets/ghosty/pet.json")), "{}");
    }

    #[test]
    fn nothing_is_copied_over_a_database_that_is_already_there() {
        let tmp = tempfile::tempdir().unwrap();
        let (old, new) = (tmp.path().join("old"), tmp.path().join("new"));
        write(&old.join("loaf.db"), "old");
        write(&new.join("pluto.db"), "current");
        let done = carry_over(&old, &new).unwrap();
        assert_eq!(done, CarriedOver::default());
        assert_eq!(read(&new.join("pluto.db")), "current");
    }

    #[test]
    fn nothing_happens_when_there_is_no_old_data() {
        let tmp = tempfile::tempdir().unwrap();
        let (old, new) = (tmp.path().join("old"), tmp.path().join("new"));
        assert_eq!(carry_over(&old, &new).unwrap(), CarriedOver::default());
        assert!(!new.exists(), "a clean install is not touched");
    }

    #[test]
    fn installed_characters_come_along_and_their_extras_file_is_renamed() {
        let tmp = tempfile::tempdir().unwrap();
        let (old, new) = (tmp.path().join("old"), tmp.path().join("new"));
        write(&old.join("loaf.db"), "main");
        write(&old.join("pets/mira/pet.json"), "{\"id\":\"mira\"}");
        write(&old.join("pets/mira/spritesheet.webp"), "img");
        write(&old.join("pets/mira/loaf.json"), "{\"reactions\":{}}");
        write(&old.join("pets/raja/pet.json"), "{\"id\":\"raja\"}");

        let done = carry_over(&old, &new).unwrap();

        assert_eq!(done.pet_packs, 2);
        assert_eq!(read(&new.join("pets/mira/pet.json")), "{\"id\":\"mira\"}");
        assert_eq!(
            read(&new.join("pets/mira/pluto.json")),
            "{\"reactions\":{}}"
        );
        assert!(!new.join("pets/mira/loaf.json").exists());
        assert!(new.join("pets/raja/pet.json").is_file());
    }

    #[test]
    fn a_second_run_copies_nothing_more() {
        let tmp = tempfile::tempdir().unwrap();
        let (old, new) = (tmp.path().join("old"), tmp.path().join("new"));
        write(&old.join("loaf.db"), "main");
        assert_eq!(carry_over(&old, &new).unwrap().database_files, 1);
        assert_eq!(carry_over(&old, &new).unwrap(), CarriedOver::default());
    }

    #[test]
    fn an_earlier_mac_install_is_found_in_either_applications_folder_and_its_login_item_too() {
        let tmp = tempfile::tempdir().unwrap();
        let (system, user, agents) = (
            tmp.path().join("Applications"),
            tmp.path().join("home/Applications"),
            tmp.path().join("LaunchAgents"),
        );
        fs::create_dir_all(system.join("Loaf.app/Contents")).unwrap();
        fs::create_dir_all(user.join("Loaf.app")).unwrap();
        write(&agents.join("Loaf.plist"), "<plist/>");
        write(&agents.join("Other.plist"), "<plist/>");

        let found = earlier_mac_app(&[system.clone(), user.clone()], &agents);

        assert_eq!(
            found.bundles,
            vec![system.join("Loaf.app"), user.join("Loaf.app")]
        );
        assert_eq!(found.login_items, vec![agents.join("Loaf.plist")]);
        assert!(!found.is_empty());
    }

    #[test]
    fn nothing_is_reported_when_no_earlier_mac_install_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let found = earlier_mac_app(
            &[tmp.path().join("Applications")],
            &tmp.path().join("LaunchAgents"),
        );
        assert!(found.is_empty());
        // a file called Loaf.app is not an app bundle
        write(&tmp.path().join("Applications/Loaf.app"), "x");
        assert!(earlier_mac_app(
            &[tmp.path().join("Applications")],
            &tmp.path().join("LaunchAgents")
        )
        .bundles
        .is_empty());
    }
}
