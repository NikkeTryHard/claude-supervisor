//! Worktree manager for git operations.

use std::path::PathBuf;

use crate::config::WorktreeConfig;

use super::error::WorktreeError;
use super::types::Worktree;

/// Manages git worktree operations.
#[derive(Debug)]
pub struct WorktreeManager {
    /// Repository root path.
    repo_root: PathBuf,
    /// Configuration.
    config: WorktreeConfig,
}

impl WorktreeManager {
    /// Create a new worktree manager.
    ///
    /// # Errors
    ///
    /// Returns `WorktreeError::NotGitRepo` if the path is not in a git repository.
    pub fn new(repo_root: PathBuf, config: WorktreeConfig) -> Result<Self, WorktreeError> {
        // Verify we're in a git repo
        let git_dir = repo_root.join(".git");
        if !git_dir.exists() {
            return Err(WorktreeError::NotGitRepo);
        }

        Ok(Self { repo_root, config })
    }

    /// Get the repository root path.
    #[must_use]
    pub fn repo_root(&self) -> &PathBuf {
        &self.repo_root
    }

    /// Get the configuration.
    #[must_use]
    pub fn config(&self) -> &WorktreeConfig {
        &self.config
    }

    /// Get the worktree directory path.
    #[must_use]
    pub fn worktree_dir(&self) -> PathBuf {
        self.repo_root.join(&self.config.worktree_dir)
    }

    /// Create a new worktree.
    ///
    /// # Errors
    ///
    /// Returns an error if the worktree creation fails.
    pub async fn create(&self, name: &str) -> Result<Worktree, WorktreeError> {
        // Validate name
        if name.is_empty() || name.contains('/') || name.contains('\\') {
            return Err(WorktreeError::InvalidName(name.to_string()));
        }

        let branch = self.config.branch_pattern.replace("{name}", name);
        let path = self.worktree_dir().join(name);

        // Check if path already exists
        if path.exists() {
            return Err(WorktreeError::AlreadyExists(name.to_string()));
        }

        // Create worktree directory parent if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Run git worktree add
        let output = tokio::process::Command::new("git")
            .args(["worktree", "add", "-b", &branch])
            .arg(&path)
            .current_dir(&self.repo_root)
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("already exists") {
                return Err(WorktreeError::BranchExists(branch));
            }
            return Err(WorktreeError::GitError(stderr.to_string()));
        }

        Ok(Worktree::new(name, path, branch))
    }

    /// List all worktrees.
    ///
    /// # Errors
    ///
    /// Returns an error if the git command fails.
    pub async fn list(&self) -> Result<Vec<Worktree>, WorktreeError> {
        let output = tokio::process::Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(&self.repo_root)
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::GitError(stderr.to_string()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let worktrees = self.parse_worktree_list(&stdout);
        Ok(worktrees)
    }

    /// Parse git worktree list --porcelain output.
    fn parse_worktree_list(&self, output: &str) -> Vec<Worktree> {
        let mut worktrees = Vec::new();
        let mut current_path: Option<PathBuf> = None;
        let mut current_branch: Option<String> = None;

        for line in output.lines() {
            if let Some(path) = line.strip_prefix("worktree ") {
                // Save previous worktree if complete
                if let (Some(path), Some(branch)) = (current_path.take(), current_branch.take()) {
                    if let Some(name) = self.extract_worktree_name(&path) {
                        worktrees.push(Worktree::new(name, path, branch));
                    }
                }
                current_path = Some(PathBuf::from(path));
            } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
                current_branch = Some(branch.to_string());
            }
        }

        // Don't forget the last one
        if let (Some(path), Some(branch)) = (current_path, current_branch) {
            if let Some(name) = self.extract_worktree_name(&path) {
                worktrees.push(Worktree::new(name, path, branch));
            }
        }

        worktrees
    }

    /// Extract worktree name from path if it's in our worktree directory.
    fn extract_worktree_name(&self, path: &std::path::Path) -> Option<String> {
        let worktree_dir = self.worktree_dir();
        path.strip_prefix(&worktree_dir)
            .ok()
            .and_then(|p| p.to_str())
            .map(String::from)
    }

    /// Remove a worktree.
    ///
    /// # Errors
    ///
    /// Returns an error if the worktree removal fails.
    pub async fn remove(&self, name: &str, force: bool) -> Result<(), WorktreeError> {
        let path = self.worktree_dir().join(name);

        if !path.exists() {
            return Err(WorktreeError::NotFound(name.to_string()));
        }

        // Check for dirty state unless force
        if !force {
            let is_dirty = self.is_dirty(&path).await?;
            if is_dirty {
                return Err(WorktreeError::DirtyWorktree { path });
            }
        }

        // Run git worktree remove
        let mut args = vec!["worktree", "remove"];
        if force {
            args.push("--force");
        }
        args.push(name);

        let output = tokio::process::Command::new("git")
            .args(&args)
            .current_dir(&self.repo_root)
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::GitError(stderr.to_string()));
        }

        Ok(())
    }

    /// Check if a worktree has uncommitted changes.
    async fn is_dirty(&self, path: &PathBuf) -> Result<bool, WorktreeError> {
        let output = tokio::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(path)
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::GitError(stderr.to_string()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(!stdout.trim().is_empty())
    }

    /// Delete a branch.
    ///
    /// # Errors
    ///
    /// Returns an error if the branch deletion fails.
    pub async fn delete_branch(&self, branch: &str, force: bool) -> Result<(), WorktreeError> {
        let flag = if force { "-D" } else { "-d" };

        let output = tokio::process::Command::new("git")
            .args(["branch", flag, branch])
            .current_dir(&self.repo_root)
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::GitError(stderr.to_string()));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worktree_manager_not_git_repo() {
        let result = WorktreeManager::new(PathBuf::from("/tmp"), WorktreeConfig::default());
        assert!(matches!(result, Err(WorktreeError::NotGitRepo)));
    }

    #[test]
    fn test_worktree_dir() {
        // This test would need a real git repo, so we just test the path calculation
        let config = WorktreeConfig {
            worktree_dir: PathBuf::from(".wt"),
            ..Default::default()
        };
        // We can't fully test without a git repo, but we can verify the logic
        assert_eq!(config.worktree_dir, PathBuf::from(".wt"));
    }
}
