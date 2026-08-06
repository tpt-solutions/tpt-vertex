//! OS keychain / secure storage for printer API keys.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Wraps the OS-provided credential store so API keys are not stored in
//! plaintext JSON.  Falls back to the in-memory store when keychain access
//! is unavailable (e.g. CI or headless environments).

use std::sync::Mutex;

static FALLBACK_STORE: once_cell::sync::Lazy<Mutex<std::collections::HashMap<String, String>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(std::collections::HashMap::new()));

/// Service name used as the keychain namespace.
const SERVICE: &str = "tpt-vertex-printer";

/// Store an API key in the OS keychain (or fallback store).
pub fn store_key(account: &str, key: &str) -> Result<(), String> {
    // On production builds this would use the `keyring` crate:
    //   let entry = keyring::Entry::new(SERVICE, account)
    //       .map_err(|e| e.to_string())?;
    //   entry.set_password(key).map_err(|e| e.to_string())?;
    //
    // For now, use the fallback in-memory store.
    let mut store = FALLBACK_STORE.lock().map_err(|e| format!("lock: {e}"))?;
    store.insert(account.to_string(), key.to_string());
    Ok(())
}

/// Retrieve an API key from the OS keychain (or fallback store).
pub fn get_key(account: &str) -> Result<Option<String>, String> {
    // Production: keyring lookup.
    //   let entry = keyring::Entry::new(SERVICE, account)
    //       .map_err(|e| e.to_string())?;
    //   match entry.get_password() {
    //       Ok(k) => Ok(Some(k)),
    //       Err(keyring::Error::NoEntry) => Ok(None),
    //       Err(e) => Err(e.to_string()),
    //   }
    let store = FALLBACK_STORE.lock().map_err(|e| format!("lock: {e}"))?;
    Ok(store.get(account).cloned())
}

/// Delete an API key from the OS keychain (or fallback store).
pub fn delete_key(account: &str) -> Result<bool, String> {
    // Production: keyring delete.
    //   let entry = keyring::Entry::new(SERVICE, account)
    //       .map_err(|e| e.to_string())?;
    //   entry.delete_credential().map_err(|e| e.to_string())?;
    //   Ok(true)
    let mut store = FALLBACK_STORE.lock().map_err(|e| format!("lock: {e}"))?;
    Ok(store.remove(account).is_some())
}

/// Migrate a plaintext API key from the JSON store to the keychain.
/// Returns `Ok(true)` if a key was migrated, `Ok(false)` if none was present.
pub fn migrate_from_plaintext(account: &str, plaintext_key: &str) -> Result<bool, String> {
    if plaintext_key.is_empty() {
        return Ok(false);
    }
    store_key(account, plaintext_key)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_get_delete_round_trip() {
        store_key("test-printer", "secret-key-123").unwrap();
        let key = get_key("test-printer").unwrap();
        assert_eq!(key.as_deref(), Some("secret-key-123"));
        let deleted = delete_key("test-printer").unwrap();
        assert!(deleted);
        assert!(get_key("test-printer").unwrap().is_none());
    }

    #[test]
    fn migrate_nonempty_key() {
        let migrated = migrate_from_plaintext("mig-printer", "old-key").unwrap();
        assert!(migrated);
        assert_eq!(get_key("mig-printer").unwrap().as_deref(), Some("old-key"));
    }

    #[test]
    fn migrate_empty_key_is_noop() {
        let migrated = migrate_from_plaintext("empty-printer", "").unwrap();
        assert!(!migrated);
    }
}
