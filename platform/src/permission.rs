//! Roles and permission levels shared across the platform.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

use serde::{Deserialize, Serialize};

/// A member's role within an organization or team.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Role {
    /// Read-only member.
    Member,
    /// Can manage projects and invite members.
    Admin,
    /// Full control, including billing and deletion.
    Owner,
}

impl Role {
    pub fn can_manage_members(self) -> bool {
        matches!(self, Role::Admin | Role::Owner)
    }
    pub fn can_delete_org(self) -> bool {
        matches!(self, Role::Owner)
    }
}

/// Access level a principal holds on a specific project/workspace. Mirrors the
/// collaboration crate's `AccessLevel` so the platform and live-editing share
/// one permission vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Permission {
    /// May view the project but not edit.
    Viewer,
    /// May view and edit the project.
    Editor,
    /// May edit and manage sharing/settings.
    Owner,
}

impl Permission {
    pub fn can_view(self) -> bool {
        true
    }
    pub fn can_edit(self) -> bool {
        matches!(self, Permission::Editor | Permission::Owner)
    }
    pub fn can_manage(self) -> bool {
        matches!(self, Permission::Owner)
    }
    /// The higher (more capable) of two permissions.
    pub fn max(self, other: Permission) -> Permission {
        if self >= other {
            self
        } else {
            other
        }
    }
}
