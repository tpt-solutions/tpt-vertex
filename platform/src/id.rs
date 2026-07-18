//! Typed identifiers for platform entities.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

use serde::{Deserialize, Serialize};

macro_rules! typed_id {
    ($name:ident, $prefix:literal) => {
        #[doc = concat!("Identifier for a ", $prefix, ".")]
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(raw: impl Into<String>) -> Self {
                $name(raw.into())
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

typed_id!(UserId, "user");
typed_id!(OrgId, "organization");
typed_id!(TeamId, "team");
typed_id!(ProjectId, "project");
typed_id!(SessionId, "session");
