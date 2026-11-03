//! Pet library (FR-63): import Codex / OpenPets pet packs and list them. Thin: the checks are in
//! `pluto_core::pets`. Pluto's own copies live in `<data>/pets`; packs installed by Codex or the
//! Petdex CLI are read in place from `~/.codex/pets` and `~/.petdex/pets`. Unverified on a real Windows or Mac PC.

use std::path::PathBuf;

use base64::Engine;
use pluto_core::error::{AppError, Result};
use pluto_core::pets::{self, PetPack, PetSource};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::ipc::AppPaths;

fn pluto_dir(paths: &AppPaths) -> PathBuf {
    paths.data_dir.join("pets")
}

/// Folders other tools install pets into: Codex (`~/.codex/pets`) and the Petdex CLI
/// (`~/.petdex/pets`). Read in place, never changed.
fn shared_dirs(app: &AppHandle) -> Vec<PathBuf> {
    let Ok(home) = app.path().home_dir() else {
        return Vec::new();
    };
    vec![
        home.join(".codex").join("pets"),
        home.join(".petdex").join("pets"),
    ]
}

/// The characters shipped with Pluto (`bundle.resources` in tauri.conf.json), read in place.
fn builtin_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().resource_dir().ok().map(|d| d.join("packs"))
}

/// Every pack: Pluto's own first, then the built-in ones, then Codex's and Petdex's (the first
/// one found wins on the same id).
fn all(app: &AppHandle, paths: &AppPaths) -> Vec<(PetPack, PathBuf)> {
    let mut found = pets::scan(&pluto_dir(paths), PetSource::Pluto);
    if let Some(dir) = builtin_dir(app) {
        for (pack, sheet) in pets::scan(&dir, PetSource::Builtin) {
            if !found.iter().any(|(p, _)| p.id == pack.id) {
                found.push((pack, sheet));
            }
        }
    }
    for dir in shared_dirs(app) {
        for (pack, sheet) in pets::scan(&dir, PetSource::Codex) {
            if !found.iter().any(|(p, _)| p.id == pack.id) {
                found.push((pack, sheet));
            }
        }
    }
    found
}

#[tauri::command]
pub async fn pets_list(app: AppHandle, paths: State<'_, AppPaths>) -> Result<Vec<PetPack>> {
    let paths = paths.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        all(&app, &paths).into_iter().map(|(p, _)| p).collect()
    })
    .await
    .map_err(|_| AppError::internal("Reading the pet library stopped unexpectedly."))
}

/// Pick a pack folder and copy it into Pluto. None if the person cancels.
#[tauri::command]
pub async fn pets_import(app: AppHandle, paths: State<'_, AppPaths>) -> Result<Option<PetPack>> {
    let picker = app.clone();
    let chosen =
        tauri::async_runtime::spawn_blocking(move || picker.dialog().file().blocking_pick_folder())
            .await
            .map_err(|_| AppError::internal("The folder picker stopped unexpectedly."))?;
    let Some(chosen) = chosen else {
        return Ok(None);
    };
    let src = chosen
        .into_path()
        .map_err(|_| AppError::io("Pluto can't open that folder."))?;
    let dest = pluto_dir(paths.inner());
    tauri::async_runtime::spawn_blocking(move || pets::install(&src, &dest).map(Some))
        .await
        .map_err(|_| AppError::internal("Importing the pet stopped unexpectedly."))?
}

/// Pick a pet ZIP (as downloaded from a gallery) and install it. None if the person cancels.
#[tauri::command]
pub async fn pets_import_zip(
    app: AppHandle,
    paths: State<'_, AppPaths>,
) -> Result<Option<PetPack>> {
    let picker = app.clone();
    let chosen = tauri::async_runtime::spawn_blocking(move || {
        picker
            .dialog()
            .file()
            .add_filter("Pet pack", &["zip"])
            .blocking_pick_file()
    })
    .await
    .map_err(|_| AppError::internal("The file picker stopped unexpectedly."))?;
    let Some(chosen) = chosen else {
        return Ok(None);
    };
    let zip = chosen
        .into_path()
        .map_err(|_| AppError::io("Pluto can't open that file."))?;
    let dest = pluto_dir(paths.inner());
    tauri::async_runtime::spawn_blocking(move || {
        std::fs::create_dir_all(&dest).map_err(|e| AppError::io(e.to_string()))?;
        pets::install_zip(&zip, &dest).map(Some)
    })
    .await
    .map_err(|_| AppError::internal("Importing the pet stopped unexpectedly."))?
}

/// Remove a pack Pluto copied in. Codex's own packs are never deleted from here.
#[tauri::command]
pub async fn pets_remove(id: String, paths: State<'_, AppPaths>) -> Result<()> {
    if !pets::id_is_valid(&id) {
        return Err(AppError::validation("id", "not a pet id"));
    }
    let dir = pluto_dir(paths.inner()).join(id);
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|e| AppError::io(e.to_string()))?;
    }
    Ok(())
}

/// The pack's spritesheet as a data URL, for the pet window and the preview.
#[tauri::command]
pub async fn pets_sheet(id: String, app: AppHandle, paths: State<'_, AppPaths>) -> Result<String> {
    let paths = paths.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (pack, sheet) = all(&app, &paths)
            .into_iter()
            .find(|(p, _)| p.id == id)
            .ok_or_else(|| AppError::not_found("That pet is not installed."))?;
        let bytes = std::fs::read(sheet).map_err(|e| AppError::io(e.to_string()))?;
        let mime = if pack.format == "svg" {
            "image/svg+xml"
        } else {
            "image/webp"
        };
        Ok(format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ))
    })
    .await
    .map_err(|_| AppError::internal("Reading the pet stopped unexpectedly."))?
}
