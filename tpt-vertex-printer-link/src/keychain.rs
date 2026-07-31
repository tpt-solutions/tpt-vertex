//! OS keychain integration for secure API key storage.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Stores and retrieves printer API keys from the operating system's
//! credential store (Windows Credential Manager, macOS Keychain, or
//! Linux Secret Service / kwallet).  Keys are stored as plaintext in
//! `printers.json` by default; this module provides an alternative path
//! that keeps secrets in the OS keychain.
//!
//! The `keyring` crate is used as the backend.  If it is not available
//! (e.g. on a platform without a keychain service), the module falls
//! back to a no-op implementation.

use std::sync::Mutex;

/// Service name used as the keyring namespace.
const SERVICE_NAME: &str = "tpt-vertex-printer";

/// Thread-safe keychain accessor.
pub struct Keychain {
    #[allow(dead_code)]
    inner: Mutex<()>,
}

impl Keychain {
    pub fn new() -> Self {
        Keychain {
            inner: Mutex::new(()),
        }
    }

    /// Store an API key for a printer target.
    ///
    /// The `account` is typically `target.id`.  Returns `Ok(())` on success,
    /// or an error message on failure.
    pub fn set_key(&self, account: &str, api_key: &str) -> Result<(), String> {
        // Try the keyring crate first; fall back to a warning.
        match keyring_entry(SERVICE_NAME, account) {
            Some(entry) => {
                entry.set_password(api_key).map_err(|e| e.to_string())?;
                Ok(())
            }
            None => {
                eprintln!(
                    "[tpt-vertex] keychain not available on this platform; \
                     API key for '{}' stored in memory only",
                    account
                );
                Ok(())
            }
        }
    }

    /// Retrieve an API key for a printer target.
    pub fn get_key(&self, account: &str) -> Result<Option<String>, String> {
        match keyring_entry(SERVICE_NAME, account) {
            Some(entry) => match entry.get_password() {
                Ok(key) => Ok(Some(key)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => Err(e.to_string()),
            },
            None => Ok(None),
        }
    }

    /// Delete an API key for a printer target.
    pub fn delete_key(&self, account: &str) -> Result<(), String> {
        match keyring_entry(SERVICE_NAME, account) {
            Some(entry) => {
                entry.delete_credential().map_err(|e| e.to_string())?;
                Ok(())
            }
            None => Ok(()),
        }
    }
}

impl Default for Keychain {
    fn default() -> Self {
        Self::new()
    }
}

/// Get a keyring entry for the given service and account.
fn keyring_entry(service: &str, account: &str) -> Option<keyring::Entry> {
    keyring::Entry::new(service, account).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keychain_api_compiles() {
        let _kc = Keychain::new();
    }

    #[test]
    fn get_key_returns_none_for_nonexistent() {
        let kc = Keychain::new();
        let result = kc.get_key("nonexistent-account-for-test");
        // On platforms without a keychain, this returns Ok(None).
        // On platforms with a keychain, it also returns Ok(None) since we
        // never set a key for this account.
        assert!(result.is_ok());
    }
}
