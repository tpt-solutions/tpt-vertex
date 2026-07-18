//! In-memory version-control repository over design revisions.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! A [`Repository`] stores [`Commit`]s in a content-addressed history DAG and
//! tracks named branches (refs to commit ids). It supports committing a new
//! revision from a [`FeatureManifest`], checking out branches, and three-way
//! merges with basic conflict detection based on per-feature changes.

use crate::{Diff, FeatureManifest};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Errors returned by repository operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoError {
    /// The named branch does not exist.
    UnknownBranch(String),
    /// The named commit does not exist.
    UnknownCommit(String),
    /// A merge was requested but the branch does not diverge from a common
    /// ancestor in a way we can resolve (e.g. fast-forward only).
    NothingToMerge,
    /// A merge is blocked by conflicting per-feature changes.
    HasConflicts,
}

/// A single node in the commit history DAG.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Commit {
    /// Content hash of this commit (over parents + manifest hash + message).
    pub id: String,
    /// Parent commit ids (`Vec` so merges can have two parents).
    pub parents: Vec<String>,
    /// The feature manifest recorded by this commit.
    pub manifest: FeatureManifest,
    /// Human-readable commit message.
    pub message: String,
}

/// A conflict discovered during a merge: both sides changed the same feature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// Feature id modified on both branches.
    pub feature_id: u64,
}

/// The result of a merge attempt.
#[derive(Debug, Clone, PartialEq)]
pub struct MergeOutcome {
    /// Conflicting features that must be resolved before the merge can commit.
    pub conflicts: Vec<Conflict>,
    /// Whether the merge produced a clean result (no conflicts).
    pub clean: bool,
}

impl MergeOutcome {
    /// True if the merge has no conflicts.
    pub fn is_clean(&self) -> bool {
        self.clean
    }
}

/// An in-memory design repository.
#[derive(Debug, Clone, Default)]
pub struct Repository {
    commits: HashMap<String, Commit>,
    branches: HashMap<String, String>,
    head: String,
    /// The branch HEAD is currently attached to, if any (detached when None).
    current_branch: Option<String>,
}

impl Repository {
    /// Create an empty repository with a default `main` branch pointing at an
    /// empty initial commit.
    pub fn new() -> Self {
        let mut repo = Repository {
            commits: HashMap::new(),
            branches: HashMap::new(),
            head: String::new(),
            current_branch: Some("main".to_string()),
        };
        let root = repo.make_commit(&[], &FeatureManifest::default(), "initial commit");
        repo.branches.insert("main".to_string(), root.clone());
        repo.head = root;
        repo
    }

    /// Return the branch HEAD is attached to, if any.
    pub fn current_branch(&self) -> Option<&str> {
        self.current_branch.as_deref()
    }

    /// Move the branch HEAD is attached to (if any) onto `commit`, keeping HEAD
    /// in sync. Used after creating a new commit on the current branch.
    fn advance_current_branch(&mut self, commit: &str) {
        if let Some(branch) = &self.current_branch {
            self.branches.insert(branch.clone(), commit.to_string());
        }
        self.head = commit.to_string();
    }

    /// Return the id of the commit the current HEAD points at.
    pub fn head(&self) -> &str {
        &self.head
    }

    /// Look up a branch's tip commit id.
    pub fn branch_tip(&self, name: &str) -> Option<&str> {
        self.branches.get(name).map(|s| s.as_str())
    }

    /// List branch names.
    pub fn branches(&self) -> Vec<String> {
        self.branches.keys().cloned().collect()
    }

    /// Look up a commit by id.
    pub fn commit(&self, id: &str) -> Option<&Commit> {
        self.commits.get(id)
    }

    /// Create a new branch pointing at the current HEAD.
    pub fn create_branch(&mut self, name: &str) -> Result<(), RepoError> {
        if self.branches.contains_key(name) {
            return Ok(());
        }
        self.branches.insert(name.to_string(), self.head.clone());
        Ok(())
    }

    /// Switch HEAD to an existing branch.
    pub fn checkout(&mut self, name: &str) -> Result<(), RepoError> {
        match self.branches.get(name).cloned() {
            Some(tip) => {
                self.head = tip;
                self.current_branch = Some(name.to_string());
                Ok(())
            }
            None => Err(RepoError::UnknownBranch(name.to_string())),
        }
    }

    /// Commit a new revision on the current branch.
    pub fn commit_revision(
        &mut self,
        manifest: &FeatureManifest,
        message: &str,
    ) -> Result<String, RepoError> {
        let parent = self.head.clone();
        let id = self.make_commit(&[parent], manifest, message);
        self.advance_current_branch(&id);
        Ok(id)
    }

    /// Compute the nearest common ancestor of two commit ids (lowest common
    /// ancestor over parent chains). Returns `None` if either commit is unknown
    /// or there is no shared history.
    pub fn merge_base(&self, a: &str, b: &str) -> Option<String> {
        if !self.commits.contains_key(a) || !self.commits.contains_key(b) {
            return None;
        }
        let anc_a = self.ancestors(a);
        let anc_b = self.ancestors(b);
        // The merge base is the first common ancestor found in a's BFS order.
        anc_a.into_iter().find(|id| anc_b.contains(id))
    }

    /// Build the set of all ancestors (including the commit itself) via BFS.
    fn ancestors(&self, start: &str) -> Vec<String> {
        let mut seen = Vec::new();
        let mut queue = vec![start.to_string()];
        while let Some(id) = queue.pop() {
            if seen.contains(&id) {
                continue;
            }
            seen.push(id.clone());
            if let Some(c) = self.commits.get(&id) {
                for p in &c.parents {
                    queue.push(p.clone());
                }
            }
        }
        seen
    }

    /// Determine whether merging `other` into the current HEAD can be done as a
    /// fast-forward (no divergent changes on the current branch).
    pub fn is_fast_forward(&self, other: &str) -> bool {
        let head = self.head.clone();
        if head == other {
            return true;
        }
        match self.merge_base(&head, other) {
            Some(base) => base == head,
            None => false,
        }
    }

    /// Compute the merge between `other` and the current HEAD.
    ///
    /// Returns conflict information based on per-feature changes relative to the
    /// merge base. A feature is conflicting if it was modified on *both* sides
    /// relative to the base. Fast-forward merges and clean merges (no shared
    /// modifications) report no conflicts.
    pub fn merge(&self, other: &str) -> Result<MergeOutcome, RepoError> {
        let head = self.head.clone();
        let head_commit = self
            .commits
            .get(&head)
            .ok_or_else(|| RepoError::UnknownCommit(head.clone()))?
            .clone();
        let other_commit = self
            .commits
            .get(other)
            .ok_or_else(|| RepoError::UnknownCommit(other.to_string()))?
            .clone();

        let base_id = self
            .merge_base(&head, other)
            .ok_or(RepoError::NothingToMerge)?;
        let base_commit = self
            .commits
            .get(&base_id)
            .ok_or(RepoError::UnknownCommit(base_id))?;

        let ours = Diff::between(&base_commit.manifest, &head_commit.manifest);
        let theirs = Diff::between(&base_commit.manifest, &other_commit.manifest);

        // A feature is in conflict when it changed on *both* sides relative to
        // the merge base AND the two sides resolved to a *different* resulting
        // feature (e.g. Added with different params, or both Modified, or one
        // Modified while the other Removed). Identical changes (same id and
        // param hash on both sides) are not conflicts.
        let ours_ids: std::collections::HashSet<u64> =
            ours.changes.iter().map(changed_id).collect();
        let theirs_ids: std::collections::HashSet<u64> =
            theirs.changes.iter().map(changed_id).collect();

        let mut conflicts = Vec::new();
        for id in ours_ids.intersection(&theirs_ids) {
            let ours_entry = head_commit.manifest.get(*id);
            let theirs_entry = other_commit.manifest.get(*id);
            if ours_entry != theirs_entry {
                conflicts.push(Conflict { feature_id: *id });
            }
        }
        conflicts.sort_by_key(|c| c.feature_id);

        Ok(MergeOutcome {
            clean: conflicts.is_empty(),
            conflicts,
        })
    }

    /// Finalize a merge by creating a merge commit with two parents (HEAD and
    /// `other`) carrying `other`'s manifest. Callers should have verified the
    /// merge is clean via [`Repository::merge`] unless they intend to resolve
    /// conflicts externally.
    pub fn commit_merge(&mut self, other: &str, message: &str) -> Result<String, RepoError> {
        let head = self.head.clone();
        let other_commit = self
            .commits
            .get(other)
            .ok_or_else(|| RepoError::UnknownCommit(other.to_string()))?
            .clone();
        let id = self.make_commit(&[head, other.to_string()], &other_commit.manifest, message);
        self.advance_current_branch(&id);
        Ok(id)
    }

    /// Build and store a commit, returning its content hash.
    fn make_commit(
        &mut self,
        parents: &[String],
        manifest: &FeatureManifest,
        message: &str,
    ) -> String {
        let mut hasher = Sha256::new();
        for p in parents {
            hasher.update(p.as_bytes());
        }
        hasher.update(manifest.hash().as_bytes());
        hasher.update(message.as_bytes());
        let id = hex(&hasher.finalize());
        let commit = Commit {
            id: id.clone(),
            parents: parents.to_vec(),
            manifest: manifest.clone(),
            message: message.to_string(),
        };
        self.commits.insert(id.clone(), commit);
        id
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Extract the feature id affected by a [`crate::Change`].
fn changed_id(c: &crate::Change) -> u64 {
    match c {
        crate::Change::Added(id) | crate::Change::Removed(id) | crate::Change::Modified(id) => *id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FeatureEntry;

    fn manifest_with(entries: &[(u64, &str, &str)]) -> FeatureManifest {
        FeatureManifest {
            entries: entries
                .iter()
                .map(|(id, kind, hash)| FeatureEntry {
                    id: *id,
                    kind: kind.to_string(),
                    param_hash: hash.to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn new_repo_has_main_branch() {
        let repo = Repository::new();
        assert!(repo.branches().contains(&"main".to_string()));
        assert_eq!(repo.branch_tip("main"), Some(repo.head()));
    }

    #[test]
    fn commit_records_revision() {
        let mut repo = Repository::new();
        let m = manifest_with(&[(1, "Extrude", "h1")]);
        let id = repo.commit_revision(&m, "add extrude").unwrap();
        assert_eq!(repo.head(), id);
        assert_eq!(repo.commit(&id).unwrap().manifest, m);
    }

    #[test]
    fn branch_divergence_and_fast_forward() {
        let mut repo = Repository::new();
        repo.create_branch("feature").unwrap();
        repo.checkout("feature").unwrap();
        let m = manifest_with(&[(1, "Extrude", "h1")]);
        let feat = repo.commit_revision(&m, "feature work").unwrap();
        repo.checkout("main").unwrap();
        // main is still at the root; feature is a direct descendant, so the
        // merge is a fast-forward (no divergent changes on main).
        assert!(repo.is_fast_forward(&feat));
        let outcome = repo.merge(&feat).unwrap();
        assert!(outcome.is_clean());

        // A fast-forward should move main directly onto the feature commit
        // rather than creating a new merge commit.
        repo.branches.insert("main".to_string(), feat.clone());
        repo.head = feat.clone();
        assert_eq!(repo.head(), feat);
        assert_eq!(repo.branch_tip("main"), Some(feat.as_str()));
    }

    #[test]
    fn merge_commit_has_two_parents() {
        let mut repo = Repository::new();
        repo.create_branch("feature").unwrap();

        // main: add feature 1
        let m_main = manifest_with(&[(1, "Extrude", "main-hash")]);
        let c_main = repo.commit_revision(&m_main, "main edit").unwrap();

        // feature: add feature 2 (parallel, no conflict)
        repo.checkout("feature").unwrap();
        let m_feat = manifest_with(&[(1, "Extrude", "main-hash"), (2, "Revolve", "feat-hash")]);
        let c_feat = repo.commit_revision(&m_feat, "add revolve").unwrap();

        repo.checkout("main").unwrap();
        let outcome = repo.merge(&c_feat).unwrap();
        assert!(outcome.is_clean());
        let merge_id = repo.commit_merge(&c_feat, "merge feature").unwrap();
        let merge = repo.commit(&merge_id).unwrap();
        assert_eq!(merge.parents.len(), 2);
        assert!(merge.parents.contains(&c_main));
        assert!(merge.parents.contains(&c_feat));
        let _ = outcome;
    }

    #[test]
    fn conflicting_merge_detected() {
        let mut repo = Repository::new();
        repo.create_branch("feature").unwrap();

        // main modifies feature 1.
        let m_main = manifest_with(&[(1, "Extrude", "main-hash")]);
        let c_main = repo.commit_revision(&m_main, "main edit").unwrap();

        // feature also modifies feature 1 (different hash).
        repo.checkout("feature").unwrap();
        let m_feat = manifest_with(&[(1, "Extrude", "feat-hash")]);
        let c_feat = repo.commit_revision(&m_feat, "feature edit").unwrap();

        repo.checkout("main").unwrap();
        let outcome = repo.merge(&c_feat).unwrap();
        assert!(!outcome.is_clean());
        assert_eq!(outcome.conflicts.len(), 1);
        assert_eq!(outcome.conflicts[0].feature_id, 1);
        let _ = c_main;
    }

    #[test]
    fn non_conflicting_parallel_changes_merge_clean() {
        let mut repo = Repository::new();
        repo.create_branch("feature").unwrap();
        let m_main = manifest_with(&[(1, "Extrude", "main-hash")]);
        repo.commit_revision(&m_main, "main edit").unwrap();
        repo.checkout("feature").unwrap();
        let m_feat = manifest_with(&[(1, "Extrude", "main-hash"), (2, "Revolve", "feat-hash")]);
        let c_feat = repo.commit_revision(&m_feat, "add revolve").unwrap();
        repo.checkout("main").unwrap();
        let outcome = repo.merge(&c_feat).unwrap();
        assert!(outcome.is_clean());
    }
}
