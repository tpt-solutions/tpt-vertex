//! Projects/workspaces plus sharing and permission resolution.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::id::{OrgId, ProjectId, TeamId, UserId};
use crate::org::{Organization, Team};
use crate::permission::Permission;

/// Lifecycle state of a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectStatus {
    Active,
    Archived,
}

/// A share grant target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShareTarget {
    User(UserId),
    Team(TeamId),
}

/// A project (workspace): the unit that owns a design document/version history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    pub org: OrgId,
    pub name: String,
    pub status: ProjectStatus,
    /// Baseline visibility for org members who have no explicit grant.
    pub org_default: Option<Permission>,
    /// Explicit per-user grants.
    pub user_grants: BTreeMap<UserId, Permission>,
    /// Explicit per-team grants.
    pub team_grants: BTreeMap<TeamId, Permission>,
}

impl Project {
    pub fn new(id: ProjectId, org: OrgId, name: impl Into<String>, creator: UserId) -> Self {
        let mut user_grants = BTreeMap::new();
        user_grants.insert(creator, Permission::Owner);
        Project {
            id,
            org,
            name: name.into(),
            status: ProjectStatus::Active,
            org_default: None,
            user_grants,
            team_grants: BTreeMap::new(),
        }
    }

    pub fn archive(&mut self) {
        self.status = ProjectStatus::Archived;
    }

    pub fn unarchive(&mut self) {
        self.status = ProjectStatus::Active;
    }

    /// Share with a user or team at a given permission level.
    pub fn share(&mut self, target: ShareTarget, level: Permission) {
        match target {
            ShareTarget::User(u) => {
                self.user_grants.insert(u, level);
            }
            ShareTarget::Team(t) => {
                self.team_grants.insert(t, level);
            }
        }
    }

    /// Revoke a share grant.
    pub fn unshare(&mut self, target: &ShareTarget) {
        match target {
            ShareTarget::User(u) => {
                self.user_grants.remove(u);
            }
            ShareTarget::Team(t) => {
                self.team_grants.remove(t);
            }
        }
    }

    /// Resolve the effective permission a user has, combining: explicit user
    /// grant, any team grants they belong to, and the org default (if they are
    /// an org member). Returns the highest applicable permission, or `None`.
    pub fn effective_permission(
        &self,
        user: &UserId,
        org: &Organization,
        teams: &[&Team],
    ) -> Option<Permission> {
        let mut best: Option<Permission> = None;

        if let Some(p) = self.user_grants.get(user).copied() {
            best = Some(best.map_or(p, |b| b.max(p)));
        }
        for team in teams {
            if team.contains(user) {
                if let Some(p) = self.team_grants.get(&team.id).copied() {
                    best = Some(best.map_or(p, |b| b.max(p)));
                }
            }
        }
        if org.is_member(user) {
            if let Some(p) = self.org_default {
                best = Some(best.map_or(p, |b| b.max(p)));
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::Role;

    #[test]
    fn creator_is_owner() {
        let creator = UserId::new("u1");
        let p = Project::new(
            ProjectId::new("p1"),
            OrgId::new("o1"),
            "Robot Arm",
            creator.clone(),
        );
        assert_eq!(p.user_grants.get(&creator), Some(&Permission::Owner));
        assert_eq!(p.status, ProjectStatus::Active);
    }

    #[test]
    fn effective_permission_takes_highest() {
        let owner = UserId::new("u1");
        let bob = UserId::new("u2");
        let mut org = Organization::new(OrgId::new("o1"), "Acme", owner.clone());
        org.set_member(bob.clone(), Role::Member);

        let mut team = Team::new(TeamId::new("t1"), OrgId::new("o1"), "Mech");
        team.add_member(bob.clone());

        let mut proj = Project::new(ProjectId::new("p1"), OrgId::new("o1"), "Arm", owner);
        proj.org_default = Some(Permission::Viewer);
        proj.share(ShareTarget::Team(team.id.clone()), Permission::Editor);

        let eff = proj.effective_permission(&bob, &org, &[&team]).unwrap();
        // Team grant (Editor) beats org default (Viewer).
        assert_eq!(eff, Permission::Editor);
        assert!(eff.can_edit());
    }

    #[test]
    fn non_member_has_no_permission() {
        let owner = UserId::new("u1");
        let org = Organization::new(OrgId::new("o1"), "Acme", owner.clone());
        let proj = Project::new(ProjectId::new("p1"), OrgId::new("o1"), "Arm", owner);
        let stranger = UserId::new("u9");
        assert_eq!(proj.effective_permission(&stranger, &org, &[]), None);
    }

    #[test]
    fn archive_toggles_status() {
        let mut proj = Project::new(
            ProjectId::new("p1"),
            OrgId::new("o1"),
            "Arm",
            UserId::new("u1"),
        );
        proj.archive();
        assert_eq!(proj.status, ProjectStatus::Archived);
        proj.unarchive();
        assert_eq!(proj.status, ProjectStatus::Active);
    }
}
