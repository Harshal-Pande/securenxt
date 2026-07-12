//! Public data types returned by the Repository Engine.
//!
//! All types in this module are pure data structures with **no** `git2`
//! references, lifetimes, or raw pointers.  They can be:
//!
//! - Cheaply cloned and sent across threads (`Send + 'static`)
//! - Serialised to JSON for Tauri IPC (`serde::Serialize`)
//! - Deserialised from JSON for tests and the CLI (`serde::Deserialize`)
//! - Stored in SQLite in Phase 4 (all fields are plain Rust scalars / strings)
//!
//! # Alignment with Git's object model
//!
//! Git stores everything as content-addressed objects identified by SHA-1.
//! Our types mirror this:
//!
//! - [`ObjectId`] — the SHA-1 hash of any Git object (blob / tree / commit / tag)
//! - [`CommitInfo`] — the Securenxt view of a commit object
//! - [`BranchInfo`] — the Securenxt view of a `refs/heads/<name>` pointer
//! - [`RepositoryInfo`] — the Securenxt view of an opened repository
//! - [`HeadState`] — the three distinct states of `HEAD` Git can be in
//! - [`RepositoryStatus`] — the three-way comparison (HEAD ↔ index ↔ worktree)
//! - [`DiffSummary`] — aggregate statistics from Git's diff engine

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// ObjectId
// ─────────────────────────────────────────────────────────────────────────────

/// A validated Git object identifier (SHA-1 hash).
///
/// `ObjectId` is a newtype over `String` that guarantees the contained value
/// is a well-formed 40-character lowercase hexadecimal string — exactly the
/// format libgit2 produces for every object it writes to the ODB.
///
/// # Rationale
///
/// Git's design principle is that every object is immutably identified by its
/// content hash.  By wrapping the raw string in a validated newtype we:
///
/// 1. Prevent accidentally passing arbitrary strings where an OID is required.
/// 2. Make the type signature self-documenting.
/// 3. Enable Phase 3 (Sync) to use `ObjectId` as a stable cross-device key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ObjectId(String);

impl ObjectId {
    /// Construct an `ObjectId` from a 40-character hex string.
    ///
    /// Returns `None` if `s` is not exactly 40 lowercase hex characters.
    pub fn new(s: impl Into<String>) -> Option<Self> {
        let s = s.into();
        if s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit()) {
            Some(Self(s))
        } else {
            None
        }
    }

    /// Construct without validation.  For internal use only.
    pub(crate) fn from_oid_unchecked(s: String) -> Self {
        Self(s)
    }

    /// The raw 40-character hex string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Abbreviated display form (first 8 characters).
    pub fn short(&self) -> &str {
        &self.0[..8]
    }
}

impl std::fmt::Display for ObjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// HeadState
// ─────────────────────────────────────────────────────────────────────────────

/// The three distinct states that `HEAD` can be in.
///
/// Git's `HEAD` is stored in `.git/HEAD` as either:
///
/// - `ref: refs/heads/<name>` — a symbolic ref to a branch (most common)
/// - A raw 40-character SHA-1 — detached HEAD (after `git checkout <sha>`)
///
/// The "unborn" state arises when the repository has no commits yet; `HEAD`
/// contains `ref: refs/heads/main` but that ref file does not yet exist.
///
/// This enum makes all three states explicit so callers can branch correctly
/// without inspecting strings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeadState {
    /// HEAD is a symbolic ref pointing at a branch that has no commits yet.
    /// The inner string is the branch name (e.g. `"main"`).
    Unborn(String),

    /// HEAD points directly at a commit SHA-1 (detached HEAD state).
    /// The inner string is the full 40-character OID.
    Detached(String),

    /// HEAD is a symbolic ref pointing at an existing branch.
    /// The inner string is the branch name (e.g. `"main"`, `"feature/x"`).
    Branch(String),
}

impl HeadState {
    /// Returns the branch name when `HEAD` is attached, `None` otherwise.
    pub fn branch_name(&self) -> Option<&str> {
        match self {
            Self::Branch(name) | Self::Unborn(name) => Some(name),
            Self::Detached(_) => None,
        }
    }

    /// Returns `true` when the repository has no commits.
    pub fn is_unborn(&self) -> bool {
        matches!(self, Self::Unborn(_))
    }

    /// Returns `true` when HEAD is not attached to a branch.
    pub fn is_detached(&self) -> bool {
        matches!(self, Self::Detached(_))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Repository-level types
// ─────────────────────────────────────────────────────────────────────────────

/// High-level metadata about a single repository.
///
/// Mirrors the information Git stores in `.git/` at open time:
/// the current HEAD state, the path, and basic flags (bare, empty).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryInfo {
    /// Absolute, canonicalised path to the repository root (the directory
    /// containing `.git/`).
    pub path: String,

    /// The last path component of `path` — used as a human-readable name.
    pub name: String,

    /// Full 40-character SHA-1 of the current HEAD commit, or `None` when the
    /// repository has no commits yet (initial state / empty repo).
    pub head_commit_id: Option<ObjectId>,

    /// The current state of HEAD.
    ///
    /// Use this instead of `branch` when you need to distinguish between
    /// "no commits yet", "detached", and "on a named branch".
    pub head_state: HeadState,

    /// Name of the currently checked-out branch (e.g. `"main"`).
    /// `None` when the repository is in a detached HEAD state or has no commits.
    pub branch: Option<String>,

    /// `true` when the repository contains no commits.
    pub is_empty: bool,

    /// `true` for bare repositories (no working tree).
    pub is_bare: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Branch types
// ─────────────────────────────────────────────────────────────────────────────

/// Metadata about a single Git branch.
///
/// A branch in Git is nothing more than a movable pointer (a file in
/// `refs/heads/`) to a commit object.  This struct captures that pointer
/// along with extra contextual metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchInfo {
    /// Short branch name (e.g. `"main"`, `"feature/login"`).
    pub name: String,

    /// `true` for remote-tracking branches (e.g. `origin/main`).
    pub is_remote: bool,

    /// Name of the upstream tracking branch if one is configured.
    pub upstream: Option<String>,

    /// Full SHA-1 of the commit this branch currently points to.
    pub commit_id: ObjectId,

    /// `true` when this is the currently checked-out branch.
    pub is_head: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Commit types
// ─────────────────────────────────────────────────────────────────────────────

/// Metadata about a single Git commit object.
///
/// A Git commit object contains:
/// - A pointer to a tree (the root directory snapshot at that moment)
/// - Zero or more parent commit pointers (forming the DAG)
/// - Author/committer information
/// - The commit message
///
/// We expose all DAG-relevant fields (`id`, `parent_ids`) so the Sync Engine
/// (Phase 3) can reconstruct the commit graph without re-opening the ODB.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitInfo {
    /// Full 40-character SHA-1 object ID of the commit.
    pub id: ObjectId,

    /// Abbreviated 8-character SHA-1 (for display purposes).
    pub short_id: String,

    /// Full commit message (subject line + optional body).
    pub message: String,

    /// Author's display name.
    pub author: String,

    /// Author's email address.
    pub email: String,

    /// When the commit was authored, in UTC.
    pub timestamp: DateTime<Utc>,

    /// Full SHA-1 IDs of all parent commits.
    ///
    /// - Empty `Vec` → initial commit (root of the DAG)
    /// - One entry → normal commit
    /// - Two or more entries → merge commit
    pub parent_ids: Vec<ObjectId>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Status types
// ─────────────────────────────────────────────────────────────────────────────

/// The condition of a single file as seen through Git's three-way comparison.
///
/// Git compares three states:
/// 1. **HEAD** — the last committed tree (object in the ODB)
/// 2. **Index** — the staged snapshot (`.git/index`)
/// 3. **Working Tree** — the files on disk
///
/// `FileStatusKind` describes the relationship between two adjacent layers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileStatusKind {
    /// New file, not yet tracked by Git (worktree only, not in index or HEAD).
    Untracked,
    /// File exists in the index but not in HEAD (newly staged for the first time).
    Added,
    /// File content has changed between the two states being compared.
    Modified,
    /// File has been removed from the layer being compared against.
    Deleted,
    /// File has been renamed (path changed between the two states).
    Renamed,
    /// File has unresolved merge conflicts (both sides modified the same region).
    Conflicted,
    /// A type change occurred (e.g. regular file → symlink, or vice versa).
    Typechange,
}

/// Status of one file in the repository with separate index- and worktree-views.
///
/// Models Git's three-way comparison explicitly:
/// - `index_status`: how this file differs between HEAD and the index (staged changes)
/// - `worktree_status`: how this file differs between the index and disk (unstaged changes)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStatus {
    /// Relative path from the repository root (POSIX separators).
    pub path: String,

    /// Optional rename target when `index_status` is [`FileStatusKind::Renamed`].
    pub new_path: Option<String>,

    /// Status relative to the current HEAD commit (staged changes).
    /// `None` when the file has no staged changes.
    pub index_status: Option<FileStatusKind>,

    /// Status relative to the index (unstaged changes in the working tree).
    /// `None` when the working tree copy matches the index.
    pub worktree_status: Option<FileStatusKind>,
}

/// A snapshot of the repository's working-tree and index state.
///
/// Produced by comparing HEAD ↔ index ↔ working tree — the same comparison
/// `git status` performs internally.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryStatus {
    /// Currently checked-out branch name, or `"HEAD"` in detached state.
    pub branch: String,

    /// Number of local commits ahead of the upstream (0 if no upstream set).
    pub ahead: usize,

    /// Number of upstream commits the local branch is behind (0 if no upstream set).
    pub behind: usize,

    /// Files with staged changes (index differs from HEAD).
    pub staged_files: Vec<FileStatus>,

    /// Files with unstaged changes (working tree differs from index).
    pub unstaged_files: Vec<FileStatus>,

    /// Paths not tracked by Git (present on disk but not in the index).
    pub untracked_files: Vec<String>,

    /// `true` when staged, unstaged, and untracked are all empty.
    pub is_clean: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Diff types
// ─────────────────────────────────────────────────────────────────────────────

/// A single contiguous block of changed lines (a "hunk") inside a unified diff.
///
/// Corresponds to Git's diff hunk header line: `@@ -a,b +c,d @@`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    /// The `@@ … @@` header line produced by the diff engine.
    pub header: String,

    /// Number of lines added in this hunk.
    pub added_lines: usize,

    /// Number of lines removed in this hunk.
    pub removed_lines: usize,
}

/// Aggregated diff statistics between two tree states.
///
/// Git's diff engine compares any two trees (or a tree against the index /
/// working tree).  This struct captures the aggregate statistics useful for
/// UIs and the CLI without exposing the raw diff bytes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffSummary {
    /// Total lines inserted across all changed files.
    pub insertions: usize,

    /// Total lines deleted across all changed files.
    pub deletions: usize,

    /// Number of files that differ between the two states.
    pub files_changed: usize,

    /// Per-hunk details (one entry per `@@ … @@` block).
    pub hunks: Vec<DiffHunk>,
}

impl DiffSummary {
    /// Constructs an empty diff (no changes between the two states).
    pub fn empty() -> Self {
        Self {
            insertions: 0,
            deletions: 0,
            files_changed: 0,
            hunks: Vec::new(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Merge result type
// ─────────────────────────────────────────────────────────────────────────────

/// Outcome of a [`crate::RepositoryManager::merge`] call.
///
/// Returned instead of a plain `()` so callers can distinguish between a
/// no-op, a fast-forward pointer advance, and a true 3-way merge commit.
///
/// # Git merge strategies
///
/// - **Already up-to-date**: the target commit is already an ancestor of HEAD.
///   Git does nothing; we propagate that as `AlreadyUpToDate`.
/// - **Fast-forward**: HEAD is an ancestor of the target.  Git moves the branch
///   pointer forward without creating a merge commit.
/// - **3-way merge**: the two branches have diverged.  Git creates a new commit
///   with two parents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MergeResult {
    /// HEAD was already pointing at or ahead of the target — nothing changed.
    AlreadyUpToDate,

    /// The merge was resolved by advancing HEAD (no merge commit was created).
    /// Contains the full SHA-1 of the new HEAD.
    FastForward(ObjectId),

    /// A 3-way merge was performed and a merge commit was created.
    /// Contains the full SHA-1 of the merge commit.
    MergeCommit(ObjectId),
}

// ─────────────────────────────────────────────────────────────────────────────
// Phase 2 extension point: CommitSigner
// ─────────────────────────────────────────────────────────────────────────────

/// Trait that Phase 2 (Identity & Trust) will implement to attach an
/// Ed25519 signature to every commit object.
///
/// In Phase 1 this trait has no implementations; the `commit()` path receives
/// `Option<&dyn CommitSigner>` which is always `None`.
///
/// Phase 2 will provide an `Ed25519Signer` struct from `securenxt-crypto` that
/// implements this trait. The only change needed at the call site is passing
/// `Some(&signer)` instead of `None`.
///
/// # Sealed
///
/// This trait is sealed: it can only be implemented by types within this crate
/// or by the `securenxt-crypto` crate via an explicit `impl`.  The sealing
/// prevents third-party code from inadvertently bypassing Securenxt's
/// identity model.
pub trait CommitSigner: private::Sealed + Send + Sync {
    /// Produce an Ed25519 signature over the commit's canonical byte representation.
    ///
    /// `data` is the raw bytes of the commit object that libgit2 will hash and
    /// store.  The returned `Vec<u8>` is the DER-encoded signature.
    fn sign(&self, data: &[u8]) -> Vec<u8>;

    /// The Ed25519 public key that corresponds to the signing key, in raw 32-byte
    /// form.  Stored in the commit header so verifiers can check without a
    /// key-distribution lookup.
    fn public_key_bytes(&self) -> [u8; 32];
}

/// Sealing module — keeps `CommitSigner` unimplementable outside approved crates.
#[doc(hidden)]
pub mod private {
    /// Marker trait.  Cannot be named from outside this module.
    pub trait Sealed {}
}
