//! API keys for the cloud services (Gnani, Gemini, Groq), kept in the OS keychain (T-105).
//!
//! Keys never go into the database, the export, the logs or an error message. [`SecretStore`] is
//! the only way the rest of the app reads or writes them. [`KeychainSecrets`] is the real one
//! (Windows Credential Manager, macOS Keychain); [`MemorySecrets`] is for tests.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::error::{AppError, Result};

/// Keychain service name; one entry per [`Account`].
pub const SERVICE: &str = "app.pluto.desktop.voice";
/// Longest key accepted. Real keys are far shorter; this stops pasted junk.
pub const SECRET_MAX: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Account {
    Gnani,
    Gemini,
    Groq,
}

impl Account {
    pub const ALL: [Account; 3] = [Self::Gnani, Self::Gemini, Self::Groq];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Gnani => "gnani",
            Self::Gemini => "gemini",
            Self::Groq => "groq",
        }
    }

    /// The account named by a command argument (`voice_key_set`'s `account`).
    pub fn parse(name: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|a| a.as_str() == name)
            .ok_or_else(|| {
                AppError::validation("account", "That isn't a service Pluto has a key for.")
            })
    }
}

/// Check a key before it is stored. The message never repeats the key.
pub fn validate(secret: &str) -> Result<String> {
    let secret = secret.trim();
    if secret.is_empty() {
        return Err(AppError::validation("secret", "Paste your key first."));
    }
    if secret.chars().count() > SECRET_MAX {
        return Err(AppError::validation(
            "secret",
            "That is too long to be a key.",
        ));
    }
    // Keys are sent as HTTP headers: anything that is not visible ASCII could break or inject.
    if !secret.chars().all(|c| c.is_ascii_graphic()) {
        return Err(AppError::validation(
            "secret",
            "A key has no spaces or special characters.",
        ));
    }
    Ok(secret.to_owned())
}

pub trait SecretStore: Send + Sync {
    fn set(&self, account: Account, secret: &str) -> Result<()>;
    fn get(&self, account: Account) -> Result<Option<String>>;
    /// Removing a key that is not there is not an error.
    fn clear(&self, account: Account) -> Result<()>;

    fn has(&self, account: Account) -> Result<bool> {
        Ok(self.get(account)?.is_some())
    }
}

/// The OS keychain.
#[derive(Debug, Default, Clone, Copy)]
pub struct KeychainSecrets;

fn entry(account: Account) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, account.as_str()).map_err(keychain_error)
}

/// A fixed message: the library's own text could name the account or the platform store.
fn keychain_error(err: keyring::Error) -> AppError {
    tracing::warn!(kind = %std::any::type_name_of_val(&err), "keychain error");
    AppError::internal(
        "Pluto couldn't reach your system's saved passwords. Your key was not changed.",
    )
}

impl SecretStore for KeychainSecrets {
    fn set(&self, account: Account, secret: &str) -> Result<()> {
        entry(account)?
            .set_password(&validate(secret)?)
            .map_err(keychain_error)
    }

    fn get(&self, account: Account) -> Result<Option<String>> {
        match entry(account)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(keychain_error(e)),
        }
    }

    fn clear(&self, account: Account) -> Result<()> {
        match entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(keychain_error(e)),
        }
    }
}

/// In memory only. For tests and for builds without a keychain.
#[derive(Default)]
pub struct MemorySecrets(Mutex<HashMap<Account, String>>);

impl MemorySecrets {
    pub fn new() -> Self {
        Self::default()
    }

    fn map(&self) -> std::sync::MutexGuard<'_, HashMap<Account, String>> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

// No `Debug` that prints the map.
impl std::fmt::Debug for MemorySecrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MemorySecrets(..)")
    }
}

impl SecretStore for MemorySecrets {
    fn set(&self, account: Account, secret: &str) -> Result<()> {
        let secret = validate(secret)?;
        self.map().insert(account, secret);
        Ok(())
    }

    fn get(&self, account: Account) -> Result<Option<String>> {
        Ok(self.map().get(&account).cloned())
    }

    fn clear(&self, account: Account) -> Result<()> {
        self.map().remove(&account);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;

    #[test]
    fn a_key_round_trips_and_clears() {
        let s = MemorySecrets::new();
        assert!(!s.has(Account::Groq).unwrap());
        s.set(Account::Groq, "  gsk_abc123  ").unwrap();
        assert_eq!(
            s.get(Account::Groq).unwrap().as_deref(),
            Some("gsk_abc123"),
            "trimmed"
        );
        assert!(s.has(Account::Groq).unwrap());
        assert!(!s.has(Account::Gemini).unwrap(), "accounts are separate");
        s.clear(Account::Groq).unwrap();
        assert_eq!(s.get(Account::Groq).unwrap(), None);
        s.clear(Account::Groq).unwrap();
    }

    #[test]
    fn bad_keys_are_refused_without_echoing_them() {
        let s = MemorySecrets::new();
        for bad in [
            "",
            "   ",
            "has space",
            "new\nline",
            "ключ",
            &"x".repeat(SECRET_MAX + 1),
        ] {
            let err = s.set(Account::Gnani, bad).unwrap_err();
            assert_eq!(err.code, ErrorCode::Validation);
            if !bad.trim().is_empty() {
                assert!(
                    !err.message.contains(bad.trim()),
                    "message must not repeat the key"
                );
            }
        }
        assert_eq!(s.get(Account::Gnani).unwrap(), None);
    }

    #[test]
    fn debug_output_never_shows_a_key() {
        let s = MemorySecrets::new();
        s.set(Account::Gemini, "AIza-super-secret").unwrap();
        assert!(!format!("{s:?}").contains("secret"));
    }

    #[test]
    fn account_names_parse() {
        assert_eq!(Account::parse("gemini").unwrap(), Account::Gemini);
        assert_eq!(
            Account::parse("openai").unwrap_err().code,
            ErrorCode::Validation
        );
        for a in Account::ALL {
            assert_eq!(Account::parse(a.as_str()).unwrap(), a);
        }
    }
}
