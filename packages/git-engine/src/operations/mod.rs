//! Repository operation sub-modules.
//!
//! Each module is focused on a single concern.  All functions here receive a
//! `&git2::Repository` reference and return `crate::error::Result<T>`.
//! Higher-level orchestration lives in [`crate::manager::RepositoryManager`].

pub(crate) mod branch;
pub(crate) mod commit;
pub(crate) mod diff;
pub(crate) mod index;
pub(crate) mod merge;
