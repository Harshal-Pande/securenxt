//! Index (staging area) and commit operations.
//!
//! Provides: `stage_all`, `stage_file`, `unstage_file`, `commit`.

use std::path::Path;

use git2::Repository;

use crate::error::{GitEngineError, Result};
use crate::types::{CommitSigner, ObjectId};
use crate::utils::{object_id_from_oid, repo_signature};

// ─────────────────────────────────────────────────────────────────────────────
// Public operations
// ─────────────────────────────────────────────────────────────────────────────

/// Stage all changed, deleted, and untracked files in the working tree.
///
/// Equivalent to `git add -A`.
pub(crate) fn stage_all(repo: &Repository) -> Result<()> {
    let mut index = repo.index()?;

    // `add_all` with "." matches all files.  The callback can be used to filter
    // or log; passing `None` accepts every path.
    index.add_all(["."].iter(), git2::IndexAddOption::DEFAULT, None)?;
    index.write()?;

    Ok(())
}

/// Stage a single file (or directory) by its path relative to the repo root.
///
/// Equivalent to `git add <path>`.
///
/// Returns `Err(GitEngineError::NotFound)` when the path does not exist in
/// either the working tree or the index (i.e. it is not a valid path to stage).
pub(crate) fn stage_file(repo: &Repository, file_path: &str) -> Result<()> {
    let workdir = repo.workdir().ok_or_else(|| {
        GitEngineError::InvalidPath("cannot stage files in a bare repository".to_string())
    })?;

    let full_path = workdir.join(file_path);

    let mut index = repo.index()?;

    if full_path.exists() {
        // File or directory exists — add it.
        index.add_path(Path::new(file_path)).map_err(|_| {
            GitEngineError::NotFound(format!("'{}' could not be staged", file_path))
        })?;
    } else {
        // File doesn't exist in working tree — may be a deletion; use
        // `update_all` scoped to this path to pick up the removal.
        let matched = std::cell::Cell::new(false);
        index.update_all(
            [file_path].iter(),
            Some(&mut |path: &Path, _| {
                if path == Path::new(file_path) {
                    matched.set(true);
                }
                0 // 0 = accept
            }),
        )?;

        if !matched.get() {
            return Err(GitEngineError::NotFound(format!(
                "path '{}' not found in the working tree or index",
                file_path
            )));
        }
    }

    index.write()?;
    Ok(())
}

/// Unstage a single file, removing it from the index while leaving the
/// working-tree copy intact.
///
/// Equivalent to `git restore --staged <path>`.
///
/// Returns `Err(GitEngineError::NotFound)` when the file is not in the index.
pub(crate) fn unstage_file(repo: &Repository, file_path: &str) -> Result<()> {
    // If the repository has no HEAD (empty repo), just remove from index.
    let head_result = repo.head();

    match head_result {
        Ok(head_ref) => {
            let head_commit = head_ref.peel_to_commit()?;
            let head_tree = head_commit.tree()?;

            // `reset_default` resets the index entry for the given path to the
            // state in `head_tree` — effectively unstaging the file.
            repo.reset_default(Some(head_tree.as_object()), [file_path].iter())?;
        }
        Err(e) if e.code() == git2::ErrorCode::UnbornBranch => {
            // No HEAD yet — remove the entry from the index directly.
            let mut index = repo.index()?;
            let before = index.len();
            index.remove_path(Path::new(file_path)).map_err(|_| {
                GitEngineError::NotFound(format!(
                    "path '{}' is not staged",
                    file_path
                ))
            })?;
            if index.len() == before {
                return Err(GitEngineError::NotFound(format!(
                    "path '{}' is not staged",
                    file_path
                )));
            }
            index.write()?;
        }
        Err(e) => return Err(e.into()),
    }

    Ok(())
}

/// Create a new commit from the current index (staging area).
///
/// The commit message must be non-empty.  Author and committer identity are
/// resolved from the repository's Git config (see [`repo_signature`]).
///
/// Returns `Err(GitEngineError::CommitError)` when:
/// - The staging area is empty (nothing to commit).
/// - The message is blank.
///
/// # Phase 2 injection point
/// The signature here will be replaced with an Ed25519-signed commit header
/// once `securenxt-crypto` is integrated.
pub(crate) fn commit(
    repo: &Repository,
    message: &str,
    _signer: Option<&dyn CommitSigner>,
) -> Result<ObjectId> {
    let message = message.trim();
    if message.is_empty() {
        return Err(GitEngineError::CommitError(
            "commit message cannot be empty".to_string(),
        ));
    }

    let sig = repo_signature(repo)?;

    let mut index = repo.index()?;
    index.write()?;
    let tree_oid = index.write_tree()?;
    let tree = repo.find_tree(tree_oid)?;

    // Collect parent commits.
    let parent_commit: Option<git2::Commit<'_>> = match repo.head() {
        Ok(head_ref) => Some(head_ref.peel_to_commit()?),
        Err(e) if e.code() == git2::ErrorCode::UnbornBranch => None,
        Err(e) => return Err(e.into()),
    };

    // Check that there are actually staged changes to commit.
    let is_empty_commit = match &parent_commit {
        None => {
            // First commit — the tree must be non-empty.
            tree.is_empty()
        }
        Some(parent) => {
            // Subsequent commits — check if the tree changed.
            let parent_tree = parent.tree()?;
            let diff = repo.diff_tree_to_tree(
                Some(&parent_tree),
                Some(&tree),
                None,
            )?;
            diff.deltas().len() == 0
        }
    };

    if is_empty_commit {
        return Err(GitEngineError::CommitError(
            "nothing to commit: staging area is empty".to_string(),
        ));
    }

    let parent_refs: Vec<&git2::Commit<'_>> = parent_commit.iter().collect();

    let commit_oid = repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        message,
        &tree,
        &parent_refs,
    )?;

    Ok(object_id_from_oid(commit_oid))
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn init_repo() -> (TempDir, Repository) {
        let tmp = TempDir::new().unwrap();
        let repo = Repository::init(tmp.path()).unwrap();
        (tmp, repo)
    }

    #[test]
    fn stage_all_and_commit() {
        let (tmp, repo) = init_repo();
        fs::write(tmp.path().join("hello.txt"), b"hello").unwrap();

        stage_all(&repo).unwrap();
        let oid = commit(&repo, "Initial commit", None).unwrap();
        assert_eq!(oid.as_str().len(), 40);
    }

    #[test]
    fn stage_specific_file() {
        let (tmp, repo) = init_repo();
        fs::write(tmp.path().join("a.txt"), b"a").unwrap();
        fs::write(tmp.path().join("b.txt"), b"b").unwrap();

        stage_file(&repo, "a.txt").unwrap();
        commit(&repo, "Stage only a", None).unwrap();

        // b.txt should remain untracked.
        let statuses = repo.statuses(None).unwrap();
        let b_status = statuses
            .iter()
            .find(|s| s.path() == Some("b.txt"))
            .expect("b.txt should be untracked");
        assert!(b_status.status().contains(git2::Status::WT_NEW));
    }

    #[test]
    fn unstage_file_removes_from_index() {
        let (tmp, repo) = init_repo();
        fs::write(tmp.path().join("x.txt"), b"x").unwrap();
        stage_file(&repo, "x.txt").unwrap();
        // Stage and make first commit so HEAD exists.
        commit(&repo, "First", None).unwrap();

        // Now modify and stage.
        fs::write(tmp.path().join("x.txt"), b"x modified").unwrap();
        stage_file(&repo, "x.txt").unwrap();

        // Unstage it.
        unstage_file(&repo, "x.txt").unwrap();

        // The file should now be modified in working tree but NOT staged.
        let statuses = repo.statuses(None).unwrap();
        let entry = statuses
            .iter()
            .find(|s| s.path() == Some("x.txt"))
            .expect("x.txt should appear in status");
        // Should be unstaged (working-tree modified), not index-modified.
        assert!(
            entry.status().contains(git2::Status::WT_MODIFIED)
                && !entry.status().contains(git2::Status::INDEX_MODIFIED),
            "expected only WT_MODIFIED, got {:?}",
            entry.status()
        );
    }

    #[test]
    fn empty_commit_message_fails() {
        let (tmp, repo) = init_repo();
        fs::write(tmp.path().join("f.txt"), b"f").unwrap();
        stage_all(&repo).unwrap();
        let result = commit(&repo, "   ", None);
        assert!(matches!(result, Err(GitEngineError::CommitError(_))));
    }

    #[test]
    fn commit_nothing_staged_fails() {
        let (tmp, repo) = init_repo();
        // Create a first commit so HEAD exists.
        fs::write(tmp.path().join("f.txt"), b"f").unwrap();
        stage_all(&repo).unwrap();
        commit(&repo, "first", None).unwrap();

        // Try to commit with an unchanged index.
        let result = commit(&repo, "empty", None);
        assert!(matches!(result, Err(GitEngineError::CommitError(_))));
    }
}
