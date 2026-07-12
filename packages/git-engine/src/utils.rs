//! Internal utility functions shared across the repository engine.
//!
//! These helpers are **not** part of the public API and are `pub(crate)` only.
//! Phase 2 will extend `repo_signature` to accept an [`Ed25519Signer`] from
//! `securenxt-crypto` so that every commit carries a verifiable identity.

use std::path::{Path, PathBuf};

use chrono::{DateTime, TimeZone, Utc};
use git2::{Oid, Repository, Signature};

use crate::error::{GitEngineError, Result};
use crate::types::ObjectId;

// ─────────────────────────────────────────────────────────────────────────────
// Path utilities
// ─────────────────────────────────────────────────────────────────────────────

/// Resolve `path` to the root of the enclosing Git repository.
///
/// The function first canonicalises `path`.  If the path itself is a Git repo
/// root (contains a `.git` entry), it is returned as-is.  Otherwise the
/// function walks up the directory tree until it finds a `.git` entry, mirroring
/// the discovery logic built into Git itself.
///
/// Returns `Err(GitEngineError::InvalidPath)` when:
/// - `path` does not exist on the filesystem.
/// - No `.git` directory is found anywhere up the hierarchy.
pub(crate) fn resolve_repository_path(path: &Path) -> Result<PathBuf> {
    let canonical = path.canonicalize().map_err(|e| {
        GitEngineError::InvalidPath(format!(
            "cannot canonicalise '{}': {e}",
            path.display()
        ))
    })?;

    // Walk upward from `canonical` until we find a `.git` entry.
    let mut current: &Path = &canonical;
    loop {
        let git_dir = current.join(".git");
        if git_dir.exists() {
            return Ok(current.to_path_buf());
        }

        match current.parent() {
            Some(parent) => current = parent,
            None => {
                return Err(GitEngineError::InvalidPath(format!(
                    "'{}' is not inside a Git repository",
                    path.display()
                )))
            }
        }
    }
}

/// Checks whether a directory exists and contains a `.git` entry *without*
/// opening the repository through libgit2.  Used for fast pre-flight checks.
pub(crate) fn path_is_git_repo(path: &Path) -> bool {
    path.exists() && path.join(".git").exists()
}

// ─────────────────────────────────────────────────────────────────────────────
// Git object utilities
// ─────────────────────────────────────────────────────────────────────────────

/// Convert a [`git2::Oid`] to our validated [`ObjectId`] newtype.
pub(crate) fn object_id_from_oid(oid: Oid) -> ObjectId {
    ObjectId::from_oid_unchecked(oid.to_string())
}

// ─────────────────────────────────────────────────────────────────────────────
// Signature helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Build a libgit2 [`Signature`] for use in commits.
///
/// Priority order for author identity:
/// 1. Git config (`user.name` / `user.email`) in the repository.
/// 2. Global Git config (`~/.gitconfig`).
/// 3. Hard fallback: `"Securenxt User" <securenxt@local>`.
///
/// # Phase 2 injection point
///
/// When `securenxt-crypto` is ready, this function will accept an
/// `Ed25519Signer` parameter and embed the public key fingerprint inside the
/// email field (following the Monkeysphere convention) so that every commit is
/// cryptographically attributable.
pub(crate) fn repo_signature(repo: &Repository) -> Result<Signature<'static>> {
    // Try to get author details from git config (repo-level then global).
    let config = repo.config()?;

    let name = config
        .get_string("user.name")
        .unwrap_or_else(|_| "Securenxt User".to_string());

    let email = config
        .get_string("user.email")
        .unwrap_or_else(|_| "securenxt@local".to_string());

    let sig = Signature::now(&name, &email)?;
    Ok(sig)
}

// ─────────────────────────────────────────────────────────────────────────────
// Timestamp utilities
// ─────────────────────────────────────────────────────────────────────────────

/// Convert a [`git2::Time`] value into a [`DateTime<Utc>`].
///
/// `git2::Time` stores a Unix timestamp (seconds) plus a UTC offset in minutes.
/// We normalise to UTC for consistent serialisation.
pub(crate) fn git_time_to_datetime(time: git2::Time) -> DateTime<Utc> {
    Utc.timestamp_opt(time.seconds(), 0)
        .single()
        .unwrap_or_else(Utc::now)
}

// ─────────────────────────────────────────────────────────────────────────────
// Repository name helper
// ─────────────────────────────────────────────────────────────────────────────

/// Extract a human-readable repository name from a filesystem path.
///
/// Uses the last path component (the directory name).  Falls back to `"repo"`
/// when the path has no components (e.g. filesystem root).
pub(crate) fn repo_name_from_path(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("repo")
        .to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_git_repo() -> (TempDir, PathBuf) {
        let tmp = TempDir::new().expect("create temp dir");
        let repo_path = tmp.path().to_path_buf();
        git2::Repository::init(&repo_path).expect("init repo");
        (tmp, repo_path)
    }

    #[test]
    fn resolve_exact_repo_root() {
        let (_tmp, repo_path) = make_git_repo();
        let resolved = resolve_repository_path(&repo_path).expect("resolve");
        assert_eq!(resolved, repo_path.canonicalize().unwrap());
    }

    #[test]
    fn resolve_subdir_of_repo() {
        let (_tmp, repo_path) = make_git_repo();
        let subdir = repo_path.join("src");
        std::fs::create_dir(&subdir).unwrap();
        let resolved = resolve_repository_path(&subdir).expect("resolve from subdir");
        assert_eq!(resolved, repo_path.canonicalize().unwrap());
    }

    #[test]
    fn resolve_non_repo_returns_err() {
        let tmp = TempDir::new().unwrap();
        let result = resolve_repository_path(tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn path_is_git_repo_true() {
        let (_tmp, repo_path) = make_git_repo();
        assert!(path_is_git_repo(&repo_path));
    }

    #[test]
    fn path_is_git_repo_false() {
        let tmp = TempDir::new().unwrap();
        assert!(!path_is_git_repo(tmp.path()));
    }

    #[test]
    fn object_id_from_oid_is_correct() {
        let oid = Oid::zero();
        let obj_id = object_id_from_oid(oid);
        assert_eq!(obj_id.as_str().len(), 40);
        assert_eq!(obj_id.short().len(), 8);
    }

    #[test]
    fn repo_name_from_path_uses_last_segment() {
        let p = PathBuf::from("/home/user/my-project");
        assert_eq!(repo_name_from_path(&p), "my-project");
    }

    #[test]
    fn repo_name_fallback_on_root() {
        let p = PathBuf::from("/");
        assert_eq!(repo_name_from_path(&p), "repo");
    }
}
