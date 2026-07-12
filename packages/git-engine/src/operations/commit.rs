//! Commit inspection operations.
//!
//! Provides: `list_recent_commits`, `get_commit`, `head_commit`.

use git2::Repository;

use crate::error::{GitEngineError, Result};
use crate::types::CommitInfo;
use crate::utils::{git_time_to_datetime, object_id_from_oid};

// ─────────────────────────────────────────────────────────────────────────────
// Public operations
// ─────────────────────────────────────────────────────────────────────────────

/// Return metadata for the HEAD commit.
///
/// Returns `None` when the repository is empty (no commits yet).
pub(crate) fn head_commit(repo: &Repository) -> Result<Option<CommitInfo>> {
    match repo.head() {
        Ok(head_ref) => {
            let commit = head_ref.peel_to_commit()?;
            Ok(Some(commit_to_info(&commit)))
        }
        Err(e) if e.code() == git2::ErrorCode::UnbornBranch => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Return the `n` most recent commits reachable from HEAD, newest first.
///
/// If `n` is 0 the result is always empty.  When the repository is empty an
/// empty `Vec` is returned (no error).
pub(crate) fn list_recent_commits(repo: &Repository, n: usize) -> Result<Vec<CommitInfo>> {
    if n == 0 {
        return Ok(Vec::new());
    }

    let mut revwalk = repo.revwalk()?;
    revwalk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)?;

    // Start from HEAD.  If HEAD is unborn (empty repo) return empty vec.
    match revwalk.push_head() {
        Ok(()) => {}
        Err(e) if e.code() == git2::ErrorCode::UnbornBranch => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    }

    let mut commits = Vec::with_capacity(n);
    for oid_result in revwalk.take(n) {
        let oid = oid_result?;
        let commit = repo.find_commit(oid)?;
        commits.push(commit_to_info(&commit));
    }

    Ok(commits)
}

/// Look up a specific commit by its full or abbreviated SHA-1.
///
/// Returns `Err(GitEngineError::NotFound)` when no such object exists.
pub(crate) fn get_commit(repo: &Repository, id: &str) -> Result<CommitInfo> {
    let oid = git2::Oid::from_str(id).map_err(|_| {
        GitEngineError::NotFound(format!("'{id}' is not a valid commit SHA-1"))
    })?;

    let commit = repo.find_commit(oid).map_err(|_| {
        GitEngineError::NotFound(format!("commit '{id}' not found in repository"))
    })?;

    Ok(commit_to_info(&commit))
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Convert a libgit2 [`git2::Commit`] into our serialisable [`CommitInfo`].
fn commit_to_info(commit: &git2::Commit<'_>) -> CommitInfo {
    let author = commit.author();

    let obj_id = object_id_from_oid(commit.id());

    CommitInfo {
        short_id: obj_id.short().to_string(),
        id: obj_id,
        message: commit.message().unwrap_or("").to_string(),
        author: author.name().unwrap_or("Unknown").to_string(),
        email: author.email().unwrap_or("").to_string(),
        timestamp: git_time_to_datetime(author.when()),
        parent_ids: commit
            .parent_ids()
            .map(object_id_from_oid)
            .collect(),
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

    fn make_repo_with_commits(n: usize) -> (TempDir, Repository) {
        let tmp = TempDir::new().unwrap();
        let repo = Repository::init(tmp.path()).unwrap();
        let sig = git2::Signature::now("Test", "test@example.com").unwrap();

        for i in 0..n {
            let file = tmp.path().join(format!("file_{i}.txt"));
            fs::write(&file, format!("content {i}")).unwrap();
            stage_all(&repo).unwrap();

            {
                let tree_id = repo.index().unwrap().write_tree().unwrap();
                let tree = repo.find_tree(tree_id).unwrap();

                let parents: Vec<git2::Commit<'_>> = repo
                    .head()
                    .ok()
                    .and_then(|h| h.peel_to_commit().ok())
                    .into_iter()
                    .collect();
                let parent_refs: Vec<&git2::Commit<'_>> = parents.iter().collect();

                repo.commit(
                    Some("HEAD"),
                    &sig,
                    &sig,
                    &format!("commit {i}"),
                    &tree,
                    &parent_refs,
                )
                .unwrap();
            } // all borrows of `repo` end here
        }

        (tmp, repo)
    }

    #[test]
    fn empty_repo_has_no_head_commit() {
        let tmp = TempDir::new().unwrap();
        let repo = Repository::init(tmp.path()).unwrap();
        let result = head_commit(&repo).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn head_commit_returns_latest() {
        let (_tmp, repo) = make_repo_with_commits(3);
        let info = head_commit(&repo).unwrap().expect("should have a commit");
        assert!(info.message.contains("commit 2")); // zero-indexed
    }

    #[test]
    fn list_recent_commits_returns_n() {
        let (_tmp, repo) = make_repo_with_commits(5);
        let commits = list_recent_commits(&repo, 3).unwrap();
        assert_eq!(commits.len(), 3);
    }

    #[test]
    fn list_recent_zero_returns_empty() {
        let (_tmp, repo) = make_repo_with_commits(2);
        let commits = list_recent_commits(&repo, 0).unwrap();
        assert!(commits.is_empty());
    }

    #[test]
    fn list_recent_more_than_exists_returns_all() {
        let (_tmp, repo) = make_repo_with_commits(2);
        let commits = list_recent_commits(&repo, 100).unwrap();
        assert_eq!(commits.len(), 2);
    }

    #[test]
    fn list_recent_empty_repo_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let repo = Repository::init(tmp.path()).unwrap();
        let commits = list_recent_commits(&repo, 10).unwrap();
        assert!(commits.is_empty());
    }

    #[test]
    fn get_commit_by_full_sha() {
        let (_tmp, repo) = make_repo_with_commits(1);
        let head = head_commit(&repo).unwrap().unwrap();
        let found = get_commit(&repo, head.id.as_str()).unwrap();
        assert_eq!(found.id, head.id);
    }

    #[test]
    fn get_commit_not_found() {
        let (_tmp, repo) = make_repo_with_commits(1);
        let result = get_commit(&repo, "0000000000000000000000000000000000000000");
        assert!(matches!(result, Err(GitEngineError::NotFound(_))));
    }

    #[test]
    fn commit_short_id_is_8_chars() {
        let (_tmp, repo) = make_repo_with_commits(1);
        let info = head_commit(&repo).unwrap().unwrap();
        assert_eq!(info.short_id.len(), 8);
    }

    #[test]
    fn initial_commit_has_no_parents() {
        let (_tmp, repo) = make_repo_with_commits(1);
        let info = head_commit(&repo).unwrap().unwrap();
        assert!(info.parent_ids.is_empty());
    }

    #[test]
    fn second_commit_has_one_parent() {
        let (_tmp, repo) = make_repo_with_commits(2);
        let info = head_commit(&repo).unwrap().unwrap();
        assert_eq!(info.parent_ids.len(), 1);
    }
}
