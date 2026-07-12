//! Error types for the Securenxt Repository Engine.
//!
//! [`GitEngineError`] is the single error type returned by every public API in
//! this crate.  It converts transparently from [`git2::Error`] via the `?`
//! operator, but intercepted error codes are mapped to richer semantic variants
//! so callers never need to match on libgit2 internals.
//!
//! # Error Philosophy (mirrors Git's approach)
//!
//! Git internally distinguishes between "object not found in ODB", "reference
//! not found", "unborn HEAD", "detached HEAD", etc.  We preserve those
//! distinctions here so that each layer (UI, CLI, sync engine) can give the
//! user an accurate message without needing to parse error strings.

use thiserror::Error;

/// All errors that can arise from repository engine operations.
#[derive(Debug, Error)]
pub enum GitEngineError {
    // ── Object / commit errors ────────────────────────────────────────────────

    /// The requested repository, object, branch, commit, or file was not found.
    #[error("not found: {0}")]
    NotFound(String),

    /// A repository, branch, or tag already exists at the given location.
    #[error("already exists: {0}")]
    AlreadyExists(String),

    /// The supplied path is invalid, non-existent, or not a Git repository root.
    #[error("invalid path: {0}")]
    InvalidPath(String),

    // ── HEAD / ref state errors ───────────────────────────────────────────────

    /// The repository has no commits yet (HEAD is unborn).
    ///
    /// Returned by operations that require at least one commit (e.g. listing
    /// branches, reading HEAD commit).  Callers should guide the user to make
    /// an initial commit.
    #[error("repository '{0}' has no commits yet (HEAD is unborn)")]
    EmptyRepository(String),

    /// HEAD is in a detached state (pointing directly at a commit OID rather
    /// than a branch reference).
    ///
    /// Returned when the caller requests "current branch" but there is none.
    #[error("HEAD is detached at '{0}'")]
    HeadDetached(String),

    // ── Branch errors ─────────────────────────────────────────────────────────

    /// A branch operation failed (e.g. checkout of a dirty tree).
    #[error("branch error: {0}")]
    BranchError(String),

    /// A Git reference (branch name, tag, refspec) is syntactically invalid.
    #[error("invalid reference '{0}': {1}")]
    InvalidReference(String, String),

    // ── Commit / index errors ─────────────────────────────────────────────────

    /// An error during a commit operation (e.g. nothing staged, bad author).
    #[error("commit error: {0}")]
    CommitError(String),

    // ── Merge errors ──────────────────────────────────────────────────────────

    /// A merge produced conflicts that must be resolved manually.
    #[error("merge conflict in {file_count} file(s): resolve conflicts and commit")]
    MergeConflict {
        /// Number of files with unresolved conflicts.
        file_count: usize,
    },

    /// The operation requires a clean working tree but the repository is dirty.
    #[error("working tree is not clean: stage or stash your changes first")]
    DirtyWorkingTree,

    // ── Transport / ODB errors ────────────────────────────────────────────────

    /// An I/O error occurred while reading or writing the filesystem.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    // ── Low-level libgit2 passthrough ─────────────────────────────────────────

    /// A direct error from the underlying libgit2 library that did not map to
    /// any specific `GitEngineError` variant.
    ///
    /// This is intentionally a catch-all: internal code uses
    /// [`GitEngineError::from_git2`] to convert `git2::Error`, which attempts
    /// to map well-known error codes first and falls back here.
    #[error("git2 error ({code:?}): {message}")]
    Git2Error {
        /// libgit2 error code.
        code: git2::ErrorCode,
        /// Human-readable message from libgit2.
        message: String,
    },

    /// A catch-all for unexpected conditions that do not fit other variants.
    #[error("unexpected error: {0}")]
    Other(String),
}

impl GitEngineError {
    /// Convert a `git2::Error` into the most specific `GitEngineError` variant.
    ///
    /// This is the single conversion point used throughout the crate, replacing
    /// the blanket `#[from] git2::Error` so that callers receive semantically
    /// rich errors instead of opaque libgit2 messages.
    ///
    /// # Mapping table
    ///
    /// | `git2::ErrorCode`      | `GitEngineError` variant   |
    /// |------------------------|----------------------------|
    /// | `NotFound`             | `NotFound`                 |
    /// | `Exists`               | `AlreadyExists`            |
    /// | `UnbornBranch`         | `EmptyRepository`          |
    /// | `Conflict`             | `MergeConflict { 0 }`      |
    /// | `InvalidSpec`          | `InvalidReference`         |
    /// | everything else        | `Git2Error`                |
    pub(crate) fn from_git2(e: git2::Error) -> Self {
        match e.code() {
            git2::ErrorCode::NotFound => GitEngineError::NotFound(e.message().to_string()),
            git2::ErrorCode::Exists => GitEngineError::AlreadyExists(e.message().to_string()),
            git2::ErrorCode::UnbornBranch => {
                GitEngineError::EmptyRepository(e.message().to_string())
            }
            git2::ErrorCode::Conflict => GitEngineError::MergeConflict { file_count: 0 },
            git2::ErrorCode::InvalidSpec => {
                GitEngineError::InvalidReference(String::new(), e.message().to_string())
            }
            code => GitEngineError::Git2Error {
                code,
                message: e.message().to_string(),
            },
        }
    }
}

/// Allow `?` on `git2::Result` values within the crate.
impl From<git2::Error> for GitEngineError {
    fn from(e: git2::Error) -> Self {
        Self::from_git2(e)
    }
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, GitEngineError>;
