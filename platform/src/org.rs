//! Organizations, teams, and membership.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::id::{OrgId, TeamId, UserId};
use crate::permission::Role;

/// An organization: the top-level tenant that owns projects and members.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Organization {
    pub id: OrgId,
    pub name: String,
    /// Member roles keyed by user id.
    pub members: BTreeMap<UserId, Role>,
    pub teams: Vec<TeamId>,
}

impl Organization {
    pub fn new(id: OrgId, name: impl Into<String>, owner: UserId) -> Self {
        let mut members = BTreeMap::new();
        members.insert(owner, Role::Owner);
        Organization {
            id,
            name: name.into(),
            members,
            teams: Vec::new(),
        }
    }

    pub fn role_of(&self, user: &UserId) -> Option<Role> {
        self.members.get(user).copied()
    }

    pub fn is_member(&self, user: &UserId) -> bool {
        self.members.contains_key(user)
    }

    /// Add or update a member's role.
    pub fn set_member(&mut self, user: UserId, role: Role) {
        self.members.insert(user, role);
    }

    pub fn remove_member(&mut self, user: &UserId) -> bool {
        self.members.remove(user).is_some()
    }
}

/// A team: a named subset of an organization's members.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Team {
    pub id: TeamId,
    pub org: OrgId,
    pub name: String,
    pub members: Vec<UserId>,
}

impl Team {
    pub fn new(id: TeamId, org: OrgId, name: impl Into<String>) -> Self {
        Team {
            id,
            org,
            name: name.into(),
            members: Vec::new(),
        }
    }

    pub fn add_member(&mut self, user: UserId) {
        if !self.members.contains(&user) {
            self.members.push(user);
        }
    }

    pub fn remove_member(&mut self, user: &UserId) {
        self.members.retain(|u| u != user);
    }

    pub fn contains(&self, user: &UserId) -> bool {
        self.members.contains(user)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_is_seeded_and_roles_update() {
        let owner = UserId::new("u1");
        let mut org = Organization::new(OrgId::new("o1"), "Acme", owner.clone());
        assert_eq!(org.role_of(&owner), Some(Role::Owner));

        let bob = UserId::new("u2");
        org.set_member(bob.clone(), Role::Member);
        assert_eq!(org.role_of(&bob), Some(Role::Member));
        assert!(org.remove_member(&bob));
        assert!(!org.is_member(&bob));
    }

    #[test]
    fn team_membership_is_deduped() {
        let mut team = Team::new(TeamId::new("t1"), OrgId::new("o1"), "Mechanical");
        let u = UserId::new("u1");
        team.add_member(u.clone());
        team.add_member(u.clone());
        assert_eq!(team.members.len(), 1);
        assert!(team.contains(&u));
    }
}
