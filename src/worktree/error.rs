//! Worktree error types.

use std::path::PathBuf;

/// Errors that can occur during worktree operations.
#[derive(thiserror::Error, Debug)]
pub enum WorktreeError {
    /// Not in a git repository.
    #[error("Not in a git repository")]
    NotGitRepo,

    /// Git command failed.
    #[error("Git command failed: {0}")]
    GitError(String),

    /// Worktree already exists.
    #[error("Worktree already exists: {0}")]
    AlreadyExists(String),

    /// Worktree not found.
    #[error("Worktree not found: {0}")]
    NotFound(String),

    /// Worktree has uncommitted changes.
    #[error("Worktree has uncommitted changes: {path}")]
    DirtyWorktree { path: PathBuf },

    /// Branch already exists.
    #[error("Branch already exists: {0}")]
    BranchExists(String),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Invalid worktree name.
    #[error("Invalid worktree name: {0}")]
    InvalidName(String),
}
