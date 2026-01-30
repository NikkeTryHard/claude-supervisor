//! Git worktree management for session isolation.
//!
//! This module provides functionality for creating, managing, and cleaning up
//! git worktrees to isolate Claude Code sessions from each other.

mod error;
mod manager;
mod registry;
mod types;

pub use error::WorktreeError;
pub use manager::WorktreeManager;
pub use registry::WorktreeRegistry;
pub use types::{Worktree, WorktreeStatus};
