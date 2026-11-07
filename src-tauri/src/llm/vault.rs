//! Where API keys rest: the OS credential store (Windows Credential Manager,
//! macOS Keychain, Secret Service on Linux), one entry per provider id.
//!
//! This keeps keys out of `llm-settings.json`, which is the file that backups,
//! sync folders, bug-report zips and path-hunting infostealers pick up. It
//! does not stop code already running as the user: on Windows nothing in user
//! space does. `Store` falls back to the file only when no keyring answers.

pub trait Vault: Send {
    /// `Ok(None)`: the keyring works but holds no key for `id`.
    fn get(&self, id: &str) -> Result<Option<String>, String>;
    fn set(&self, id: &str, key: &str) -> Result<(), String>;
    /// Deleting a key that isn't there is not an error.
    fn delete(&self, id: &str) -> Result<(), String>;
}

/// Entries show up in Credential Manager as `<provider id>.com.lyryc.app`.
const SERVICE: &str = "com.lyryc.app";

pub struct OsKeyring;

fn entry(id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, id).map_err(|e| e.to_string())
}

impl Vault for OsKeyring {
    fn get(&self, id: &str) -> Result<Option<String>, String> {
        match entry(id)?.get_password() {
            Ok(key) => Ok(Some(key)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn set(&self, id: &str, key: &str) -> Result<(), String> {
        entry(id)?.set_password(key).map_err(|e| e.to_string())
    }

    fn delete(&self, id: &str) -> Result<(), String> {
        match entry(id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// A keyring that takes nothing: the in-memory store, and the
/// "no keyring on this machine" case in tests.
pub struct NoKeyring;

impl Vault for NoKeyring {
    fn get(&self, _: &str) -> Result<Option<String>, String> {
        Ok(None)
    }
    fn set(&self, _: &str, _: &str) -> Result<(), String> {
        Err("no keyring".into())
    }
    fn delete(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(test)]
pub use mem::MemVault;

#[cfg(test)]
mod mem {
    use super::Vault;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    /// Shared between `Store`s so a test can reload and still find its keys.
    #[derive(Clone, Default)]
    pub struct MemVault(pub Arc<Mutex<HashMap<String, String>>>);

    impl MemVault {
        pub fn has(&self, id: &str) -> bool {
            self.0.lock().unwrap().contains_key(id)
        }
    }

    impl Vault for MemVault {
        fn get(&self, id: &str) -> Result<Option<String>, String> {
            Ok(self.0.lock().unwrap().get(id).cloned())
        }
        fn set(&self, id: &str, key: &str) -> Result<(), String> {
            self.0.lock().unwrap().insert(id.into(), key.into());
            Ok(())
        }
        fn delete(&self, id: &str) -> Result<(), String> {
            self.0.lock().unwrap().remove(id);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Touches the real OS keyring, so it only runs when asked.
    #[test]
    #[ignore = "uses the real OS keyring"]
    fn os_keyring_round_trip() {
        let id = format!("test-{}", uuid::Uuid::new_v4().simple());
        let k = OsKeyring;
        assert_eq!(k.get(&id).unwrap(), None);
        k.set(&id, "sk-round-trip-ключ").unwrap();
        assert_eq!(k.get(&id).unwrap().as_deref(), Some("sk-round-trip-ключ"));
        k.delete(&id).unwrap();
        k.delete(&id).unwrap();
        assert_eq!(k.get(&id).unwrap(), None);
    }
}
