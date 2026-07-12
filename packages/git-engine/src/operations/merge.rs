//! Merge and local-fetch operations.
//!
//! Provides: `merge`, `fetch_local`.

use git2::{AnnotatedCommit, MergeOptions, Repository};

use crate::error::{GitEngineError, Result};
use crate::types::MergeResult;
use crate::utils::{object_id_from_oid, repo_signature};

// ─────────────────────────────────────────────────────────────────────────────
// Public operations
// ─────────────────────────────────────────────────────────────────────────────

/// Merge the named local branch into the currently checked-out branch.
///
/// The function handles three cases automatically:
/// 1. **Already up-to-date** — the target is already an ancestor of HEAD; no
///    action is taken and `Ok(MergeResult::AlreadyUpToDate)` is returned.
/// 2. **Fast-forward** — the current HEAD is an ancestor of the target; HEAD is
///    advanced to the target commit without creating a merge commit.
/// 3. **True merge** — a 3-way merge is performed.  If it succeeds without
///    conflicts a merge commit is created automatically.  If there are conflicts
///    `Err(GitEngineError::MergeConflict)` is returned with the conflict count;
///    the working tree is left in the conflicted state for the caller to resolve.
///
/// Returns `Err(GitEngineError::NotFound)` when `branch_name` does not exist.
pub(crate) fn merge(repo: &Repository, branch_name: &str) -> Result<MergeResult> {
    // Find the target branch and annotate its tip commit.
    let branch = repo
        .find_branch(branch_name, git2::BranchType::Local)
        .map_err(|_| {
            GitEngineError::NotFound(format!("branch '{branch_name}' not found"))
        })?;

    let branch_ref = branch.into_reference();
    let branch_commit_oid = branch_ref.target().ok_or_else(|| {
        GitEngineError::BranchError(format!("branch '{branch_name}' has no target"))
    })?;

    let annotated = repo.find_annotated_commit(branch_commit_oid)?;

    // Analyse what kind of merge is needed.
    let (analysis, _) = repo.merge_analysis(&[&annotated])?;

    if analysis.is_up_to_date() {
        return Ok(MergeResult::AlreadyUpToDate);
    }

    if analysis.is_fast_forward() {
        return do_fast_forward(repo, &annotated, branch_name);
    }

    if analysis.is_normal() {
        return do_three_way_merge(repo, &annotated, branch_name);
    }

    Err(GitEngineError::Other(format!(
        "unexpected merge analysis result for branch '{branch_name}'"
    )))
}

/// Fetch from a local filesystem path, bringing its refs into this repository.
///
/// The remote is added temporarily using the file:// URL convention and removed
/// after fetching so that it does not pollute the repository's remote list.
///
/// `refspecs` may be empty, in which case the default refspec
/// `refs/heads/*:refs/remotes/<remote_name>/*` is used.
///
/// This is the synchronisation primitive used in Phase 3 (Sync) to exchange
/// packfiles between repositories over the Bluetooth transport — the actual
/// transport hands libgit2 a local bare-repo clone of the remote state.
pub(crate) fn fetch_local(
    repo: &Repository,
    source_path: &str,
    remote_name: &str,
    refspecs: &[&str],
) -> Result<()> {
    // Build a file:// URL from the source path.
    let url = if source_path.starts_with("file://") {
        source_path.to_string()
    } else {
        format!("file://{}", source_path.replace('\\', "/"))
    };

    // Add (or update) an anonymous remote.
    let mut remote = match repo.find_remote(remote_name) {
        Ok(r) => r,
        Err(_) => repo.remote(remote_name, &url)?,
    };

    let default_refspec;
    let fetch_refspecs: Vec<&str> = if refspecs.is_empty() {
        default_refspec = format!("+refs/heads/*:refs/remotes/{remote_name}/*");
        vec![&default_refspec]
    } else {
        refspecs.to_vec()
    };

    remote.fetch(&fetch_refspecs, None, None)?;

    // Disconnect the remote object (the remote entry remains in config; callers
    // can delete it via git2::Repository::remote_delete if needed).
    drop(remote);

    Ok(())
}

// MergeResult is defined in crate::types and re-exported from the crate root.
// See types.rs for the variant documentation.

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Advance HEAD to `target` without creating a merge commit.
fn do_fast_forward(
    repo: &Repository,
    target: &AnnotatedCommit<'_>,
    branch_name: &str,
) -> Result<MergeResult> {
    let target_oid = target.id();
    let target_commit = repo.find_commit(target_oid)?;
    let target_tree = target_commit.tree()?;

    // Update the working tree.
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.safe();
    repo.checkout_tree(target_tree.as_object(), Some(&mut checkout))?;

    // Move the branch ref and HEAD.
    match repo.head() {
        Ok(head_ref) => {
            let head_name = head_ref.name().ok_or_else(|| {
                GitEngineError::BranchError(
                    "HEAD reference has a non-UTF-8 name".to_string(),
                )
            })?;
            let mut branch_ref = repo.find_reference(head_name)?;
            let msg = format!(
                "Fast-forward merge: {} -> {}",
                branch_name,
                object_id_from_oid(target_oid).short()
            );
            branch_ref.set_target(target_oid, &msg)?;
            repo.set_head(head_name)?;
        }
        Err(e) if e.code() == git2::ErrorCode::UnbornBranch => {
            repo.set_head_detached(target_oid)?;
        }
        Err(e) => return Err(e.into()),
    }

    Ok(MergeResult::FastForward(object_id_from_oid(target_oid)))
}

/// Perform a 3-way merge and create a merge commit on success.
fn do_three_way_merge(
    repo: &Repository,
    target: &AnnotatedCommit<'_>,
    branch_name: &str,
) -> Result<MergeResult> {
    let mut merge_opts = MergeOptions::new();
    merge_opts.fail_on_conflict(false);

    repo.merge(&[target], Some(&mut merge_opts), None)?;

    // Count conflicts.
    let index = repo.index()?;
    let conflict_count = index.conflicts()?.count();

    if conflict_count > 0 {
        // Leave the working tree in the conflicted state for the user to resolve.
        return Err(GitEngineError::MergeConflict {
            file_count: conflict_count,
        });
    }

    // No conflicts — create the merge commit.
    let sig = repo_signature(repo)?;

    let mut index = repo.index()?;
    let tree_oid = index.write_tree()?;
    let tree = repo.find_tree(tree_oid)?;

    let head_commit = repo.head()?.peel_to_commit()?;
    let target_commit = repo.find_commit(target.id())?;

    let msg = format!("Merge branch '{branch_name}'");
    let commit_oid = repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        &msg,
        &tree,
        &[&head_commit, &target_commit],
    )?;

    // Clean up merge state.
    repo.cleanup_state()?;

    Ok(MergeResult::MergeCommit(object_id_from_oid(commit_oid)))
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::{
        branch::{checkout_branch, create_branch},
        index::{commit as do_commit, stage_all},
    };
    use std::fs;
    use tempfile::TempDir;

    /// Build a repo with an initial commit.
    fn repo_with_commit(dir: &TempDir) -> Repository {
        let repo = Repository::init(dir.path()).unwrap();
        fs::write(dir.path().join("base.txt"), b"base").unwrap();
        stage_all(&repo).unwrap();
        do_commit(&repo, "Initial commit", None).unwrap();
        repo
    }

    #[test]
    fn merge_already_up_to_date() {
        let tmp = TempDir::new().unwrap();
        let repo = repo_with_commit(&tmp);

        // Determine default branch name.
        let default_branch = repo
            .head()
            .unwrap()
            .shorthand()
            .unwrap()
            .to_string();

        create_branch(&repo, "feature").unwrap();
        // Merge the branch we're already on into itself (via feature which is identical).
        checkout_branch(&repo, "feature").unwrap();
        let result = merge(&repo, &default_branch).unwrap();
        assert_eq!(result, MergeResult::AlreadyUpToDate);
    }

    #[test]
    fn fast_forward_merge() {
        let tmp = TempDir::new().unwrap();
        let repo = repo_with_commit(&tmp);

        let default_branch = repo
            .head()
            .unwrap()
            .shorthand()
            .unwrap()
            .to_string();

        // Create a branch, add a commit on it.
        create_branch(&repo, "ff-branch").unwrap();
        checkout_branch(&repo, "ff-branch").unwrap();
        fs::write(tmp.path().join("extra.txt"), b"extra").unwrap();
        stage_all(&repo).unwrap();
        do_commit(&repo, "Extra commit on ff-branch", None).unwrap();

        // Switch back to default and merge — should be a fast-forward.
        checkout_branch(&repo, &default_branch).unwrap();
        let result = merge(&repo, "ff-branch").unwrap();
        assert!(
            matches!(result, MergeResult::FastForward(_)),
            "expected fast-forward, got {:?}",
            result
        );
    }

    #[test]
    fn three_way_merge_no_conflicts() {
        let tmp = TempDir::new().unwrap();
        let repo = repo_with_commit(&tmp);

        let default_branch = repo
            .head()
            .unwrap()
            .shorthand()
            .unwrap()
            .to_string();

        // Commit on feature branch (touches different file).
        create_branch(&repo, "feature-a").unwrap();
        checkout_branch(&repo, "feature-a").unwrap();
        fs::write(tmp.path().join("feature.txt"), b"feature content").unwrap();
        stage_all(&repo).unwrap();
        do_commit(&repo, "Feature commit", None).unwrap();

        // Commit on main (touches different file).
        checkout_branch(&repo, &default_branch).unwrap();
        fs::write(tmp.path().join("main-extra.txt"), b"main content").unwrap();
        stage_all(&repo).unwrap();
        do_commit(&repo, "Main commit", None).unwrap();

        // Merge feature-a into main — both touched different files, no conflict.
        let result = merge(&repo, "feature-a").unwrap();
        assert!(
            matches!(result, MergeResult::MergeCommit(_)),
            "expected merge commit, got {:?}",
            result
        );
    }

    #[test]
    fn merge_nonexistent_branch_fails() {
        let tmp = TempDir::new().unwrap();
        let repo = repo_with_commit(&tmp);
        let result = merge(&repo, "nonexistent");
        assert!(matches!(result, Err(GitEngineError::NotFound(_))));
    }
}
