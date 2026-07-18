//! Pluggable storage backend for platform entities and project assets.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! The [`Store`] trait abstracts persistence so the platform can run against an
//! in-memory store (tests, single-node), a database, or object storage without
//! changing business logic. [`MemoryStore`] is the reference implementation.
//! Binary project assets (evaluated meshes, exports) go through the separate
//! [`BlobStore`] trait, which a production deployment backs with S3/GCS.

use std::collections::HashMap;

use crate::auth::{Session, User};
use crate::id::{OrgId, ProjectId, SessionId, TeamId, UserId};
use crate::org::{Organization, Team};
use crate::project::Project;

/// Errors from the storage layer.
#[derive(Debug, Clone, PartialEq)]
pub enum StoreError {
    NotFound,
    Conflict,
    Backend(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::NotFound => f.write_str("not found"),
            StoreError::Conflict => f.write_str("conflict"),
            StoreError::Backend(m) => write!(f, "backend error: {m}"),
        }
    }
}

impl std::error::Error for StoreError {}

/// Metadata/entity persistence.
pub trait Store {
    fn put_user(&mut self, user: User) -> Result<(), StoreError>;
    fn user(&self, id: &UserId) -> Option<User>;
    fn user_by_email(&self, email: &str) -> Option<User>;

    fn put_session(&mut self, session: Session) -> Result<(), StoreError>;
    fn session(&self, id: &SessionId) -> Option<Session>;
    fn delete_session(&mut self, id: &SessionId) -> Result<(), StoreError>;

    fn put_org(&mut self, org: Organization) -> Result<(), StoreError>;
    fn org(&self, id: &OrgId) -> Option<Organization>;

    fn put_team(&mut self, team: Team) -> Result<(), StoreError>;
    fn team(&self, id: &TeamId) -> Option<Team>;
    fn teams_of_org(&self, org: &OrgId) -> Vec<Team>;

    fn put_project(&mut self, project: Project) -> Result<(), StoreError>;
    fn project(&self, id: &ProjectId) -> Option<Project>;
    fn projects_of_org(&self, org: &OrgId) -> Vec<Project>;
}

/// Binary asset persistence (content-addressed blobs).
pub trait BlobStore {
    fn put_blob(&mut self, key: &str, bytes: Vec<u8>) -> Result<(), StoreError>;
    fn blob(&self, key: &str) -> Option<Vec<u8>>;
}

/// In-memory reference implementation of [`Store`] + [`BlobStore`].
#[derive(Debug, Default)]
pub struct MemoryStore {
    users: HashMap<UserId, User>,
    email_index: HashMap<String, UserId>,
    sessions: HashMap<SessionId, Session>,
    orgs: HashMap<OrgId, Organization>,
    teams: HashMap<TeamId, Team>,
    projects: HashMap<ProjectId, Project>,
    blobs: HashMap<String, Vec<u8>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        MemoryStore::default()
    }
}

impl Store for MemoryStore {
    fn put_user(&mut self, user: User) -> Result<(), StoreError> {
        self.email_index
            .insert(user.email.to_lowercase(), user.id.clone());
        self.users.insert(user.id.clone(), user);
        Ok(())
    }
    fn user(&self, id: &UserId) -> Option<User> {
        self.users.get(id).cloned()
    }
    fn user_by_email(&self, email: &str) -> Option<User> {
        self.email_index
            .get(&email.to_lowercase())
            .and_then(|id| self.users.get(id))
            .cloned()
    }

    fn put_session(&mut self, session: Session) -> Result<(), StoreError> {
        self.sessions.insert(session.id.clone(), session);
        Ok(())
    }
    fn session(&self, id: &SessionId) -> Option<Session> {
        self.sessions.get(id).cloned()
    }
    fn delete_session(&mut self, id: &SessionId) -> Result<(), StoreError> {
        self.sessions.remove(id);
        Ok(())
    }

    fn put_org(&mut self, org: Organization) -> Result<(), StoreError> {
        self.orgs.insert(org.id.clone(), org);
        Ok(())
    }
    fn org(&self, id: &OrgId) -> Option<Organization> {
        self.orgs.get(id).cloned()
    }

    fn put_team(&mut self, team: Team) -> Result<(), StoreError> {
        self.teams.insert(team.id.clone(), team);
        Ok(())
    }
    fn team(&self, id: &TeamId) -> Option<Team> {
        self.teams.get(id).cloned()
    }
    fn teams_of_org(&self, org: &OrgId) -> Vec<Team> {
        self.teams
            .values()
            .filter(|t| &t.org == org)
            .cloned()
            .collect()
    }

    fn put_project(&mut self, project: Project) -> Result<(), StoreError> {
        self.projects.insert(project.id.clone(), project);
        Ok(())
    }
    fn project(&self, id: &ProjectId) -> Option<Project> {
        self.projects.get(id).cloned()
    }
    fn projects_of_org(&self, org: &OrgId) -> Vec<Project> {
        self.projects
            .values()
            .filter(|p| &p.org == org)
            .cloned()
            .collect()
    }
}

impl BlobStore for MemoryStore {
    fn put_blob(&mut self, key: &str, bytes: Vec<u8>) -> Result<(), StoreError> {
        self.blobs.insert(key.to_string(), bytes);
        Ok(())
    }
    fn blob(&self, key: &str) -> Option<Vec<u8>> {
        self.blobs.get(key).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::hash_password;

    #[test]
    fn user_round_trips_and_email_lookup() {
        let mut s = MemoryStore::new();
        let u = User {
            id: UserId::new("u1"),
            email: "Alice@Example.com".into(),
            display_name: "Alice".into(),
            password_hash: hash_password("password1", "salt"),
        };
        s.put_user(u.clone()).unwrap();
        assert_eq!(s.user(&UserId::new("u1")), Some(u.clone()));
        // Email lookup is case-insensitive.
        assert_eq!(s.user_by_email("alice@example.com"), Some(u));
    }

    #[test]
    fn blob_round_trips() {
        let mut s = MemoryStore::new();
        s.put_blob("k", vec![1, 2, 3]).unwrap();
        assert_eq!(s.blob("k"), Some(vec![1, 2, 3]));
        assert_eq!(s.blob("missing"), None);
    }
}
