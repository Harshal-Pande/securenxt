//! Branch management operations.
//!
//! Provides: `list_branches`, `create_branch`, `delete_branch`, `checkout_branch`.

use git2::{BranchType, Repository};

use crate::error::{GitEngineError, Result};
use crate::types::BranchInfo;
use crate::utils::object_id_from_oid;

// ─────────────────────────────────────────────────────────────────────────────
// Public operations
// ─────────────────────────────────────────────────────────────────────────────

/// List all local (and optionally remote-tracking) branches in the repository.
///
/// # Arguments
/// * `include_remote` — when `true`, remote-tracking branches are included in
///   the result alongside local branches.
pub(crate) fn list_branches(
    repo: &Repository,
    include_remote: bool,
) -> Result<Vec<BranchInfo>> {
    let branch_type = if include_remote {
        None // None = list all (local + remote)
    } else {
        Some(BranchType::Local)
    };

    let head_ref = repo.head().ok();
    let head_target = head_ref.as_ref().and_then(|r| r.target());

    let mut branches = Vec::new();

    for branch_result in repo.branches(branch_type)? {
        let (branch, btype) = branch_result?;

        let name = match branch.name()? {
            Some(n) => n.to_string(),
            None => continue, // Skip branches with non-UTF-8 names.
        };

        let is_remote = btype == BranchType::Remote;

        let commit_oid = match branch.get().target() {
            Some(oid) => oid,
            None => continue,
        };

        let upstream = if !is_remote {
            branch
                .upstream()
                .ok()
                .and_then(|u| u.name().ok().flatten().map(String::from))
        } else {
            None
        };

        let is_head = head_target.map_or(false, |h| h == commit_oid);

        branches.push(BranchInfo {
            name,
            is_remote,
            upstream,
            commit_id: object_id_from_oid(commit_oid),
            is_head,
        });
    }

    Ok(branches)
}

/// Create a new local branch pointing at the current HEAD commit.
///
/// Returns `Err(GitEngineError::AlreadyExists)` if a branch with that name
/// already exists.  Returns `Err(GitEngineError::CommitError)` if the
/// repository has no commits yet.
pub(crate) fn create_branch(repo: &Repository, name: &str) -> Result<BranchInfo> {
    // Validate the branch name using git2's reference name validator.
    // A valid branch ref takes the form refs/heads/<name>.
    if !git2::Reference::is_valid_name(&format!("refs/heads/{name}")) {
        return Err(GitEngineError::InvalidReference(
            name.to_string(),
            "not a valid branch name".to_string(),
        ));
    }

    // Check for existing branch.
    if repo.find_branch(name, BranchType::Local).is_ok() {
        return Err(GitEngineError::AlreadyExists(format!(
            "branch '{name}' already exists"
        )));
    }

    // Resolve HEAD to a commit.
    let head = repo.head().map_err(|_| {
        GitEngineError::CommitError(
            "cannot create branch on an empty repository (no commits yet)".to_string(),
        )
    })?;

    let commit = head.peel_to_commit()?;

    let branch = repo.branch(name, &commit, false)?;

    let commit_oid = branch.get().target().ok_or_else(|| {
        GitEngineError::BranchError(format!("branch '{name}' has no target commit"))
    })?;

    Ok(BranchInfo {
        name: name.to_string(),
        is_remote: false,
        upstream: None,
        commit_id: object_id_from_oid(commit_oid),
        is_head: false,
    })
}

/// Delete a local branch by name.
///
/// Returns `Err(GitEngineError::BranchError)` when attempting to delete the
/// currently checked-out branch.
pub(crate) fn delete_branch(repo: &Repository, name: &str) -> Result<()> {
    let mut branch = repo
        .find_branch(name, BranchType::Local)
        .map_err(|_| GitEngineError::NotFound(format!("branch '{name}' not found")))?;

    if branch.is_head() {
        return Err(GitEngineError::BranchError(format!(
            "cannot delete the currently checked-out branch '{name}'"
        )));
    }

    branch.delete()?;
    Ok(())
}

/// Check out a local branch, updating HEAD and the working tree.
///
/// This is a "safe" checkout — it refuses to proceed when the working tree is
/// dirty, returning `Err(GitEngineError::DirtyWorkingTree)`.  The caller can
/// use [`crate::operations::index::stage_all`] + [`crate::operations::index::commit`]
/// or inspect the status first.
pub(crate) fn checkout_branch(repo: &Repository, name: &str) -> Result<()> {
    // Locate the branch.
    let branch = repo
        .find_branch(name, BranchType::Local)
        .map_err(|_| GitEngineError::NotFound(format!("branch '{name}' not found")))?;

    let branch_ref = branch.into_reference();

    // Peel to tree so we can build a CheckoutBuilder.
    let tree = branch_ref.peel(git2::ObjectType::Tree)?;

    // Build a strict checkout — fail on conflicts.
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.safe();

    // Perform the checkout.
    repo.checkout_tree(&tree, Some(&mut checkout))
        .map_err(|e| {
            if e.code() == git2::ErrorCode::Conflict {
                GitEngineError::DirtyWorkingTree
            } else {
                GitEngineError::from_git2(e)
            }
        })?;

    // Move HEAD.
    let branch_name = branch_ref.name().ok_or_else(|| {
        GitEngineError::BranchError("branch reference has a non-UTF-8 name".to_string())
    })?;
    repo.set_head(branch_name)?;

    Ok(())
}

/// Return metadata for the currently checked-out branch.
///
/// Returns `None` when in detached HEAD state.
pub(crate) fn current_branch(repo: &Repository) -> Result<Option<BranchInfo>> {
    let head = match repo.head() {
        Ok(h) => h,
        Err(e) if e.code() == git2::ErrorCode::UnbornBranch => return Ok(None),
        Err(e) => return Err(e.into()),
    };

    if head.is_branch() {
        let name = head
            .shorthand()
            .ok_or_else(|| {
                GitEngineError::BranchError("current branch has a non-UTF-8 name".to_string())
            })?
            .to_string();

        let commit_oid = head.target().ok_or_else(|| {
            GitEngineError::BranchError("HEAD has no target commit".to_string())
        })?;

        Ok(Some(BranchInfo {
            name,
            is_remote: false,
            upstream: None,
            commit_id: object_id_from_oid(commit_oid),
            is_head: true,
        }))
    } else {
        // Detached HEAD — no branch.
        Ok(None)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::index::stage_all;
    use std::fs;
    use tempfile::TempDir;

    /// Create a bare-minimum repo with one commit so branch operations work.
    fn repo_with_commit() -> (TempDir, Repository) {
        let tmp = TempDir::new().unwrap();
        let repo = Repository::init(tmp.path()).unwrap();

        // Write a file and stage it.
        fs::write(tmp.path().join("README.md"), b"hello").unwrap();
        stage_all(&repo).unwrap();

        // Commit using a fixed signature — scope `tree` so it drops before `repo` is moved.
        let sig = git2::Signature::now("Test", "test@example.com").unwrap();
        {
            let tree_id = repo.index().unwrap().write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
                .unwrap();
        } // `tree` borrow ends here, `repo` can be moved freely.

        (tmp, repo)
    }

    #[test]
    fn list_branches_returns_main_or_master() {
        let (_tmp, repo) = repo_with_commit();
        let branches = list_branches(&repo, false).unwrap();
        assert!(!branches.is_empty(), "expected at least one branch");
        assert!(
            branches.iter().any(|b| b.name == "main" || b.name == "master"),
            "expected a 'main' or 'master' branch, got: {:?}",
            branches.iter().map(|b| &b.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn create_and_list_branch() {
        let (_tmp, repo) = repo_with_commit();
        create_branch(&repo, "feature/hello").unwrap();
        let branches = list_branches(&repo, false).unwrap();
        assert!(branches.iter().any(|b| b.name == "feature/hello"));
    }

    #[test]
    fn create_duplicate_branch_fails() {
        let (_tmp, repo) = repo_with_commit();
        create_branch(&repo, "dup").unwrap();
        let result = create_branch(&repo, "dup");
        assert!(matches!(result, Err(GitEngineError::AlreadyExists(_))));
    }

    #[test]
    fn delete_non_current_branch() {
        let (_tmp, repo) = repo_with_commit();
        create_branch(&repo, "to-delete").unwrap();
        delete_branch(&repo, "to-delete").unwrap();
        let branches = list_branches(&repo, false).unwrap();
        assert!(!branches.iter().any(|b| b.name == "to-delete"));
    }

    #[test]
    fn checkout_branch_switches_head() {
        let (_tmp, repo) = repo_with_commit();
        create_branch(&repo, "switch-to").unwrap();
        checkout_branch(&repo, "switch-to").unwrap();
        let current = current_branch(&repo).unwrap().unwrap();
        assert_eq!(current.name, "switch-to");
    }

    #[test]
    fn invalid_branch_name_rejected() {
        let (_tmp, repo) = repo_with_commit();
        // Branch names cannot contain spaces.
        let result = create_branch(&repo, "bad name");
        assert!(matches!(result, Err(GitEngineError::InvalidReference(_, _))));
    }
}
