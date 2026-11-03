//! Pet packs in the Codex / OpenPets format (FR-63): a folder with `pet.json` and one
//! `spritesheet.webp` (or `.svg`), 8 columns of 192x208 cells, 9 rows (V1) or 11 (V2).
//!
//! Packs are art only: nothing in them is ever run. This module checks a folder, copies a good
//! one into Pluto's own pets folder, and lists what is installed. A pack can also come as a ZIP
//! (`install_zip`): only `pet.json` and the spritesheet are taken out of it, safely.

use crate::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const CELL_W: u32 = 192;
pub const CELL_H: u32 = 208;
pub const COLUMNS: u32 = 8;
/// Largest spritesheet we accept (OpenPets caps a download at 25 MB).
pub const SHEET_MAX_BYTES: u64 = 25 * 1024 * 1024;

/// Where an installed pet came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum PetSource {
    /// Copied into Pluto's pets folder.
    Pluto,
    /// Read in place from `~/.codex/pets` (Codex / Petdex installs).
    Codex,
    /// Shipped with Pluto (owner 2026-10-25: characters can be changed). Never removed.
    Builtin,
}

/// One checked pet pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PetPack {
    /// The pack's own default pose for some moments (`pluto.json`), keyed by reaction key.
    /// The user's choice on the Character page still wins over these.
    #[serde(default)]
    pub reactions: std::collections::BTreeMap<String, String>,
    /// The pack's one special reaction (`pluto.json`), if it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub special: Option<PetSpecial>,
    pub id: String,
    pub display_name: String,
    pub description: String,
    /// 9 rows (V1) or 11 rows (V2, with the cursor-gaze rows).
    pub rows: u32,
    /// "webp" or "svg".
    pub format: String,
    pub source: PetSource,
}

/// What sets a pack's special reaction off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "kebab-case")]
pub enum SpecialTrigger {
    /// Every few clicks on the pet.
    Click,
    /// Every `every` tasks finished.
    Tasks,
    /// The first time the pet is seen each day, in place of the plain hello.
    Daily,
}

/// One signature move per character: a pose from the shared nine rows plus its own line.
/// (The sheet stays at nine rows, so a special reuses a row; it is the pose, words and
/// trigger that make it the character's own.)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PetSpecial {
    pub name: String,
    pub pose: String,
    pub line: String,
    pub trigger: SpecialTrigger,
    /// For `click` and `tasks`: how many. 2 to 50.
    pub every: u32,
}

/// The moments a pack may give its own pose to (the same keys as the Character page's Reactions).
pub const REACTION_KEYS: [&str; 7] = [
    "task-added",
    "task-started",
    "task-done",
    "all-done",
    "task-deleted",
    "reminder",
    "focus-done",
];

#[derive(Default, Deserialize)]
struct PlutoJson {
    #[serde(default)]
    reactions: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    special: Option<PetSpecial>,
}

fn pose_ok(p: &str) -> bool {
    (1..=24).contains(&p.len()) && p.chars().all(|c| c.is_ascii_lowercase() || c == '_')
}

/// Read `pluto.json` next to a pack's `pet.json`. It is optional and only ever trimmed, never
/// trusted: an unreadable file or an odd entry is dropped and the pack still loads.
pub fn read_extras(
    dir: &Path,
) -> (
    std::collections::BTreeMap<String, String>,
    Option<PetSpecial>,
) {
    let Ok(raw) = fs::read_to_string(dir.join("pluto.json")) else {
        return Default::default();
    };
    if raw.len() > 64 * 1024 {
        return Default::default();
    }
    let parsed: PlutoJson = serde_json::from_str(&raw).unwrap_or_default();
    let reactions = parsed
        .reactions
        .into_iter()
        .filter(|(k, v)| REACTION_KEYS.contains(&k.as_str()) && pose_ok(v))
        .collect();
    let special = parsed.special.and_then(|mut sp| {
        sp.name = sp.name.trim().chars().take(30).collect();
        sp.line = sp.line.trim().chars().take(80).collect();
        sp.every = sp.every.clamp(2, 50);
        (pose_ok(&sp.pose) && !sp.name.is_empty()).then_some(sp)
    });
    (reactions, special)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PetJson {
    id: String,
    display_name: String,
    #[serde(default)]
    description: String,
    spritesheet_path: String,
}

fn bad(msg: impl Into<String>) -> AppError {
    AppError::validation("pet", msg)
}

/// A pack id is a folder name: letters, digits, `-`, `_`, `.`; 1-64 long; never `.` or `..`.
pub fn id_is_valid(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id != "."
        && id != ".."
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Width and height of a WebP image, read from its header.
pub fn webp_size(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 30 || &b[0..4] != b"RIFF" || &b[8..12] != b"WEBP" {
        return None;
    }
    let le24 = |i: usize| u32::from(b[i]) | u32::from(b[i + 1]) << 8 | u32::from(b[i + 2]) << 16;
    match &b[12..16] {
        b"VP8X" => Some((le24(24) + 1, le24(27) + 1)),
        b"VP8L" if b[20] == 0x2f => {
            let bits = u32::from_le_bytes([b[21], b[22], b[23], b[24]]);
            Some(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1))
        }
        b"VP8 " => {
            let w = u32::from(u16::from_le_bytes([b[26], b[27]]) & 0x3fff);
            let h = u32::from(u16::from_le_bytes([b[28], b[29]]) & 0x3fff);
            Some((w, h))
        }
        _ => None,
    }
}

/// Width and height from an SVG's root `width`/`height` attributes (plain numbers or `px`).
pub fn svg_size(text: &str) -> Option<(u32, u32)> {
    let start = text.find("<svg")?;
    let tag = &text[start..start + text[start..].find('>')?];
    let attr = |name: &str| -> Option<u32> {
        let key = format!(" {name}=\"");
        let from = tag.find(&key)? + key.len();
        let raw = &tag[from..from + tag[from..].find('"')?];
        raw.trim_end_matches("px")
            .parse::<f64>()
            .ok()
            .map(|v| v as u32)
    };
    Some((attr("width")?, attr("height")?))
}

/// Rows in a sheet of this size: 9 or 11, or None if the size is not a pet atlas.
pub fn rows_for(width: u32, height: u32) -> Option<u32> {
    if width != COLUMNS * CELL_W {
        return None;
    }
    match height / CELL_H {
        9 | 11 if height.is_multiple_of(CELL_H) => Some(height / CELL_H),
        _ => None,
    }
}

/// Check a pack folder. Returns the pack and the spritesheet path inside it.
pub fn check(dir: &Path, source: PetSource) -> Result<(PetPack, PathBuf)> {
    let raw = fs::read_to_string(dir.join("pet.json")).map_err(|_| bad("pet.json is missing"))?;
    let meta: PetJson =
        serde_json::from_str(&raw).map_err(|e| bad(format!("pet.json is not valid: {e}")))?;
    if !id_is_valid(&meta.id) {
        return Err(bad(
            "pet.json id must be 1-64 letters, digits, '-', '_' or '.'",
        ));
    }
    if meta.display_name.trim().is_empty() {
        return Err(bad("pet.json displayName is empty"));
    }
    let name = meta.spritesheet_path.as_str();
    let format = match name {
        "spritesheet.webp" => "webp",
        "spritesheet.svg" => "svg",
        _ => {
            return Err(bad(
                "spritesheetPath must be spritesheet.webp or spritesheet.svg",
            ))
        }
    };
    if dir.join("spritesheet.webp").exists() && dir.join("spritesheet.svg").exists() {
        return Err(bad("a pack has one spritesheet, not both .webp and .svg"));
    }
    let sheet = dir.join(name);
    let len = fs::metadata(&sheet)
        .map_err(|_| bad(format!("{name} is missing")))?
        .len();
    if len > SHEET_MAX_BYTES {
        return Err(bad(format!("{name} is larger than 25 MB")));
    }
    let bytes = fs::read(&sheet).map_err(|e| AppError::io(e.to_string()))?;
    let size = if format == "webp" {
        webp_size(&bytes)
    } else {
        svg_size(&String::from_utf8_lossy(&bytes))
    };
    let (w, h) = size.ok_or_else(|| bad(format!("{name} could not be read")))?;
    let rows = rows_for(w, h).ok_or_else(|| {
        bad(format!(
            "{name} is {w}x{h}; a pet sheet is 1536x1872 or 1536x2288"
        ))
    })?;
    let (reactions, special) = read_extras(dir);
    let pack = PetPack {
        reactions,
        special,
        id: meta.id,
        display_name: meta.display_name.trim().to_string(),
        description: meta.description,
        rows,
        format: format.to_string(),
        source,
    };
    Ok((pack, sheet))
}

/// Copy a checked pack into `<pets_dir>/<id>/` (pet.json and the spritesheet only).
/// Replaces an earlier copy with the same id.
pub fn install(src: &Path, pets_dir: &Path) -> Result<PetPack> {
    let (mut pack, sheet) = check(src, PetSource::Pluto)?;
    pack.source = PetSource::Pluto;
    let io = |e: std::io::Error| AppError::io(e.to_string());
    let dest = pets_dir.join(&pack.id);
    let tmp = pets_dir.join(format!(".{}.part", pack.id));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).map_err(io)?;
    fs::copy(src.join("pet.json"), tmp.join("pet.json")).map_err(io)?;
    fs::copy(&sheet, tmp.join(sheet.file_name().unwrap_or_default())).map_err(io)?;
    if !pack.reactions.is_empty() || pack.special.is_some() {
        // Only the trimmed, checked entries are kept, never the file as it came.
        let extras = serde_json::json!({ "reactions": pack.reactions, "special": pack.special });
        fs::write(tmp.join("pluto.json"), extras.to_string()).map_err(io)?;
    }
    let _ = fs::remove_dir_all(&dest);
    fs::rename(&tmp, &dest).map_err(io)?;
    Ok(pack)
}

/// Largest ZIP we open, and the most we unpack from it.
pub const ZIP_MAX_BYTES: u64 = 25 * 1024 * 1024;
const UNPACK_MAX_BYTES: u64 = 30 * 1024 * 1024;

/// Install a pack from a ZIP file. The pack may sit at the top of the ZIP or inside one folder.
/// Only `pet.json`, `pluto.json` and `spritesheet.webp|svg` are unpacked (to a temporary folder, then checked
/// and copied by [`install`]); paths that escape the folder, links and oversized files are
/// refused, and nothing in the ZIP is ever run.
pub fn install_zip(zip_path: &Path, pets_dir: &Path) -> Result<PetPack> {
    let io = |e: std::io::Error| AppError::io(e.to_string());
    let bad_zip = |e: zip::result::ZipError| bad(format!("the ZIP could not be read: {e}"));
    if fs::metadata(zip_path).map_err(io)?.len() > ZIP_MAX_BYTES {
        return Err(bad("the ZIP is larger than 25 MB"));
    }
    let mut archive =
        zip::ZipArchive::new(fs::File::open(zip_path).map_err(io)?).map_err(bad_zip)?;
    // Where pet.json is decides the pack's folder inside the ZIP ("" or "name/").
    let mut root: Option<String> = None;
    for i in 0..archive.len() {
        let f = archive.by_index(i).map_err(bad_zip)?;
        let Some(path) = f.enclosed_name() else {
            return Err(bad("the ZIP has an unsafe path"));
        };
        let p = path.to_string_lossy().replace('\\', "/");
        let depth = p.matches('/').count();
        if p.ends_with("pet.json") && depth <= 1 && (p == "pet.json" || p.ends_with("/pet.json")) {
            let prefix = p.trim_end_matches("pet.json").to_owned();
            if root.as_ref().is_some_and(|r| *r != prefix) {
                return Err(bad("the ZIP holds more than one pet"));
            }
            root = Some(prefix);
        }
    }
    let root = root.ok_or_else(|| bad("pet.json is missing from the ZIP"))?;
    let tmp = pets_dir.join(".unzip.part");
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).map_err(io)?;
    let mut unpacked = 0u64;
    for i in 0..archive.len() {
        let mut f = archive.by_index(i).map_err(bad_zip)?;
        let name = f
            .enclosed_name()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let Some(file) = name.strip_prefix(&root) else {
            continue;
        };
        if !matches!(
            file,
            "pet.json" | "pluto.json" | "spritesheet.webp" | "spritesheet.svg"
        ) {
            continue;
        }
        if f.is_symlink() || !f.is_file() {
            return Err(bad(format!("{file} in the ZIP is not a plain file")));
        }
        let size = f.size();
        unpacked += size;
        if size > SHEET_MAX_BYTES || unpacked > UNPACK_MAX_BYTES {
            return Err(bad(format!("{file} in the ZIP is too large")));
        }
        let mut out = fs::File::create(tmp.join(file)).map_err(io)?;
        // Read at most the declared size + 1 so a lying header can't unpack forever.
        let copied = std::io::copy(&mut (&mut f).take(size + 1), &mut out).map_err(io)?;
        if copied > size {
            return Err(bad(format!("{file} in the ZIP is larger than it says")));
        }
    }
    let result = install(&tmp, pets_dir);
    let _ = fs::remove_dir_all(&tmp);
    result
}

/// Every good pack directly under `root` (bad ones are skipped). Sorted by name.
pub fn scan(root: &Path, source: PetSource) -> Vec<(PetPack, PathBuf)> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut found: Vec<_> = entries
        .flatten()
        .filter(|e| e.path().is_dir() && !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|e| check(&e.path(), source).ok())
        .collect();
    found.sort_by(|a, b| {
        a.0.display_name
            .to_lowercase()
            .cmp(&b.0.display_name.to_lowercase())
    });
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal lossless WebP header for the given size (enough for `webp_size`).
    fn webp(w: u32, h: u32) -> Vec<u8> {
        let mut b = b"RIFF\0\0\0\0WEBPVP8L\0\0\0\0".to_vec();
        b.push(0x2f);
        b.extend(((w - 1) | (h - 1) << 14).to_le_bytes());
        b.resize(40, 0);
        b
    }

    fn pack(dir: &Path, id: &str, sheet: &[u8]) {
        fs::create_dir_all(dir).unwrap();
        fs::write(
            dir.join("pet.json"),
            format!(r#"{{"id":"{id}","displayName":"Boba","description":"d","spritesheetPath":"spritesheet.webp"}}"#),
        )
        .unwrap();
        fs::write(dir.join("spritesheet.webp"), sheet).unwrap();
    }

    #[test]
    fn reads_webp_sizes_from_all_three_headers() {
        assert_eq!(webp_size(&webp(1536, 1872)), Some((1536, 1872)));
        let mut x = b"RIFF\0\0\0\0WEBPVP8X\0\0\0\0\0\0\0\0".to_vec();
        x.extend([0xff, 0x05, 0x00, 0x4f, 0x07, 0x00]); // 1535+1, 1871+1
        x.resize(40, 0);
        assert_eq!(webp_size(&x), Some((1536, 1872)));
        let mut y = b"RIFF\0\0\0\0WEBPVP8 \0\0\0\0\0\0\0\0\0\0".to_vec();
        y.extend(1536u16.to_le_bytes());
        y.extend(2288u16.to_le_bytes());
        y.resize(40, 0);
        assert_eq!(webp_size(&y), Some((1536, 2288)));
        assert_eq!(webp_size(b"not an image at all, really not"), None);
    }

    #[test]
    fn reads_svg_size() {
        assert_eq!(
            svg_size(
                r#"<?xml?><svg xmlns="x" width="1536" height="1872" viewBox="0 0 1536 1872">"#
            ),
            Some((1536, 1872))
        );
        assert_eq!(
            svg_size(r#"<svg width="1536px" height="2288px">"#),
            Some((1536, 2288))
        );
        assert_eq!(svg_size("<svg viewBox='0 0 1 1'>"), None);
    }

    #[test]
    fn accepts_v1_and_v2_atlases_only() {
        assert_eq!(rows_for(1536, 1872), Some(9));
        assert_eq!(rows_for(1536, 2288), Some(11));
        assert_eq!(rows_for(1536, 2080), None);
        assert_eq!(rows_for(1024, 1872), None);
    }

    #[test]
    fn ids_cannot_escape_the_folder() {
        assert!(id_is_valid("boba"));
        assert!(id_is_valid("hoodie-cat_2.0"));
        assert!(!id_is_valid(".."));
        assert!(!id_is_valid("a/b"));
        assert!(!id_is_valid(""));
        assert!(!id_is_valid(&"x".repeat(65)));
    }

    #[test]
    fn checks_installs_and_lists_a_pack() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("download/boba");
        pack(&src, "boba", &webp(1536, 1872));
        let pets = t.path().join("pets");
        let got = install(&src, &pets).unwrap();
        assert_eq!(
            (got.id.as_str(), got.rows, got.source),
            ("boba", 9, PetSource::Pluto)
        );
        assert!(pets.join("boba/spritesheet.webp").exists());
        let listed = scan(&pets, PetSource::Pluto);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].0.display_name, "Boba");
        // Installing again replaces the copy instead of failing.
        install(&src, &pets).unwrap();
        assert_eq!(scan(&pets, PetSource::Pluto).len(), 1);
    }

    #[test]
    fn refuses_bad_packs_with_a_reason() {
        let t = tempfile::tempdir().unwrap();
        let wrong = t.path().join("wrong");
        pack(&wrong, "wrong", &webp(1024, 1024));
        assert!(check(&wrong, PetSource::Pluto)
            .unwrap_err()
            .message
            .contains("1024x1024"));
        let escape = t.path().join("escape");
        pack(&escape, "../evil", &webp(1536, 1872));
        assert!(check(&escape, PetSource::Pluto).is_err());
        let empty = t.path().join("empty");
        fs::create_dir_all(&empty).unwrap();
        assert!(check(&empty, PetSource::Pluto)
            .unwrap_err()
            .message
            .contains("pet.json"));
        // A folder of bad packs lists nothing rather than failing.
        assert!(scan(t.path(), PetSource::Codex).is_empty());
    }

    #[test]
    fn our_own_packs_pass_the_check() {
        let packs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs");
        let found = scan(&packs, PetSource::Pluto);
        let mut ids: Vec<_> = found.iter().map(|(p, _)| p.id.as_str()).collect();
        ids.sort_unstable();
        assert_eq!(
            ids,
            ["bear", "bolt", "ghosty", "mira", "mochi", "pip", "raja"]
        );
        assert!(found.iter().all(|(p, _)| p.rows == 9 && p.format == "webp"));
    }

    /// A ZIP holding `files` (name → bytes), written to `path`.
    fn zip_of(path: &Path, files: &[(&str, &[u8])]) {
        let mut w = zip::ZipWriter::new(fs::File::create(path).unwrap());
        for (name, bytes) in files {
            w.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            std::io::Write::write_all(&mut w, bytes).unwrap();
        }
        w.finish().unwrap();
    }

    fn pet_json(id: &str) -> Vec<u8> {
        format!(r#"{{"id":"{id}","displayName":"Kayadu","description":"d","spritesheetPath":"spritesheet.webp"}}"#).into_bytes()
    }

    #[test]
    fn installs_a_pack_from_a_zip_with_a_top_folder_and_extra_files() {
        let t = tempfile::tempdir().unwrap();
        let z = t.path().join("pack.zip");
        let sheet = webp(1536, 1872);
        zip_of(
            &z,
            &[
                ("kayadu-pluto-pet/README.md", b"hi"),
                ("kayadu-pluto-pet/pet.json", &pet_json("kayadu-lohar")),
                ("kayadu-pluto-pet/spritesheet.webp", &sheet),
                ("kayadu-pluto-pet/frame-map.json", b"{}"),
            ],
        );
        let pets = t.path().join("pets");
        let got = install_zip(&z, &pets).unwrap();
        assert_eq!(
            (got.id.as_str(), got.display_name.as_str()),
            ("kayadu-lohar", "Kayadu")
        );
        // Only the pack files are kept; extras and the temporary folder are gone.
        let mut kept: Vec<_> = fs::read_dir(pets.join("kayadu-lohar"))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        kept.sort();
        assert_eq!(kept, ["pet.json", "spritesheet.webp"]);
        assert!(!pets.join(".unzip.part").exists());
    }

    #[test]
    fn a_pack_may_bring_its_own_reactions_and_one_special() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("fox");
        pack(&src, "fox", &webp(1536, 1872));
        fs::write(
            src.join("pluto.json"),
            r#"{"cell":{"width":192},"reactions":{"task-done":"jump","nonsense":"jump","reminder":"Bad Pose!"},
               "special":{"name":" Pounce ","pose":"toss","line":"Pounce!","trigger":"click","every":1}}"#,
        )
        .unwrap();
        let (got, _) = check(&src, PetSource::Pluto).unwrap();
        // Unknown moments and odd poses are dropped; "every" is kept in range.
        assert_eq!(got.reactions.len(), 1);
        assert_eq!(got.reactions["task-done"], "jump");
        let special = got.special.clone().unwrap();
        assert_eq!((special.name.as_str(), special.every), ("Pounce", 2));
        // Installing keeps them (rewritten from the checked values) and a reader sees the same.
        let pets = t.path().join("pets");
        install(&src, &pets).unwrap();
        let (again, _) = check(&pets.join("fox"), PetSource::Pluto).unwrap();
        assert_eq!(
            (again.reactions, again.special),
            (got.reactions, got.special)
        );
    }

    #[test]
    fn a_broken_or_missing_pluto_json_never_stops_a_pack_loading() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("fox");
        pack(&src, "fox", &webp(1536, 1872));
        let (plain, _) = check(&src, PetSource::Pluto).unwrap();
        assert!(plain.reactions.is_empty() && plain.special.is_none());
        fs::write(src.join("pluto.json"), "{ not json").unwrap();
        let (broken, _) = check(&src, PetSource::Pluto).unwrap();
        assert!(broken.reactions.is_empty() && broken.special.is_none());
    }

    #[test]
    fn a_zip_carries_pluto_json_too() {
        let t = tempfile::tempdir().unwrap();
        let z = t.path().join("pack.zip");
        let sheet = webp(1536, 1872);
        zip_of(
            &z,
            &[
                ("pet.json", &pet_json("kayadu-lohar")),
                ("spritesheet.webp", &sheet),
                ("pluto.json", br#"{"reactions":{"all-done":"party"}}"#),
            ],
        );
        let got = install_zip(&z, &t.path().join("pets")).unwrap();
        assert_eq!(got.reactions["all-done"], "party");
    }

    #[test]
    fn installs_a_pack_from_a_zip_without_a_folder() {
        let t = tempfile::tempdir().unwrap();
        let z = t.path().join("flat.zip");
        zip_of(
            &z,
            &[
                ("pet.json", &pet_json("flat")),
                ("spritesheet.webp", &webp(1536, 2288)),
            ],
        );
        assert_eq!(install_zip(&z, &t.path().join("pets")).unwrap().rows, 11);
    }

    #[test]
    fn refuses_bad_zips() {
        let t = tempfile::tempdir().unwrap();
        let pets = t.path().join("pets");
        let none = t.path().join("none.zip");
        zip_of(&none, &[("readme.txt", b"no pet here")]);
        assert!(install_zip(&none, &pets)
            .unwrap_err()
            .message
            .contains("pet.json"));
        let two = t.path().join("two.zip");
        zip_of(
            &two,
            &[
                ("a/pet.json", &pet_json("a")),
                ("b/pet.json", &pet_json("b")),
            ],
        );
        assert!(install_zip(&two, &pets)
            .unwrap_err()
            .message
            .contains("more than one"));
        let small = t.path().join("small.zip");
        zip_of(
            &small,
            &[
                ("pet.json", &pet_json("small")),
                ("spritesheet.webp", &webp(800, 600)),
            ],
        );
        assert!(install_zip(&small, &pets)
            .unwrap_err()
            .message
            .contains("800x600"));
        let junk = t.path().join("junk.zip");
        fs::write(&junk, b"not a zip").unwrap();
        assert!(install_zip(&junk, &pets)
            .unwrap_err()
            .message
            .contains("could not be read"));
        let escape = t.path().join("escape.zip");
        zip_of(&escape, &[("../pet.json", &pet_json("x"))]);
        assert!(install_zip(&escape, &pets).is_err());
    }
}
