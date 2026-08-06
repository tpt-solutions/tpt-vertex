//! User accounts and session handling.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! A minimal, storage-backed account system: sign up, authenticate, and issue
//! opaque session tokens. Password hashing uses Argon2id (via the `argon2`
//! crate) with a per-user salt. The hashing is kept behind
//! [`hash_password`]/[`verify_password`] so the algorithm and parameters can be
//! tuned without touching call sites.

use argon2::Argon2;
use base64::Engine;
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

/// Hash a password with a per-user salt using Argon2id.
///
/// The returned string is `"{salt}${hash}"`, where `salt` is the caller-provided
/// salt echoed back (so it can be re-derived at verify time) and `hash` is the
/// base64-encoded 32-byte Argon2id digest. The salt is deterministically mapped
/// to a valid Argon2 salt so callers may pass any stable, non-secret string.
pub fn hash_password(password: &str, salt: &str) -> String {
    let argon2 = Argon2::default(); // Argon2id with default parameters
    let salt_b64 = derive_salt(salt);
    let mut out = [0u8; 32];
    argon2
        .hash_password_into(password.as_bytes(), salt_b64.as_bytes(), &mut out)
        .expect("argon2id hashing");
    let hash_b64 = base64::engine::general_purpose::STANDARD.encode(out);
    format!("{salt}${hash_b64}")
}

/// Verify a plaintext password against a stored `"{salt}${hash}"` string.
pub fn verify_password(password: &str, stored: &str) -> bool {
    let Some((salt, hash_b64)) = stored.split_once('$') else {
        return false;
    };
    let argon2 = Argon2::default();
    let salt_b64 = derive_salt(salt);
    let mut out = [0u8; 32];
    if argon2
        .hash_password_into(password.as_bytes(), salt_b64.as_bytes(), &mut out)
        .is_err()
    {
        return false;
    }
    let computed = base64::engine::general_purpose::STANDARD.encode(out);
    computed == hash_b64
}

/// Deterministically map an arbitrary caller salt string to a valid,
/// base64-encoded Argon2 salt. The raw salt is always 16 bytes (cyclically
/// derived from the caller's salt), so `hash_password_into` always receives a
/// base64 string of sufficient length (Argon2 requires >= 8 raw bytes).
fn derive_salt(salt: &str) -> String {
    let input = if salt.is_empty() {
        "tpt-vertex-default-salt"
    } else {
        salt
    };
    let raw = input.as_bytes();
    let mut salt_bytes = [0u8; 16];
    for (i, b) in salt_bytes.iter_mut().enumerate() {
        *b = raw[i % raw.len().max(1)];
    }
    base64::engine::general_purpose::STANDARD.encode(salt_bytes)
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
