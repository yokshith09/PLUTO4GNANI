//! Settings and preferences service (F0-07): validated, persisted through the DB writer, and
//! announced on the bus only after commit.

use std::collections::BTreeMap;
use std::sync::Arc;

use rusqlite::{params, Connection};
use serde_json::Value;

use crate::clock::Clock;
use crate::db::Database;
use crate::error::{AppError, Result};
use crate::events::Event;
use crate::settings;

pub struct SettingsService {
    db: Arc<Database>,
    clock: Arc<dyn Clock>,
}

fn load(conn: &Connection, table: &'static str) -> Result<BTreeMap<String, Value>> {
    let mut stmt = conn.prepare(&format!("SELECT key, value FROM {table}"))?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut out = BTreeMap::new();
    for row in rows {
        let (key, raw) = row?;
        if let Ok(value) = serde_json::from_str(&raw) {
            out.insert(key, value);
        }
    }
    Ok(out)
}

impl SettingsService {
    pub fn new(db: Arc<Database>, clock: Arc<dyn Clock>) -> Self {
        Self { db, clock }
    }

    /// Every known setting: stored value if valid, otherwise its default.
    pub async fn get_all(&self) -> Result<BTreeMap<String, Value>> {
        let stored = self.db.read(|c| load(c, "settings")).await?;
        let mut all = settings::all_defaults();
        for (key, value) in stored {
            if settings::validate(&key, &value).is_none() && all.contains_key(&key) {
                all.insert(key, value);
            }
        }
        Ok(all)
    }

    /// Validate, write, then publish `SettingChanged` (only after commit).
    pub async fn set(&self, key: &str, value: Value) -> Result<()> {
        if settings::SettingsKey::parse(key).is_none() {
            return Err(AppError::validation(key, "Unknown setting."));
        }
        if let Some(err) = settings::validate(key, &value) {
            return Err(err);
        }
        let at = self.clock.now_ms();
        let key = key.to_owned();
        self.db
            .write(move |tx| {
                tx.execute(
                    "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                    params![key, value.to_string(), at],
                )?;
                Ok(((), vec![Event::SettingChanged { at, key, value }]))
            })
            .await
    }

    /// Settings › Advanced › Reset settings: every setting back to its default. Announces each
    /// one that actually changes, so the shell and the UI apply the defaults at once.
    /// Preferences (layout, last page) and data are untouched.
    pub async fn reset_all(&self) -> Result<()> {
        let at = self.clock.now_ms();
        let before = self.get_all().await?;
        let defaults = settings::all_defaults();
        self.db
            .write(move |tx| {
                tx.execute("DELETE FROM settings", [])?;
                let events = defaults
                    .into_iter()
                    .filter(|(key, value)| before.get(key) != Some(value))
                    .map(|(key, value)| Event::SettingChanged { at, key, value })
                    .collect();
                Ok(((), events))
            })
            .await
    }

    pub async fn get_pref(&self, key: &str) -> Result<Option<Value>> {
        let key = key.to_owned();
        let mut all = self.db.read(|c| load(c, "user_preferences")).await?;
        Ok(all.remove(&key))
    }

    pub async fn set_pref(&self, key: &str, value: Value) -> Result<()> {
        if key.trim().is_empty() {
            return Err(AppError::validation("key", "A preference needs a name."));
        }
        let at = self.clock.now_ms();
        let key = key.to_owned();
        self.db
            .write(move |tx| {
                tx.execute(
                    "INSERT INTO user_preferences (key, value, updated_at) VALUES (?1, ?2, ?3)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                    params![key, value.to_string(), at],
                )?;
                Ok(((), vec![]))
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::{EventBus, Recv};
    use crate::clock::FakeClock;
    use chrono_tz::Tz;
    use serde_json::json;

    fn service(dir: &tempfile::TempDir, bus: &EventBus) -> SettingsService {
        let db = Arc::new(Database::open(&dir.path().join("t.db"), bus.clone()).unwrap());
        SettingsService::new(db, Arc::new(FakeClock::new(1_000, Tz::UTC)))
    }

    #[tokio::test]
    async fn reset_puts_every_setting_back_and_announces_only_real_changes() {
        let dir = tempfile::tempdir().unwrap();
        let bus = EventBus::default();
        let s = service(&dir, &bus);
        s.set("pet.size", json!("L")).await.unwrap();
        s.set("general.theme", json!("system")).await.unwrap(); // already the default
        s.set_pref("ui.last_view", json!("tasks")).await.unwrap();
        let mut events = bus.subscribe();
        s.reset_all().await.unwrap();
        assert_eq!(s.get_all().await.unwrap(), settings::all_defaults());
        assert_eq!(
            s.get_pref("ui.last_view").await.unwrap(),
            Some(json!("tasks"))
        );
        match events.recv().await {
            Recv::Event(Event::SettingChanged { key, value, .. }) => {
                assert_eq!(key, "pet.size");
                assert_eq!(value, json!("M"));
            }
            other => panic!("expected one SettingChanged, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn missing_keys_return_their_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let s = service(&dir, &EventBus::default());
        let all = s.get_all().await.unwrap();
        assert_eq!(all.len(), 21);
        assert_eq!(all["general.theme"], json!("system"));
    }

    #[tokio::test]
    async fn set_persists_and_publishes_after_commit() {
        let dir = tempfile::tempdir().unwrap();
        let bus = EventBus::default();
        let mut events = bus.subscribe();
        let s = service(&dir, &bus);
        s.set("general.theme", json!("dark")).await.unwrap();
        assert_eq!(s.get_all().await.unwrap()["general.theme"], json!("dark"));
        match events.recv().await {
            Recv::Event(Event::SettingChanged { key, value, at }) => {
                assert_eq!(
                    (key.as_str(), value, at),
                    ("general.theme", json!("dark"), 1_000)
                );
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[tokio::test]
    async fn invalid_and_unknown_settings_are_refused_and_name_the_field() {
        let dir = tempfile::tempdir().unwrap();
        let s = service(&dir, &EventBus::default());
        let e = s.set("pet.opacity", json!(150)).await.unwrap_err();
        assert_eq!(e.field.as_deref(), Some("pet.opacity"));
        let e = s.set("nope", json!(1)).await.unwrap_err();
        assert_eq!(e.field.as_deref(), Some("nope"));
        assert_eq!(s.get_all().await.unwrap()["pet.opacity"], json!(100));
    }

    #[tokio::test]
    async fn preferences_round_trip_and_do_not_publish() {
        let dir = tempfile::tempdir().unwrap();
        let s = service(&dir, &EventBus::default());
        assert_eq!(s.get_pref("ui.last_view").await.unwrap(), None);
        s.set_pref("ui.last_view", json!("tasks")).await.unwrap();
        assert_eq!(
            s.get_pref("ui.last_view").await.unwrap(),
            Some(json!("tasks"))
        );
    }
}
