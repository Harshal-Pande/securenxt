//! # securenxt-git-engine
//!
//! Repository Engine for the Securenxt secure decentralized collaboration platform.
//!
//! This crate wraps `libgit2` (via `git2-rs`) and provides a clean, typed API for
//! all local repository management operations required by Securenxt.  It is
//! **Phase 1** of the platform — no networking, no Bluetooth, no cryptographic
//! identity.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use securenxt_git_engine::RepositoryManager;
//!
//! // Initialise a new repository.
//! let mgr = RepositoryManager::init_repository("/path/to/new-repo").unwrap();
//!
//! // Or open an existing one (path may be any subdirectory).
//! let mgr = RepositoryManager::open_repository("/path/to/repo/src/main.rs").unwrap();
//!
//! // Stage everything and commit.
//! mgr.stage_all().unwrap();
//! let oid = mgr.commit("feat: initial implementation").unwrap();
//! println!("committed: {oid}");
//!
//! // Inspect status.
//! let status = mgr.repository_status().unwrap();
//! println!("clean: {}", status.is_clean);
//! ```
//!
//! ## Phase 2 preparation
//!
//! Every commit-creation path in this crate contains an annotated injection point
//! (`// Phase 2: inject Ed25519Signer here`) ready for the Identity & Trust phase.
//! Re-adding `securenxt-crypto` as a dependency and passing an `Ed25519Signer`
//! through to [`manager::RepositoryManager::commit`] is the only change needed.
//!
//! ## Module structure
//!
//! | Module | Responsibility |
//! |--------|----------------|
//! | [`error`] | Error types (`GitEngineError`, `Result`) |
//! | [`types`] | Data types (`RepositoryInfo`, `CommitInfo`, …) |
//! | [`utils`] | Path resolution, timestamp helpers (crate-internal) |
//! | [`manager`] | `RepositoryManager` — public entry point |
//! | `operations::branch` | Branch CRUD and checkout |
//! | `operations::commit` | Commit inspection |
//! | `operations::diff` | Status and diff |
//! | `operations::index` | Stage, unstage, commit |
//! | `operations::merge` | Merge and local fetch |

// Enforce documentation on all public items.
#![warn(missing_docs)]
// Deny the most dangerous footguns in production code.
#![deny(clippy::unwrap_in_result)]

pub mod error;
pub mod manager;
pub mod types;

// Operations are pub(crate) — consumers use RepositoryManager, not raw functions.
pub(crate) mod operations;
pub(crate) mod utils;

// Convenience re-exports at the crate root so callers can write:
//   use securenxt_git_engine::{RepositoryManager, GitEngineError};
pub use error::{GitEngineError, Result};
pub use manager::RepositoryManager;
pub use types::{
    BranchInfo, CommitInfo, CommitSigner, DiffHunk, DiffSummary, FileStatus, FileStatusKind,
    HeadState, MergeResult, ObjectId, RepositoryInfo, RepositoryStatus,
};
