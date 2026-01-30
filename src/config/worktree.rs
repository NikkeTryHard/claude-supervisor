//! Worktree configuration.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Configuration for git worktree isolation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeConfig {
    /// Whether worktree isolation is enabled.
    #[serde(default)]
    pub enabled: bool,

    /// Directory where worktrees are created (relative to repo root).
    #[serde(default = "default_worktree_dir")]
    pub worktree_dir: PathBuf,

    /// Whether to automatically clean up worktrees on session end.
    #[serde(default)]
    pub auto_cleanup: bool,

    /// Branch name pattern for worktrees. Use {name} as placeholder.
    #[serde(default = "default_branch_pattern")]
    pub branch_pattern: String,
}

fn default_worktree_dir() -> PathBuf {
    PathBuf::from(".worktrees")
}

fn default_branch_pattern() -> String {
    "supervisor/{name}".to_string()
}

impl Default for WorktreeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            worktree_dir: default_worktree_dir(),
            auto_cleanup: false,
            branch_pattern: default_branch_pattern(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worktree_config_default() {
        let config = WorktreeConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.worktree_dir, PathBuf::from(".worktrees"));
        assert!(!config.auto_cleanup);
        assert_eq!(config.branch_pattern, "supervisor/{name}");
    }

    #[test]
    fn test_worktree_config_deserialize_defaults() {
        let json = "{}";
        let config: WorktreeConfig = serde_json::from_str(json).unwrap();
        assert!(!config.enabled);
        assert_eq!(config.worktree_dir, PathBuf::from(".worktrees"));
    }

    #[test]
    fn test_worktree_config_deserialize_custom() {
        let json = r#"{
            "enabled": true,
            "worktree_dir": ".wt",
            "auto_cleanup": true,
            "branch_pattern": "agent/{name}"
        }"#;
        let config: WorktreeConfig = serde_json::from_str(json).unwrap();
        assert!(config.enabled);
        assert_eq!(config.worktree_dir, PathBuf::from(".wt"));
        assert!(config.auto_cleanup);
        assert_eq!(config.branch_pattern, "agent/{name}");
    }

    #[test]
    fn test_worktree_config_serialize() {
        let config = WorktreeConfig {
            enabled: true,
            worktree_dir: PathBuf::from(".worktrees"),
            auto_cleanup: true,
            branch_pattern: "test/{name}".to_string(),
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"enabled\":true"));
        assert!(json.contains("\"auto_cleanup\":true"));
    }
}
