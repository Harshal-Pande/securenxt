//! [`RepositoryManager`] — the primary entry point for all repository operations.
//!
//! All public methods on `RepositoryManager` delegate to the specialised
//! operation modules in [`crate::operations`].  The manager owns the
//! `git2::Repository` handle, keeping lifetime management simple for callers.
//!
//! # Construction
//!
//! ```rust,no_run
//! use securenxt_git_engine::manager::RepositoryManager;
//!
//! // Open an existing repository.
//! let manager = RepositoryManager::open_repository("/path/to/repo").unwrap();
//!
//! // Or initialise a new one.
//! let manager = RepositoryManager::init_repository("/path/to/new-repo").unwrap();
//! ```
//!
//! # Thread safety
//!
//! `git2::Repository` is `Send` but **not** `Sync`.  `RepositoryManager` therefore
//! carries the same constraints.  In the Tauri command layer (Phase 3) each
//! command handler should obtain a fresh `RepositoryManager` per call, or the
//! shared state should be protected by a `tokio::sync::Mutex`.

use std::path::{Path, PathBuf};

use git2::Repository;

use crate::error::{GitEngineError, Result};
use crate::operations::{
    branch, commit as commit_ops, diff, index, merge as merge_ops,
};
use crate::types::{
    BranchInfo, CommitInfo, CommitSigner, DiffSummary, HeadState, MergeResult, ObjectId, RepositoryInfo, RepositoryStatus,
};
use crate::utils::{object_id_from_oid, path_is_git_repo, repo_name_from_path, resolve_repository_path};

// ─────────────────────────────────────────────────────────────────────────────
// RepositoryManager
// ─────────────────────────────────────────────────────────────────────────────

/// Owns a `git2::Repository` handle and exposes the full repository engine API.
pub struct RepositoryManager {
    repo: Repository,
    path: PathBuf,
}

impl RepositoryManager {
    // ─────────────────────────────────────────────────────────────────────────
    // Construction
    // ─────────────────────────────────────────────────────────────────────────

    /// Initialise a new Git repository at `path`.
    ///
    /// Creates the directory and all intermediate components if they do not
    /// exist.  Returns `Err(GitEngineError::AlreadyExists)` when a repository
    /// is already present at that location.
    pub fn init_repository(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();

        if path_is_git_repo(path) {
            return Err(GitEngineError::AlreadyExists(format!(
                "a repository already exists at '{}'",
                path.display()
            )));
        }

        std::fs::create_dir_all(path)?;
        let repo = Repository::init(path)?;
        let canon = path.canonicalize()?;

        Ok(Self { repo, path: canon })
    }

    /// Open an existing repository at `path`.
    ///
    /// `path` may be the repository root or any subdirectory inside it.
    /// [`resolve_repository_path`] is used to walk up to the root.
    pub fn open_repository(path: impl AsRef<Path>) -> Result<Self> {
        let root = resolve_repository_path(path.as_ref())?;
        let repo = Repository::open(&root)?;
        Ok(Self { repo, path: root })
    }

    /// Clone a local repository at `source_path` into `destination_path`.
    ///
    /// Uses `git2`'s local clone optimisation (hardlinks where possible).
    /// Returns `Err(GitEngineError::AlreadyExists)` when the destination
    /// already contains a repository.
    pub fn clone_repository_local(
        source_path: impl AsRef<Path>,
        destination_path: impl AsRef<Path>,
    ) -> Result<Self> {
        let src = source_path.as_ref();
        let dst = destination_path.as_ref();

        if !src.exists() {
            return Err(GitEngineError::NotFound(format!(
                "source path '{}' does not exist",
                src.display()
            )));
        }

        if path_is_git_repo(dst) {
            return Err(GitEngineError::AlreadyExists(format!(
                "destination '{}' already contains a repository",
                dst.display()
            )));
        }

        std::fs::create_dir_all(dst)?;

        // Use a file:// URL so libgit2 uses the local-clone fast path.
        let url = format!("file://{}", src.canonicalize()?.display().to_string().replace('\\', "/"));

        let repo = git2::build::RepoBuilder::new()
            .clone(&url, dst)?;

        let canon = dst.canonicalize()?;
        Ok(Self { repo, path: canon })
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Repository introspection
    // ─────────────────────────────────────────────────────────────────────────

    /// Return high-level metadata about this repository.
    pub fn get_repository_info(&self) -> Result<RepositoryInfo> {
        let head_commit_id = match self.repo.head() {
            Ok(head) => head.target().map(object_id_from_oid),
            Err(e) if e.code() == git2::ErrorCode::UnbornBranch => None,
            Err(e) => return Err(e.into()),
        };

        let head_state = self.head_state()?;
        let branch = head_state.branch_name().map(String::from);

        Ok(RepositoryInfo {
            name: repo_name_from_path(&self.path),
            path: self.path.to_string_lossy().to_string(),
            head_commit_id,
            head_state,
            branch,
            is_empty: self.repo.is_empty()?,
            is_bare: self.repo.is_bare(),
        })
    }

    /// Returns `true` if a valid Git repository exists at `path`.
    ///
    /// This is a lightweight check — it does **not** open the repository.
    pub fn repository_exists(path: impl AsRef<Path>) -> bool {
        path_is_git_repo(path.as_ref())
    }

    /// Validate repository integrity using libgit2's built-in consistency check.
    ///
    /// Specifically, this verifies that the ODB (object database) is readable
    /// and that HEAD resolves to a valid object.  It does **not** perform a full
    /// `git fsck` — that would require shelling out to the Git binary.
    pub fn verify_repository(&self) -> Result<()> {
        // Check that we can access the ODB.
        let odb = self.repo.odb()?;

        // If the repo is non-empty, verify that HEAD resolves.
        if !self.repo.is_empty()? {
            let head = self.repo.head()?;
            let oid = head.target().ok_or_else(|| {
                GitEngineError::Other("HEAD does not point to a commit".to_string())
            })?;
            // Confirm the object exists and is a commit.
            let obj_type = odb.read(oid)?.kind();
            if obj_type != git2::ObjectType::Commit {
                return Err(GitEngineError::Other(format!(
                    "HEAD points to a {:?} object, expected Commit",
                    obj_type
                )));
            }
        }

        Ok(())
    }

    /// Absolute path to the repository root (directory containing `.git/`).
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Return the current state of HEAD.
    pub fn head_state(&self) -> Result<HeadState> {
        match self.repo.head() {
            Ok(head_ref) => {
                if head_ref.is_branch() {
                    let name = head_ref.shorthand().unwrap_or_default().to_string();
                    Ok(HeadState::Branch(name))
                } else {
                    let oid_str = head_ref
                        .target()
                        .map(|oid| oid.to_string())
                        .unwrap_or_default();
                    Ok(HeadState::Detached(oid_str))
                }
            }
            Err(e) if e.code() == git2::ErrorCode::UnbornBranch => {
                // To find the unborn branch name, read `.git/HEAD` directly via libgit2 internals
                // or fallback to "main" / "master". In libgit2, an unborn branch is still pointed
                // to by HEAD as a symbolic ref.
                if let Ok(head_sym) = self.repo.find_reference("HEAD") {
                    if let Some(target) = head_sym.symbolic_target() {
                        let name = target.trim_start_matches("refs/heads/").to_string();
                        return Ok(HeadState::Unborn(name));
                    }
                }
                Ok(HeadState::Unborn("main".to_string()))
            }
            Err(e) => Err(e.into()),
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Branch operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Return metadata for the currently checked-out branch.
    ///
    /// Returns `None` when in detached HEAD state.
    pub fn current_branch(&self) -> Result<Option<BranchInfo>> {
        branch::current_branch(&self.repo)
    }

    /// List all local branches, and optionally remote-tracking branches.
    pub fn list_branches(&self, include_remote: bool) -> Result<Vec<BranchInfo>> {
        branch::list_branches(&self.repo, include_remote)
    }

    /// Create a new local branch at the current HEAD commit.
    pub fn create_branch(&self, name: &str) -> Result<BranchInfo> {
        branch::create_branch(&self.repo, name)
    }

    /// Delete a local branch by name.
    ///
    /// Returns an error when attempting to delete the currently checked-out branch.
    pub fn delete_branch(&self, name: &str) -> Result<()> {
        branch::delete_branch(&self.repo, name)
    }

    /// Check out a local branch, updating HEAD and the working tree.
    pub fn checkout_branch(&self, name: &str) -> Result<()> {
        branch::checkout_branch(&self.repo, name)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Commit operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Return metadata for the current HEAD commit, or `None` if the repo is empty.
    pub fn head_commit(&self) -> Result<Option<CommitInfo>> {
        commit_ops::head_commit(&self.repo)
    }

    /// Return the `n` most recent commits from HEAD, newest first.
    pub fn list_recent_commits(&self, n: usize) -> Result<Vec<CommitInfo>> {
        commit_ops::list_recent_commits(&self.repo, n)
    }

    /// Look up a commit by its full or abbreviated SHA-1.
    pub fn get_commit(&self, id: &str) -> Result<CommitInfo> {
        commit_ops::get_commit(&self.repo, id)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Status and diff operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Return a snapshot of the repository's working-tree and index state.
    pub fn repository_status(&self) -> Result<RepositoryStatus> {
        diff::repository_status(&self.repo)
    }

    /// Return `true` when the working tree and index have no local changes.
    pub fn is_clean(&self) -> Result<bool> {
        Ok(diff::repository_status(&self.repo)?.is_clean)
    }

    /// Produce a diff summary comparing the working tree against HEAD.
    pub fn diff_working_tree(&self) -> Result<DiffSummary> {
        diff::diff_working_tree(&self.repo)
    }

    /// Produce a diff summary comparing the staging area against HEAD.
    pub fn diff_index_to_head(&self) -> Result<DiffSummary> {
        diff::diff_index_to_head(&self.repo)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Index operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Stage all changed, deleted, and untracked files (equivalent to `git add -A`).
    pub fn stage_all(&self) -> Result<()> {
        index::stage_all(&self.repo)
    }

    /// Stage a single file by its path relative to the repository root.
    pub fn stage_file(&self, file_path: &str) -> Result<()> {
        index::stage_file(&self.repo, file_path)
    }

    /// Remove a file from the staging area without touching the working-tree copy.
    pub fn unstage_file(&self, file_path: &str) -> Result<()> {
        index::unstage_file(&self.repo, file_path)
    }

    /// Create a commit from the current staging area.
    ///
    /// Returns the full 40-character SHA-1 of the newly created commit.
    ///
    /// # Phase 2 note
    /// When `securenxt-crypto` is integrated, this will additionally attach an
    /// Ed25519 signature header to the commit object (via `git2::Commit::with_raw_header`).
    pub fn commit(
        &self,
        message: &str,
        signer: Option<&dyn CommitSigner>,
    ) -> Result<ObjectId> {
        index::commit(&self.repo, message, signer)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Merge and fetch operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Merge a local branch into the currently checked-out branch.
    ///
    /// Returns a [`MergeResult`] describing whether the merge was a
    /// fast-forward, a 3-way merge commit, or already up-to-date.
    pub fn merge(&self, branch_name: &str) -> Result<MergeResult> {
        merge_ops::merge(&self.repo, branch_name)
    }

    /// Fetch from a local filesystem repository into this one.
    ///
    /// `source_path` is the path to the other repository.
    /// `remote_name` is the local name to use for the remote (e.g. `"peer-alice"`).
    /// `refspecs` may be empty to use the default `refs/heads/*` refspec.
    pub fn fetch_local(
        &self,
        source_path: &str,
        remote_name: &str,
        refspecs: &[&str],
    ) -> Result<()> {
        merge_ops::fetch_local(&self.repo, source_path, remote_name, refspecs)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Path utilities
    // ─────────────────────────────────────────────────────────────────────────

    /// Resolve `path` (which may be a file inside the repo) to the repository root.
    ///
    /// Returns `Err(GitEngineError::InvalidPath)` when `path` is not inside a
    /// Git repository.
    pub fn resolve_repository_path(path: impl AsRef<Path>) -> Result<PathBuf> {
        resolve_repository_path(path.as_ref())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_manager_with_commit() -> (TempDir, RepositoryManager) {
        let tmp = TempDir::new().unwrap();
        let mgr = RepositoryManager::init_repository(tmp.path()).unwrap();
        fs::write(tmp.path().join("README.md"), b"# Test").unwrap();
        mgr.stage_all().unwrap();
        mgr.commit("Initial commit", None).unwrap();
        (tmp, mgr)
    }

    #[test]
    fn init_creates_repository() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("new-repo");
        let mgr = RepositoryManager::init_repository(&path).unwrap();
        assert!(path.join(".git").exists());
        assert_eq!(mgr.path(), path.canonicalize().unwrap());
    }

    #[test]
    fn init_existing_repo_fails() {
        let tmp = TempDir::new().unwrap();
        RepositoryManager::init_repository(tmp.path()).unwrap();
        let result = RepositoryManager::init_repository(tmp.path());
        assert!(matches!(result, Err(GitEngineError::AlreadyExists(_))));
    }

    #[test]
    fn open_valid_repo() {
        let tmp = TempDir::new().unwrap();
        RepositoryManager::init_repository(tmp.path()).unwrap();
        let mgr = RepositoryManager::open_repository(tmp.path()).unwrap();
        assert_eq!(mgr.path(), tmp.path().canonicalize().unwrap());
    }

    #[test]
    fn open_non_repo_fails() {
        let tmp = TempDir::new().unwrap();
        let result = RepositoryManager::open_repository(tmp.path());
        assert!(matches!(result, Err(GitEngineError::InvalidPath(_))));
    }

    #[test]
    fn clone_local_repo() {
        let (src_tmp, _) = make_manager_with_commit();
        let dst_tmp = TempDir::new().unwrap();
        let dst = dst_tmp.path().join("clone");
        let mgr = RepositoryManager::clone_repository_local(src_tmp.path(), &dst).unwrap();
        assert!(dst.join(".git").exists());
        let info = mgr.get_repository_info().unwrap();
        assert!(!info.is_empty);
    }

    #[test]
    fn repository_exists_true() {
        let tmp = TempDir::new().unwrap();
        RepositoryManager::init_repository(tmp.path()).unwrap();
        assert!(RepositoryManager::repository_exists(tmp.path()));
    }

    #[test]
    fn repository_exists_false() {
        let tmp = TempDir::new().unwrap();
        assert!(!RepositoryManager::repository_exists(tmp.path()));
    }

    #[test]
    fn get_repository_info_empty_repo() {
        let tmp = TempDir::new().unwrap();
        let mgr = RepositoryManager::init_repository(tmp.path()).unwrap();
        let info = mgr.get_repository_info().unwrap();
        assert!(info.is_empty);
        assert!(info.head_commit_id.is_none());
    }

    #[test]
    fn get_repository_info_with_commit() {
        let (_tmp, mgr) = make_manager_with_commit();
        let info = mgr.get_repository_info().unwrap();
        assert!(!info.is_empty);
        assert!(info.head_commit_id.is_some());
        assert!(info.branch.is_some());
    }

    #[test]
    fn verify_valid_repository() {
        let (_tmp, mgr) = make_manager_with_commit();
        mgr.verify_repository().unwrap();
    }

    #[test]
    fn is_clean_after_commit() {
        let (_tmp, mgr) = make_manager_with_commit();
        assert!(mgr.is_clean().unwrap());
    }

    #[test]
    fn is_clean_false_with_untracked() {
        let (tmp, mgr) = make_manager_with_commit();
        fs::write(tmp.path().join("untracked.txt"), b"new").unwrap();
        assert!(!mgr.is_clean().unwrap());
    }

    #[test]
    fn stage_commit_and_list_commits() {
        let (tmp, mgr) = make_manager_with_commit();
        fs::write(tmp.path().join("second.txt"), b"second").unwrap();
        mgr.stage_all().unwrap();
        mgr.commit("Second commit", None).unwrap();

        let commits = mgr.list_recent_commits(10).unwrap();
        assert_eq!(commits.len(), 2);
        assert!(commits[0].message.contains("Second"));
    }

    #[test]
    fn create_and_checkout_branch() {
        let (_tmp, mgr) = make_manager_with_commit();
        mgr.create_branch("dev").unwrap();
        mgr.checkout_branch("dev").unwrap();
        let current = mgr.current_branch().unwrap().unwrap();
        assert_eq!(current.name, "dev");
    }

    #[test]
    fn diff_shows_modifications() {
        let (tmp, mgr) = make_manager_with_commit();
        fs::write(tmp.path().join("README.md"), b"# Modified\nNew line\n").unwrap();
        let diff = mgr.diff_working_tree().unwrap();
        assert!(diff.files_changed >= 1);
    }

    #[test]
    fn resolve_repository_path_from_subdir() {
        let (tmp, _mgr) = make_manager_with_commit();
        let subdir = tmp.path().join("src");
        fs::create_dir(&subdir).unwrap();
        let resolved = RepositoryManager::resolve_repository_path(&subdir).unwrap();
        assert_eq!(resolved, tmp.path().canonicalize().unwrap());
    }
}
