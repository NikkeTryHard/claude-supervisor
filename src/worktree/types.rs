//! Worktree types.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Status of a worktree.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeStatus {
    /// Worktree is active and in use.
    Active,
    /// Worktree is idle (not currently in use).
    #[default]
    Idle,
    /// Worktree has been marked for cleanup.
    PendingCleanup,
}

/// Represents a git worktree managed by the supervisor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Worktree {
    /// Unique name for this worktree.
    pub name: String,

    /// Absolute path to the worktree directory.
    pub path: PathBuf,

    /// Branch name associated with this worktree.
    pub branch: String,

    /// Current status.
    #[serde(default)]
    pub status: WorktreeStatus,

    /// When the worktree was created.
    pub created_at: DateTime<Utc>,

    /// When the worktree was last accessed.
    #[serde(default)]
    pub last_accessed: Option<DateTime<Utc>>,

    /// Session ID currently using this worktree, if any.
    #[serde(default)]
    pub session_id: Option<String>,
}

impl Worktree {
    /// Create a new worktree record.
    #[must_use]
    pub fn new(name: impl Into<String>, path: PathBuf, branch: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            path,
            branch: branch.into(),
            status: WorktreeStatus::Idle,
            created_at: Utc::now(),
            last_accessed: None,
            session_id: None,
        }
    }

    /// Mark the worktree as active with a session ID.
    pub fn activate(&mut self, session_id: impl Into<String>) {
        self.status = WorktreeStatus::Active;
        self.session_id = Some(session_id.into());
        self.last_accessed = Some(Utc::now());
    }

    /// Mark the worktree as idle.
    pub fn deactivate(&mut self) {
        self.status = WorktreeStatus::Idle;
        self.session_id = None;
        self.last_accessed = Some(Utc::now());
    }

    /// Mark the worktree for cleanup.
    pub fn mark_for_cleanup(&mut self) {
        self.status = WorktreeStatus::PendingCleanup;
    }

    /// Check if the worktree is currently active.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.status == WorktreeStatus::Active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worktree_new() {
        let wt = Worktree::new("test", PathBuf::from("/tmp/test"), "main");
        assert_eq!(wt.name, "test");
        assert_eq!(wt.path, PathBuf::from("/tmp/test"));
        assert_eq!(wt.branch, "main");
        assert_eq!(wt.status, WorktreeStatus::Idle);
        assert!(wt.session_id.is_none());
    }

    #[test]
    fn test_worktree_activate() {
        let mut wt = Worktree::new("test", PathBuf::from("/tmp/test"), "main");
        wt.activate("session-123");
        assert!(wt.is_active());
        assert_eq!(wt.session_id, Some("session-123".to_string()));
        assert!(wt.last_accessed.is_some());
    }

    #[test]
    fn test_worktree_deactivate() {
        let mut wt = Worktree::new("test", PathBuf::from("/tmp/test"), "main");
        wt.activate("session-123");
        wt.deactivate();
        assert!(!wt.is_active());
        assert!(wt.session_id.is_none());
    }

    #[test]
    fn test_worktree_status_serialize() {
        let status = WorktreeStatus::Active;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"active\"");

        let status = WorktreeStatus::PendingCleanup;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"pending_cleanup\"");
    }
}
