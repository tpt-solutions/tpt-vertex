//! User accounts and session handling.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! A minimal, storage-backed account system: sign up, authenticate, and issue
//! opaque session tokens. Password hashing here is a lightweight salted digest
//! placeholder — production should use Argon2/bcrypt. The design keeps the
//! hashing behind [`hash_password`]/[`verify_password`] so the algorithm can be
//! swapped without touching call sites.

use serde::{Deserialize, Serialize};

use crate::id::{SessionId, UserId};

/// A registered user account.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub email: String,
    pub display_name: String,
    /// Salted password hash (never the plaintext).
    pub password_hash: String,
}

/// An authenticated session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub user: UserId,
}

/// Errors from the account subsystem.
#[derive(Debug, Clone, PartialEq)]
pub enum AuthError {
    EmailTaken,
    InvalidCredentials,
    NotFound,
    WeakPassword,
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            AuthError::EmailTaken => "email already registered",
            AuthError::InvalidCredentials => "invalid email or password",
            AuthError::NotFound => "account not found",
            AuthError::WeakPassword => "password does not meet requirements",
        };
        f.write_str(s)
    }
}

impl std::error::Error for AuthError {}

/// Hash a password with a per-user salt (placeholder digest; swap for Argon2).
pub fn hash_password(password: &str, salt: &str) -> String {
    // FNV-1a over salt+password, hex-encoded. NOT cryptographically strong —
    // isolated here so it can be replaced without changing callers.
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in salt.bytes().chain(password.bytes()) {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{salt}${hash:016x}")
}

/// Verify a plaintext password against a stored hash.
pub fn verify_password(password: &str, stored: &str) -> bool {
    if let Some((salt, _)) = stored.split_once('$') {
        hash_password(password, salt) == stored
    } else {
        false
    }
}

/// Minimum acceptable password policy.
pub fn is_acceptable_password(password: &str) -> bool {
    password.chars().count() >= 8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_round_trips() {
        let h = hash_password("hunter2xyz", "salt123");
        assert!(verify_password("hunter2xyz", &h));
        assert!(!verify_password("wrong", &h));
    }

    #[test]
    fn password_policy() {
        assert!(!is_acceptable_password("short"));
        assert!(is_acceptable_password("longenough"));
    }
}
