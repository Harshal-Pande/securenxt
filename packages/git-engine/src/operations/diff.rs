//! Diff and repository-status operations.
//!
//! Provides: `repository_status`, `diff_working_tree`.

use git2::{Repository, Status, StatusOptions};

use crate::error::Result;
use crate::types::{DiffHunk, DiffSummary, FileStatus, FileStatusKind, RepositoryStatus};

// ─────────────────────────────────────────────────────────────────────────────
// Public operations
// ─────────────────────────────────────────────────────────────────────────────

/// Return a snapshot of the repository's working-tree and index state.
///
/// The result categorises every changed file into staged, unstaged, or
/// untracked.  Ahead/behind counts are computed against the upstream tracking
/// branch, defaulting to 0 when no upstream is configured.
pub(crate) fn repository_status(repo: &Repository) -> Result<RepositoryStatus> {
    // Resolve branch name (or "HEAD" in detached state).
    let branch = match repo.head() {
        Ok(head_ref) => head_ref
            .shorthand()
            .unwrap_or("HEAD")
            .to_string(),
        Err(_) => "HEAD".to_string(),
    };

    // Compute ahead / behind relative to upstream.
    let (ahead, behind) = compute_ahead_behind(repo).unwrap_or((0, 0));

    // Enumerate file statuses.
    let mut status_opts = StatusOptions::new();
    status_opts
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);

    let statuses = repo.statuses(Some(&mut status_opts))?;

    let mut staged_files: Vec<FileStatus> = Vec::new();
    let mut unstaged_files: Vec<FileStatus> = Vec::new();
    let mut untracked_files: Vec<String> = Vec::new();

    for entry in statuses.iter() {
        let path = entry
            .path()
            .unwrap_or("")
            .to_string();

        let status = entry.status();

        if status.contains(Status::WT_NEW) {
            untracked_files.push(path.clone());
            continue;
        }

        // Staged (index) changes.
        let index_kind = index_status_kind(status);
        if let Some(kind) = index_kind {
            staged_files.push(FileStatus {
                path: path.clone(),
                new_path: entry
                    .head_to_index()
                    .and_then(|d| d.new_file().path())
                    .and_then(|p| p.to_str())
                    .filter(|&p| p != path)
                    .map(String::from),
                index_status: Some(kind),
                worktree_status: None,
            });
        }

        // Unstaged (working-tree) changes.
        let wt_kind = worktree_status_kind(status);
        if let Some(kind) = wt_kind {
            unstaged_files.push(FileStatus {
                path: path.clone(),
                new_path: None,
                index_status: None,
                worktree_status: Some(kind),
            });
        }
    }

    let is_clean = staged_files.is_empty()
        && unstaged_files.is_empty()
        && untracked_files.is_empty();

    Ok(RepositoryStatus {
        branch,
        ahead,
        behind,
        staged_files,
        unstaged_files,
        untracked_files,
        is_clean,
    })
}

/// Produce a [`DiffSummary`] comparing the working tree against the HEAD commit.
///
/// This is equivalent to `git diff HEAD` — it shows all changes that have not
/// yet been staged or committed.  When there are no changes, returns
/// [`DiffSummary::empty()`].
pub(crate) fn diff_working_tree(repo: &Repository) -> Result<DiffSummary> {
    let head_tree = match repo.head() {
        Ok(head_ref) => {
            let commit = head_ref.peel_to_commit()?;
            Some(commit.tree()?)
        }
        Err(e) if e.code() == git2::ErrorCode::UnbornBranch => None,
        Err(e) => return Err(e.into()),
    };

    // Diff from HEAD tree (or empty tree) to working directory.
    let mut diff_opts = git2::DiffOptions::new();
    diff_opts.include_untracked(false);

    let diff = repo.diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut diff_opts))?;

    let stats = diff.stats()?;

    let mut hunks: Vec<DiffHunk> = Vec::new();

    diff.foreach(
        &mut |_, _| true,
        None,
        Some(&mut |_delta, hunk| {
            hunks.push(DiffHunk {
                header: std::str::from_utf8(hunk.header())
                    .unwrap_or("")
                    .trim()
                    .to_string(),
                added_lines: hunk.new_lines() as usize,
                removed_lines: hunk.old_lines() as usize,
            });
            true
        }),
        None,
    )?;

    Ok(DiffSummary {
        insertions: stats.insertions(),
        deletions: stats.deletions(),
        files_changed: stats.files_changed(),
        hunks,
    })
}

/// Produce a [`DiffSummary`] comparing the staging area (index) against the HEAD commit.
///
/// This is equivalent to `git diff --cached HEAD` (or just `git diff --cached` for initial commits)
/// — it shows all changes that have been staged for the next commit.
pub(crate) fn diff_index_to_head(repo: &Repository) -> Result<DiffSummary> {
    let head_tree = match repo.head() {
        Ok(head_ref) => {
            let commit = head_ref.peel_to_commit()?;
            Some(commit.tree()?)
        }
        Err(e) if e.code() == git2::ErrorCode::UnbornBranch => None,
        Err(e) => return Err(e.into()),
    };

    let index = repo.index()?;

    // Diff from HEAD tree (or empty tree) to index.
    let diff = repo.diff_tree_to_index(head_tree.as_ref(), Some(&index), None)?;

    let stats = diff.stats()?;

    let mut hunks: Vec<DiffHunk> = Vec::new();

    diff.foreach(
        &mut |_, _| true,
        None,
        Some(&mut |_delta, hunk| {
            hunks.push(DiffHunk {
                header: std::str::from_utf8(hunk.header())
                    .unwrap_or("")
                    .trim()
                    .to_string(),
                added_lines: hunk.new_lines() as usize,
                removed_lines: hunk.old_lines() as usize,
            });
            true
        }),
        None,
    )?;

    Ok(DiffSummary {
        insertions: stats.insertions(),
        deletions: stats.deletions(),
        files_changed: stats.files_changed(),
        hunks,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Map a libgit2 [`Status`] bitfield to an index-side [`FileStatusKind`].
fn index_status_kind(status: Status) -> Option<FileStatusKind> {
    if status.contains(Status::INDEX_NEW) {
        Some(FileStatusKind::Added)
    } else if status.contains(Status::INDEX_MODIFIED) {
        Some(FileStatusKind::Modified)
    } else if status.contains(Status::INDEX_DELETED) {
        Some(FileStatusKind::Deleted)
    } else if status.contains(Status::INDEX_RENAMED) {
        Some(FileStatusKind::Renamed)
    } else if status.contains(Status::INDEX_TYPECHANGE) {
        Some(FileStatusKind::Typechange)
    } else if status.contains(Status::CONFLICTED) {
        Some(FileStatusKind::Conflicted)
    } else {
        None
    }
}

/// Map a libgit2 [`Status`] bitfield to a working-tree-side [`FileStatusKind`].
fn worktree_status_kind(status: Status) -> Option<FileStatusKind> {
    if status.contains(Status::WT_MODIFIED) {
        Some(FileStatusKind::Modified)
    } else if status.contains(Status::WT_DELETED) {
        Some(FileStatusKind::Deleted)
    } else if status.contains(Status::WT_RENAMED) {
        Some(FileStatusKind::Renamed)
    } else if status.contains(Status::WT_TYPECHANGE) {
        Some(FileStatusKind::Typechange)
    } else {
        None
    }
}

/// Compute how many commits the local branch is ahead of / behind its upstream.
///
/// Returns `(ahead, behind)` or `None` when no upstream is configured.
fn compute_ahead_behind(repo: &Repository) -> Option<(usize, usize)> {
    let head = repo.head().ok()?;
    let branch_name = head.shorthand()?;

    let local_branch = repo
        .find_branch(branch_name, git2::BranchType::Local)
        .ok()?;
    let upstream = local_branch.upstream().ok()?;

    let local_oid = head.target()?;
    let upstream_oid = upstream.get().target()?;

    repo.graph_ahead_behind(local_oid, upstream_oid).ok()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::index::{commit as do_commit, stage_all, stage_file};
    use std::fs;
    use tempfile::TempDir;

    fn repo_with_one_commit() -> (TempDir, Repository) {
        let tmp = TempDir::new().unwrap();
        let repo = Repository::init(tmp.path()).unwrap();
        fs::write(tmp.path().join("base.txt"), b"base content").unwrap();
        stage_all(&repo).unwrap();
        do_commit(&repo, "Initial commit", None).unwrap();
        (tmp, repo)
    }

    #[test]
    fn clean_repo_is_clean() {
        let (_tmp, repo) = repo_with_one_commit();
        let status = repository_status(&repo).unwrap();
        assert!(status.is_clean);
        assert!(status.staged_files.is_empty());
        assert!(status.unstaged_files.is_empty());
        assert!(status.untracked_files.is_empty());
    }

    #[test]
    fn untracked_file_appears_in_status() {
        let (tmp, repo) = repo_with_one_commit();
        fs::write(tmp.path().join("new.txt"), b"new").unwrap();
        let status = repository_status(&repo).unwrap();
        assert!(!status.is_clean);
        assert!(status.untracked_files.contains(&"new.txt".to_string()));
    }

    #[test]
    fn staged_new_file_appears_in_staged() {
        let (tmp, repo) = repo_with_one_commit();
        fs::write(tmp.path().join("staged.txt"), b"staged").unwrap();
        stage_file(&repo, "staged.txt").unwrap();
        let status = repository_status(&repo).unwrap();
        assert!(status.staged_files.iter().any(|f| f.path == "staged.txt"));
    }

    #[test]
    fn modified_file_appears_unstaged() {
        let (tmp, repo) = repo_with_one_commit();
        fs::write(tmp.path().join("base.txt"), b"modified content").unwrap();
        let status = repository_status(&repo).unwrap();
        assert!(status.unstaged_files.iter().any(|f| f.path == "base.txt"));
    }

    #[test]
    fn diff_working_tree_counts_changes() {
        let (tmp, repo) = repo_with_one_commit();
        // Modify the existing file: +2 lines, content replaced.
        fs::write(tmp.path().join("base.txt"), b"line1\nline2\nline3\n").unwrap();
        let diff = diff_working_tree(&repo).unwrap();
        // At minimum, there should be one file changed.
        assert!(diff.files_changed >= 1);
        assert!(diff.insertions > 0 || diff.deletions > 0);
    }

    #[test]
    fn diff_clean_repo_is_empty() {
        let (_tmp, repo) = repo_with_one_commit();
        let diff = diff_working_tree(&repo).unwrap();
        assert_eq!(diff.files_changed, 0);
        assert_eq!(diff.insertions, 0);
        assert_eq!(diff.deletions, 0);

        let staged_diff = diff_index_to_head(&repo).unwrap();
        assert_eq!(staged_diff.files_changed, 0);
    }

    #[test]
    fn diff_empty_repo_returns_empty_summary() {
        let tmp = TempDir::new().unwrap();
        let repo = Repository::init(tmp.path()).unwrap();
        let diff = diff_working_tree(&repo).unwrap();
        assert_eq!(diff.files_changed, 0);
    }
}
