//! The private diary (FR-86). Entries are encrypted at rest with XChaCha20-Poly1305; the key is
//! derived from the password with Argon2id and held in memory only while the diary is unlocked.
//! Entries are encrypted with a random data key. That key is stored twice, wrapped by the
//! password-derived key and by a recovery key shown once at setup, so a forgotten password can be
//! replaced with the recovery key. With neither, the diary cannot be opened (erase is the last resort).
//!
//! Each entry is bound to its day (the day is the associated data), so one day's ciphertext cannot
//! be moved onto another. The database holds ciphertext, nonce and salt as hex text.

use std::sync::Mutex;

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::rand_core::RngCore;
use chacha20poly1305::aead::{Aead, KeyInit, OsRng, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use chrono::NaiveDate;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::clock::Clock;
use crate::db::Database;
use crate::error::{AppError, Result};

pub const PASSWORD_MIN: usize = 6;
pub const TEXT_MAX: usize = 200_000;
const WRAP_AAD: &[u8] = b"wrap";
const RECOVERY_ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

type Key = Zeroizing<[u8; 32]>;

/// The diary's in-memory key. `None` = locked.
#[derive(Default)]
pub struct Diary {
    key: Mutex<Option<Key>>,
}

impl Diary {
    fn set(&self, key: Option<Key>) {
        *self.key.lock().unwrap_or_else(|p| p.into_inner()) = key;
    }
    fn get(&self) -> Option<Key> {
        self.key
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .map(|k| Zeroizing::new(**k))
    }
    pub fn is_unlocked(&self) -> bool {
        self.key.lock().unwrap_or_else(|p| p.into_inner()).is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct DiaryStatus {
    /// A password has been set.
    pub configured: bool,
    pub unlocked: bool,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Result<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return Err(AppError::internal("The diary data is damaged."));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            s.get(i..i + 2)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
                .ok_or_else(|| AppError::internal("The diary data is damaged."))
        })
        .collect()
}

#[cfg(not(test))]
fn kdf_params() -> Params {
    // 19 MiB, 2 passes: the OWASP minimum for Argon2id.
    Params::new(19 * 1024, 2, 1, Some(32)).expect("valid argon2 params")
}
#[cfg(test)]
fn kdf_params() -> Params {
    Params::new(8, 1, 1, Some(32)).expect("valid argon2 params")
}

async fn derive(password: &str, salt: Vec<u8>) -> Result<Key> {
    let password = Zeroizing::new(password.as_bytes().to_vec());
    tokio::task::spawn_blocking(move || {
        let mut key = Zeroizing::new([0u8; 32]);
        Argon2::new(Algorithm::Argon2id, Version::V0x13, kdf_params())
            .hash_password_into(&password, &salt, &mut *key)
            .map_err(|_| AppError::internal("Pluto could not prepare the diary key."))?;
        Ok(key)
    })
    .await
    .map_err(|_| AppError::internal("Pluto could not prepare the diary key."))?
}

fn seal(key: &Key, aad: &[u8], plain: &[u8]) -> Result<(String, String)> {
    let cipher = XChaCha20Poly1305::new((&**key).into());
    let mut nonce = [0u8; 24];
    OsRng.fill_bytes(&mut nonce);
    let data = cipher
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: plain, aad })
        .map_err(|_| AppError::internal("Pluto could not lock the diary entry."))?;
    Ok((hex(&nonce), hex(&data)))
}

fn open(key: &Key, aad: &[u8], nonce: &str, data: &str) -> Result<Vec<u8>> {
    let nonce = unhex(nonce)?;
    if nonce.len() != 24 {
        return Err(AppError::internal("The diary data is damaged."));
    }
    XChaCha20Poly1305::new((&**key).into())
        .decrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &unhex(data)?,
                aad,
            },
        )
        .map_err(|_| AppError::internal("The diary data is damaged."))
}

/// 160 random bits as eight groups of four letters and digits, e.g. `ABCD-EFGH-…`.
fn new_recovery_key() -> String {
    let mut raw = [0u8; 20];
    OsRng.fill_bytes(&mut raw);
    let (mut acc, mut bits, mut out) = (0u32, 0u32, String::new());
    for b in raw {
        acc = (acc << 8) | u32::from(b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(char::from(RECOVERY_ALPHABET[((acc >> bits) & 31) as usize]));
        }
    }
    out.as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join("-")
}

/// Upper-case, with spaces and dashes dropped, so a key typed by hand matches.
fn normalize_recovery(key: &str) -> String {
    key.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

fn need_key(diary: &Diary) -> Result<Key> {
    diary
        .get()
        .ok_or_else(|| AppError::invalid_transition("The diary is locked."))
}

fn check_day(day: &str) -> Result<()> {
    NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .map(|_| ())
        .map_err(|_| AppError::validation("day", "Pick a day."))
}

pub async fn status(db: &Database, diary: &Diary) -> Result<DiaryStatus> {
    let configured = db
        .read(|conn| {
            Ok(conn
                .query_row("SELECT 1 FROM diary_meta WHERE id = 1", [], |_| Ok(()))
                .optional()?
                .is_some())
        })
        .await?;
    Ok(DiaryStatus {
        configured,
        unlocked: configured && diary.is_unlocked(),
    })
}

fn wrap(kek: &Key, dek: &Key) -> Result<(String, String)> {
    seal(kek, WRAP_AAD, &**dek)
}

fn unwrap_key(kek: &Key, nonce: &str, data: &str) -> Option<Key> {
    let raw = open(kek, WRAP_AAD, nonce, data).ok()?;
    let arr: [u8; 32] = raw.try_into().ok()?;
    Some(Zeroizing::new(arr))
}

fn random_salt() -> [u8; 16] {
    let mut salt = [0u8; 16];
    OsRng.fill_bytes(&mut salt);
    salt
}

/// Set the first password. The diary is unlocked afterwards. Returns the recovery key, which is
/// shown once and never stored in readable form.
pub async fn setup(
    db: &Database,
    diary: &Diary,
    clock: &dyn Clock,
    password: &str,
) -> Result<String> {
    if password.chars().count() < PASSWORD_MIN {
        return Err(AppError::validation(
            "password",
            format!("Use at least {PASSWORD_MIN} characters."),
        ));
    }
    let mut dek_raw = [0u8; 32];
    OsRng.fill_bytes(&mut dek_raw);
    let dek: Key = Zeroizing::new(dek_raw);
    let recovery = new_recovery_key();
    let (salt, rec_salt) = (random_salt(), random_salt());
    let kek = derive(password, salt.to_vec()).await?;
    let rkek = derive(&normalize_recovery(&recovery), rec_salt.to_vec()).await?;
    let (wrap_nonce, wrapped) = wrap(&kek, &dek)?;
    let (rec_nonce, rec_wrapped) = wrap(&rkek, &dek)?;
    let (salt, rec_salt, now) = (hex(&salt), hex(&rec_salt), clock.now_ms());
    db.write(move |tx| {
        let made = tx.execute(
            "INSERT OR IGNORE INTO diary_meta (id, salt, wrap_nonce, wrapped, rec_salt, rec_nonce, rec_wrapped, created_at)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![salt, wrap_nonce, wrapped, rec_salt, rec_nonce, rec_wrapped, now],
        )?;
        if made == 0 {
            return Err(AppError::conflict("The diary already has a password."));
        }
        Ok(((), vec![]))
    })
    .await?;
    diary.set(Some(dek));
    Ok(recovery)
}

type MetaRow = (String, String, String, String, String, String);

async fn meta(db: &Database) -> Result<MetaRow> {
    db.read(|conn| {
        Ok(conn
            .query_row(
                "SELECT salt, wrap_nonce, wrapped, rec_salt, rec_nonce, rec_wrapped FROM diary_meta WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )
            .optional()?)
    })
    .await?
    .ok_or_else(|| AppError::not_found("The diary has no password yet."))
}

pub async fn unlock(db: &Database, diary: &Diary, password: &str) -> Result<()> {
    let m = meta(db).await?;
    let kek = derive(password, unhex(&m.0)?).await?;
    match unwrap_key(&kek, &m.1, &m.2) {
        Some(dek) => {
            diary.set(Some(dek));
            Ok(())
        }
        None => Err(AppError::validation(
            "password",
            "That is not the right password.",
        )),
    }
}

/// Forgot the password: the recovery key opens the diary and a new password replaces the old one.
/// The recovery key stays the same.
pub async fn recover(
    db: &Database,
    diary: &Diary,
    recovery_key: &str,
    new_password: &str,
) -> Result<()> {
    if new_password.chars().count() < PASSWORD_MIN {
        return Err(AppError::validation(
            "password",
            format!("Use at least {PASSWORD_MIN} characters."),
        ));
    }
    let m = meta(db).await?;
    let rkek = derive(&normalize_recovery(recovery_key), unhex(&m.3)?).await?;
    let dek = unwrap_key(&rkek, &m.4, &m.5).ok_or_else(|| {
        AppError::validation("recovery_key", "That is not the right recovery key.")
    })?;
    let salt = random_salt();
    let kek = derive(new_password, salt.to_vec()).await?;
    let (nonce, wrapped) = wrap(&kek, &dek)?;
    let salt = hex(&salt);
    db.write(move |tx| {
        tx.execute(
            "UPDATE diary_meta SET salt = ?1, wrap_nonce = ?2, wrapped = ?3 WHERE id = 1",
            params![salt, nonce, wrapped],
        )?;
        Ok(((), vec![]))
    })
    .await?;
    diary.set(Some(dek));
    Ok(())
}

/// Forget the key. Nothing readable stays in memory from here on.
pub fn lock(diary: &Diary) {
    diary.set(None);
}

/// Days that have an entry, newest first. Needs the diary unlocked, so the list is private too.
pub async fn days(db: &Database, diary: &Diary) -> Result<Vec<String>> {
    need_key(diary)?;
    db.read(|conn| {
        let mut stmt = conn.prepare("SELECT day FROM diary_entries ORDER BY day DESC")?;
        let rows = stmt
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(rows)
    })
    .await
}

pub async fn day_get(db: &Database, diary: &Diary, day: &str) -> Result<String> {
    let key = need_key(diary)?;
    check_day(day)?;
    let day = day.to_owned();
    let row = db
        .read({
            let day = day.clone();
            move |conn| {
                Ok(conn
                    .query_row(
                        "SELECT nonce, data FROM diary_entries WHERE day = ?1",
                        [&day],
                        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
                    )
                    .optional()?)
            }
        })
        .await?;
    match row {
        None => Ok(String::new()),
        Some((nonce, data)) => String::from_utf8(open(&key, day.as_bytes(), &nonce, &data)?)
            .map_err(|_| AppError::internal("The diary data is damaged.")),
    }
}

/// Save a day's text. Empty text removes the entry.
pub async fn day_save(
    db: &Database,
    diary: &Diary,
    clock: &dyn Clock,
    day: &str,
    text: &str,
) -> Result<()> {
    let key = need_key(diary)?;
    check_day(day)?;
    if text.len() > TEXT_MAX {
        return Err(AppError::validation("text", "That entry is too long."));
    }
    let (day, now) = (day.to_owned(), clock.now_ms());
    if text.trim().is_empty() {
        return db
            .write(move |tx| {
                tx.execute("DELETE FROM diary_entries WHERE day = ?1", [&day])?;
                Ok(((), vec![]))
            })
            .await;
    }
    let (nonce, data) = seal(&key, day.as_bytes(), text.as_bytes())?;
    db.write(move |tx| {
        tx.execute(
            "INSERT INTO diary_entries (day, nonce, data, updated_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(day) DO UPDATE SET nonce = excluded.nonce, data = excluded.data, updated_at = excluded.updated_at",
            params![day, nonce, data, now],
        )?;
        Ok(((), vec![]))
    })
    .await
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct DiaryHit {
    pub day: String,
    /// A short stretch of the entry around the first match.
    pub snippet: String,
}

pub const SEARCH_MAX: usize = 100;
const SNIPPET_CHARS: usize = 60;

/// Find entries that contain `query` (ignoring case), newest first. Entries are decrypted one at a
/// time here, in memory, only while the diary is unlocked; nothing is indexed or stored.
pub async fn search(db: &Database, diary: &Diary, query: &str) -> Result<Vec<DiaryHit>> {
    let key = need_key(diary)?;
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Ok(vec![]);
    }
    let rows = db
        .read(|conn| {
            let mut stmt =
                conn.prepare("SELECT day, nonce, data FROM diary_entries ORDER BY day DESC")?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await?;
    let mut hits = Vec::new();
    for (day, nonce, data) in rows {
        let text = String::from_utf8(open(&key, day.as_bytes(), &nonce, &data)?)
            .map_err(|_| AppError::internal("The diary data is damaged."))?;
        let lower = text.to_lowercase();
        let Some(at) = lower.find(&needle) else {
            continue;
        };
        // `lower` can differ in byte length from `text` for some scripts, so cut by characters.
        let chars: Vec<char> = text.chars().collect();
        let at_chars = lower[..at].chars().count().min(chars.len());
        let from = at_chars.saturating_sub(SNIPPET_CHARS / 3);
        let to = (from + SNIPPET_CHARS).min(chars.len());
        let snippet: String = chars[from..to]
            .iter()
            .collect::<String>()
            .replace('\n', " ");
        hits.push(DiaryHit {
            day,
            snippet: format!(
                "{}{}{}",
                if from > 0 { "…" } else { "" },
                snippet.trim(),
                if to < chars.len() { "…" } else { "" }
            ),
        });
        if hits.len() >= SEARCH_MAX {
            break;
        }
    }
    Ok(hits)
}

/// Forgotten password: erase the diary (every entry and the password) so a new one can be set.
pub async fn erase(db: &Database, diary: &Diary) -> Result<()> {
    diary.set(None);
    db.write(|tx| {
        tx.execute("DELETE FROM diary_entries", [])?;
        tx.execute("DELETE FROM diary_meta", [])?;
        Ok(((), vec![]))
    })
    .await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono_tz::Tz;

    use super::*;
    use crate::bus::EventBus;
    use crate::clock::FakeClock;
    use crate::error::ErrorCode;

    fn setup_db() -> (tempfile::TempDir, Arc<Database>, FakeClock, Diary) {
        let dir = tempfile::tempdir().unwrap();
        let db =
            Arc::new(Database::open(&dir.path().join("pluto.db"), EventBus::default()).unwrap());
        (
            dir,
            db,
            FakeClock::new(1_793_188_800_000, Tz::UTC),
            Diary::default(),
        )
    }

    #[test]
    fn hex_round_trips_and_rejects_junk() {
        assert_eq!(unhex(&hex(&[0, 1, 254, 255])).unwrap(), [0, 1, 254, 255]);
        assert!(unhex("abc").is_err());
        assert!(unhex("zz").is_err());
    }

    #[tokio::test]
    async fn a_new_diary_needs_a_password_and_opens_only_with_it() {
        let (_d, db, clock, diary) = setup_db();
        assert_eq!(
            status(&db, &diary).await.unwrap(),
            DiaryStatus {
                configured: false,
                unlocked: false
            }
        );
        assert_eq!(
            setup(&db, &diary, &clock, "abc").await.unwrap_err().code,
            ErrorCode::Validation
        );
        setup(&db, &diary, &clock, "correct horse").await.unwrap();
        assert_eq!(
            status(&db, &diary).await.unwrap(),
            DiaryStatus {
                configured: true,
                unlocked: true
            }
        );
        assert_eq!(
            setup(&db, &diary, &clock, "another one")
                .await
                .unwrap_err()
                .code,
            ErrorCode::Conflict
        );

        lock(&diary);
        assert!(!status(&db, &diary).await.unwrap().unlocked);
        assert_eq!(
            unlock(&db, &diary, "wrong password")
                .await
                .unwrap_err()
                .code,
            ErrorCode::Validation
        );
        assert!(!diary.is_unlocked());
        unlock(&db, &diary, "correct horse").await.unwrap();
        assert!(diary.is_unlocked());
    }

    #[tokio::test]
    async fn entries_round_trip_and_the_database_never_holds_the_text() {
        let (dir, db, clock, diary) = setup_db();
        setup(&db, &diary, &clock, "correct horse").await.unwrap();
        let secret = "I need to submit the lab report by Friday ✓";
        day_save(&db, &diary, &clock, "2026-10-28", secret)
            .await
            .unwrap();
        day_save(&db, &diary, &clock, "2026-10-27", "second day")
            .await
            .unwrap();
        assert_eq!(day_get(&db, &diary, "2026-10-28").await.unwrap(), secret);
        assert_eq!(day_get(&db, &diary, "2026-10-01").await.unwrap(), "");
        assert_eq!(
            days(&db, &diary).await.unwrap(),
            ["2026-10-28", "2026-10-27"]
        );

        // The same text twice gives different ciphertext (fresh nonce each save).
        let read = |day: &str| {
            let conn = rusqlite::Connection::open(dir.path().join("pluto.db")).unwrap();
            conn.query_row(
                "SELECT data FROM diary_entries WHERE day = ?1",
                [day],
                |r| r.get::<_, String>(0),
            )
            .unwrap()
        };
        let first = read("2026-10-28");
        day_save(&db, &diary, &clock, "2026-10-28", secret)
            .await
            .unwrap();
        assert_ne!(first, read("2026-10-28"));

        // No part of the plaintext, in the database or its write-ahead log, is readable.
        let mut bytes = Vec::new();
        for entry in std::fs::read_dir(dir.path()).unwrap().flatten() {
            if entry.file_name().to_string_lossy().starts_with("pluto.db") {
                bytes.extend(std::fs::read(entry.path()).unwrap());
            }
        }
        let hay = String::from_utf8_lossy(&bytes);
        assert!(!hay.contains("lab report") && !hay.contains("second day"));
    }

    #[tokio::test]
    async fn a_locked_diary_reads_and_writes_nothing() {
        let (_d, db, clock, diary) = setup_db();
        setup(&db, &diary, &clock, "correct horse").await.unwrap();
        day_save(&db, &diary, &clock, "2026-10-28", "hello")
            .await
            .unwrap();
        lock(&diary);
        for err in [
            day_get(&db, &diary, "2026-10-28").await.unwrap_err(),
            day_save(&db, &diary, &clock, "2026-10-28", "x")
                .await
                .unwrap_err(),
            days(&db, &diary).await.unwrap_err(),
        ] {
            assert_eq!(err.code, ErrorCode::InvalidTransition);
        }
    }

    #[tokio::test]
    async fn an_entry_moved_to_another_day_cannot_be_opened_and_empty_text_removes_it() {
        let (dir, db, clock, diary) = setup_db();
        setup(&db, &diary, &clock, "correct horse").await.unwrap();
        day_save(&db, &diary, &clock, "2026-10-28", "hello")
            .await
            .unwrap();
        rusqlite::Connection::open(dir.path().join("pluto.db"))
            .unwrap()
            .execute("UPDATE diary_entries SET day = '2026-10-29'", [])
            .unwrap();
        assert_eq!(
            day_get(&db, &diary, "2026-10-29").await.unwrap_err().code,
            ErrorCode::Internal
        );
        day_save(&db, &diary, &clock, "2026-10-29", "  ")
            .await
            .unwrap();
        assert!(days(&db, &diary).await.unwrap().is_empty());
        assert!(day_save(&db, &diary, &clock, "tomorrow", "x")
            .await
            .is_err());
    }

    #[tokio::test]
    async fn erasing_lets_a_new_password_be_set_and_old_entries_are_gone() {
        let (_d, db, clock, diary) = setup_db();
        setup(&db, &diary, &clock, "old password").await.unwrap();
        day_save(&db, &diary, &clock, "2026-10-28", "hello")
            .await
            .unwrap();
        erase(&db, &diary).await.unwrap();
        assert_eq!(
            status(&db, &diary).await.unwrap(),
            DiaryStatus {
                configured: false,
                unlocked: false
            }
        );
        setup(&db, &diary, &clock, "new password").await.unwrap();
        assert!(days(&db, &diary).await.unwrap().is_empty());
    }

    #[test]
    fn recovery_keys_are_long_grouped_and_typed_back_loosely() {
        let k = new_recovery_key();
        assert_eq!(k.len(), 32 + 7);
        assert!(k.split('-').all(|g| g.len() == 4));
        assert_ne!(k, new_recovery_key());
        assert_eq!(
            normalize_recovery(&k.to_lowercase().replace('-', " ")),
            normalize_recovery(&k)
        );
    }

    #[tokio::test]
    async fn a_forgotten_password_is_replaced_with_the_recovery_key_and_no_entry_is_lost() {
        let (_d, db, clock, diary) = setup_db();
        let key = setup(&db, &diary, &clock, "old password").await.unwrap();
        day_save(&db, &diary, &clock, "2026-10-28", "kept safe")
            .await
            .unwrap();
        lock(&diary);

        let wrong = recover(
            &db,
            &diary,
            "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH",
            "new password",
        )
        .await;
        assert_eq!(wrong.unwrap_err().code, ErrorCode::Validation);
        assert!(!diary.is_unlocked());
        assert_eq!(
            recover(&db, &diary, &key, "abc").await.unwrap_err().code,
            ErrorCode::Validation
        );

        recover(&db, &diary, &key.to_lowercase(), "new password")
            .await
            .unwrap();
        assert_eq!(
            day_get(&db, &diary, "2026-10-28").await.unwrap(),
            "kept safe"
        );
        lock(&diary);
        assert!(unlock(&db, &diary, "old password").await.is_err());
        unlock(&db, &diary, "new password").await.unwrap();
        // The recovery key still works after a recovery.
        lock(&diary);
        recover(&db, &diary, &key, "third password").await.unwrap();
        assert_eq!(
            day_get(&db, &diary, "2026-10-28").await.unwrap(),
            "kept safe"
        );
    }

    #[tokio::test]
    async fn search_finds_text_ignoring_case_only_while_unlocked_and_shows_a_snippet() {
        let (_d, db, clock, diary) = setup_db();
        setup(&db, &diary, &clock, "correct horse").await.unwrap();
        day_save(
            &db,
            &diary,
            &clock,
            "2026-10-27",
            "Went to the Chemistry lab and met Rao.",
        )
        .await
        .unwrap();
        day_save(
            &db,
            &diary,
            &clock,
            "2026-10-28",
            "Quiet day.\nLong walk by the river, then home.",
        )
        .await
        .unwrap();
        day_save(&db, &diary, &clock, "2026-10-26", "Nothing special.")
            .await
            .unwrap();

        let hits = search(&db, &diary, "  CHEMISTRY ").await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].day, "2026-10-27");
        assert!(hits[0].snippet.contains("Chemistry"));
        let hits = search(&db, &diary, "o").await.unwrap();
        assert_eq!(
            hits.iter().map(|h| h.day.as_str()).collect::<Vec<_>>(),
            ["2026-10-28", "2026-10-27", "2026-10-26"],
            "newest first"
        );
        assert!(!search(&db, &diary, "river").await.unwrap()[0]
            .snippet
            .contains('\n'));
        assert!(search(&db, &diary, "zebra").await.unwrap().is_empty());
        assert!(search(&db, &diary, "   ").await.unwrap().is_empty());

        lock(&diary);
        assert_eq!(
            search(&db, &diary, "river").await.unwrap_err().code,
            ErrorCode::InvalidTransition
        );
    }
}
