//! Platform layer for TPT Vertex: accounts, organizations/teams,
//! projects/workspaces, sharing & permissions, and pluggable storage.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! This crate provides the multi-tenant backbone that sits above the geometry
//! kernel and collaboration layers. It is deliberately storage-agnostic: all
//! persistence goes through the [`storage::Store`]/[`storage::BlobStore`] traits,
//! with [`storage::MemoryStore`] as the reference implementation.
//!
//! The [`Platform`] façade wires the pieces together: sign up / log in, create
//! organizations, teams and projects, share projects, and resolve a user's
//! effective permission on a project (combining explicit, team, and org grants —
//! see [`project::Project::effective_permission`]).

pub mod auth;
pub mod id;
pub mod org;
pub mod permission;
pub mod project;
pub mod storage;

pub use auth::{AuthError, Session, User};
pub use id::{OrgId, ProjectId, SessionId, TeamId, UserId};
pub use org::{Organization, Team};
pub use permission::{Permission, Role};
pub use project::{Project, ProjectStatus, ShareTarget};
pub use storage::{BlobStore, MemoryStore, Store, StoreError};

/// High-level platform operations over a [`Store`]. Ids are supplied by the
/// caller (which owns id generation) to keep this crate free of RNG/uuid deps.
pub struct Platform<S: Store> {
    pub store: S,
}

impl<S: Store> Platform<S> {
    pub fn new(store: S) -> Self {
        Platform { store }
    }

    /// Register a new account. Fails if the email is taken or the password is
    /// too weak.
    pub fn sign_up(
        &mut self,
        id: UserId,
        email: &str,
        display_name: &str,
        password: &str,
        salt: &str,
    ) -> Result<User, AuthError> {
        if !auth::is_acceptable_password(password) {
            return Err(AuthError::WeakPassword);
        }
        if self.store.user_by_email(email).is_some() {
            return Err(AuthError::EmailTaken);
        }
        let user = User {
            id,
            email: email.to_string(),
            display_name: display_name.to_string(),
            password_hash: auth::hash_password(password, salt),
        };
        self.store.put_user(user.clone()).map_err(AuthError::from)?;
        Ok(user)
    }

    /// Authenticate and open a session.
    pub fn log_in(
        &mut self,
        session_id: SessionId,
        email: &str,
        password: &str,
    ) -> Result<Session, AuthError> {
        let user = self
            .store
            .user_by_email(email)
            .ok_or(AuthError::InvalidCredentials)?;
        if !auth::verify_password(password, &user.password_hash) {
            return Err(AuthError::InvalidCredentials);
        }
        let session = Session {
            id: session_id,
            user: user.id.clone(),
        };
        self.store
            .put_session(session.clone())
            .map_err(AuthError::from)?;
        Ok(session)
    }

    /// Resolve the user behind a session token.
    pub fn authenticate(&self, session: &SessionId) -> Option<User> {
        let s = self.store.session(session)?;
        self.store.user(&s.user)
    }

    /// Log out (invalidate a session).
    pub fn log_out(&mut self, session: &SessionId) {
        let _ = self.store.delete_session(session);
    }

    /// Create an organization owned by `owner`.
    pub fn create_org(
        &mut self,
        id: OrgId,
        name: &str,
        owner: UserId,
    ) -> Result<Organization, StoreError> {
        let org = Organization::new(id, name, owner);
        self.store.put_org(org.clone())?;
        Ok(org)
    }

    /// Create a project within an org, owned by `creator`.
    pub fn create_project(
        &mut self,
        id: ProjectId,
        org: OrgId,
        name: &str,
        creator: UserId,
    ) -> Result<Project, StoreError> {
        let project = Project::new(id, org, name, creator);
        self.store.put_project(project.clone())?;
        Ok(project)
    }

    /// Share a project with a user or team.
    pub fn share_project(
        &mut self,
        project: &ProjectId,
        target: ShareTarget,
        level: Permission,
    ) -> Result<(), StoreError> {
        let mut p = self.store.project(project).ok_or(StoreError::NotFound)?;
        p.share(target, level);
        self.store.put_project(p)
    }

    /// Compute a user's effective permission on a project.
    pub fn permission_for(&self, project: &ProjectId, user: &UserId) -> Option<Permission> {
        let p = self.store.project(project)?;
        let org = self.store.org(&p.org)?;
        let teams = self.store.teams_of_org(&p.org);
        let team_refs: Vec<&Team> = teams.iter().collect();
        p.effective_permission(user, &org, &team_refs)
    }
}

impl From<StoreError> for AuthError {
    fn from(_: StoreError) -> Self {
        AuthError::NotFound
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn platform() -> Platform<MemoryStore> {
        Platform::new(MemoryStore::new())
    }

    #[test]
    fn sign_up_and_log_in_flow() {
        let mut p = platform();
        p.sign_up(
            UserId::new("u1"),
            "alice@example.com",
            "Alice",
            "password1",
            "s1",
        )
        .unwrap();

        // Duplicate email rejected.
        assert_eq!(
            p.sign_up(
                UserId::new("u2"),
                "alice@example.com",
                "A2",
                "password2",
                "s2"
            ),
            Err(AuthError::EmailTaken)
        );

        // Wrong password rejected; correct succeeds.
        assert_eq!(
            p.log_in(SessionId::new("x"), "alice@example.com", "nope")
                .err(),
            Some(AuthError::InvalidCredentials)
        );
        let session = p
            .log_in(SessionId::new("sess1"), "alice@example.com", "password1")
            .unwrap();
        assert_eq!(
            p.authenticate(&session.id).map(|u| u.id),
            Some(UserId::new("u1"))
        );
        p.log_out(&session.id);
        assert!(p.authenticate(&session.id).is_none());
    }

    #[test]
    fn weak_password_rejected() {
        let mut p = platform();
        assert_eq!(
            p.sign_up(UserId::new("u1"), "a@b.com", "A", "short", "s"),
            Err(AuthError::WeakPassword)
        );
    }

    #[test]
    fn project_sharing_and_permission_resolution() {
        let mut p = platform();
        let owner = UserId::new("u1");
        let bob = UserId::new("u2");
        p.sign_up(owner.clone(), "o@x.com", "Owner", "password1", "s")
            .unwrap();
        p.sign_up(bob.clone(), "b@x.com", "Bob", "password1", "s")
            .unwrap();

        let org = p
            .create_org(OrgId::new("o1"), "Acme", owner.clone())
            .unwrap();
        let proj = p
            .create_project(ProjectId::new("p1"), org.id.clone(), "Arm", owner.clone())
            .unwrap();

        // Bob has no access yet.
        assert_eq!(p.permission_for(&proj.id, &bob), None);

        // Share with Bob as editor.
        p.share_project(&proj.id, ShareTarget::User(bob.clone()), Permission::Editor)
            .unwrap();
        assert_eq!(p.permission_for(&proj.id, &bob), Some(Permission::Editor));

        // Owner retains owner permission.
        assert_eq!(p.permission_for(&proj.id, &owner), Some(Permission::Owner));
    }
}
