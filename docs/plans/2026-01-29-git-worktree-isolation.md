# Git Worktree Isolation Implementation Plan

> **REQUIRED:** Use `execute-plan` to implement this plan batch by batch.

**Goal:** Run Claude Code in isolated git worktrees for safe experimentation, with automatic cleanup and merge capabilities.

**Architecture:** A new `worktree` module manages worktree lifecycle via `git worktree` CLI commands. `WorktreeManager` creates/removes worktrees, `WorktreeRegistry` persists state to `.worktrees/state.json`. `ClaudeProcessBuilder` gains a `working_dir` field to spawn Claude in the worktree directory.

**Tech Stack:** tokio::process::Command for git CLI, serde_json for state persistence, chrono for timestamps.

**Issue:** #27

---

## Batch 1: WorktreeConfig and Module Scaffold

**Goal:** Create the worktree module structure and configuration types.

### Task 1.1: Create WorktreeConfig

**Files:**
- Create: `src/config/worktree.rs`
- Modify: `src/config/mod.rs:1-11`
- Modify: `src/config/types.rs:39-70`
- Test: `src/config/worktree.rs` (inline tests)

**Step 1: Write failing test**

Create `src/config/worktree.rs`:
```rust
//! Worktree configuration.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Configuration for git worktree isolation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeConfig {
    /// Whether worktree isolation is enabled.
    #[serde(default)]
    pub enabled: bool,

    /// Base directory for worktrees (relative to project root).
    #[serde(default = "default_worktree_dir")]
    pub worktree_dir: PathBuf,

    /// Automatically clean up worktrees after task completion.
    #[serde(default)]
    pub auto_cleanup: bool,

    /// Branch naming pattern. Use {name} as placeholder.
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
            "branch_pattern": "feature/{name}"
        }"#;
        let config: WorktreeConfig = serde_json::from_str(json).unwrap();
        assert!(config.enabled);
        assert_eq!(config.worktree_dir, PathBuf::from(".wt"));
        assert!(config.auto_cleanup);
        assert_eq!(config.branch_pattern, "feature/{name}");
    }
}
```

**Step 2: Verify failure**

Run: `cargo t worktree_config -v`

Expected: FAIL with "unresolved import" or module not found

**Step 3: Implement module registration**

In `src/config/mod.rs`, add the worktree module:
```rust
//! Configuration module.

mod claude_settings;
mod loader;
mod stop;
mod types;
mod worktree;

pub use claude_settings::*;
pub use loader::*;
pub use stop::*;
pub use types::*;
pub use worktree::*;
```

**Step 4: Verify pass**

Run: `cargo t worktree_config -v`

Expected: PASS (3 tests)

**Step 5: Commit**
```bash
git add src/config/worktree.rs src/config/mod.rs
git commit -m "feat(config): add WorktreeConfig for worktree isolation"
```

---

### Task 1.2: Integrate WorktreeConfig into SupervisorConfig

**Files:**
- Modify: `src/config/types.rs:39-70`
- Test: `src/config/types.rs` (inline tests)

**Step 1: Write failing test**

Add to `src/config/types.rs` in the `tests` module:
```rust
    #[test]
    fn test_supervisor_config_with_worktree() {
        let json = r#"{
            "worktree": {
                "enabled": true,
                "worktree_dir": ".worktrees",
                "auto_cleanup": true
            }
        }"#;
        let config: SupervisorConfig = serde_json::from_str(json).unwrap();
        assert!(config.worktree.enabled);
        assert!(config.worktree.auto_cleanup);
    }
```

**Step 2: Verify failure**

Run: `cargo t test_supervisor_config_with_worktree -v`

Expected: FAIL with "no field `worktree` on type `SupervisorConfig`"

**Step 3: Implement**

Modify `src/config/types.rs`:

Add import at top:
```rust
use super::WorktreeConfig;
```

Add field to `SupervisorConfig` struct (after `stop` field):
```rust
    #[serde(default)]
    pub stop: StopConfig,
    #[serde(default)]
    pub worktree: WorktreeConfig,
```

Update `Default` impl for `SupervisorConfig`:
```rust
impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            policy: PolicyLevel::Permissive,
            auto_continue: false,
            allowed_tools: ["Read", "Glob", "Grep"]
                .into_iter()
                .map(String::from)
                .collect(),
            denied_tools: HashSet::new(),
            ai_supervisor: false,
            stop: StopConfig::default(),
            worktree: WorktreeConfig::default(),
        }
    }
}
```

**Step 4: Verify pass**

Run: `cargo t test_supervisor_config_with_worktree -v`

Expected: PASS

**Step 5: Commit**
```bash
git add src/config/types.rs
git commit -m "feat(config): integrate WorktreeConfig into SupervisorConfig"
```

---

### Task 1.3: Create Worktree Module Scaffold

**Files:**
- Create: `src/worktree/mod.rs`
- Modify: `src/lib.rs:1-11`

**Step 1: Write failing test**

Create `src/worktree/mod.rs`:
```rust
//! Git worktree management for isolated Claude Code execution.
//!
//! This module provides functionality to create, manage, and clean up
//! git worktrees for running Claude Code in isolated environments.

mod error;
mod manager;
mod registry;
mod types;

pub use error::*;
pub use manager::*;
pub use registry::*;
pub use types::*;

#[cfg(test)]
mod tests {
    #[test]
    fn test_module_compiles() {
        // Placeholder to verify module structure
        assert!(true);
    }
}
```

**Step 2: Verify failure**

Run: `cargo t worktree::tests::test_module_compiles -v`

Expected: FAIL with "file not found for module `error`"

**Step 3: Implement submodule stubs**

Create `src/worktree/error.rs`:
```rust
//! Worktree error types.

use std::path::PathBuf;

/// Errors that can occur during worktree operations.
#[derive(thiserror::Error, Debug)]
pub enum WorktreeError {
    /// Git command failed.
    #[error("Git command failed: {0}")]
    GitError(String),

    /// Worktree already exists.
    #[error("Worktree already exists at {0}")]
    AlreadyExists(PathBuf),

    /// Worktree not found.
    #[error("Worktree not found: {0}")]
    NotFound(String),

    /// Worktree has uncommitted changes.
    #[error("Worktree has uncommitted changes: {0}")]
    DirtyWorktree(PathBuf),

    /// State persistence error.
    #[error("State error: {0}")]
    StateError(String),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
```

Create `src/worktree/types.rs`:
```rust
//! Worktree types.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Status of a worktree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorktreeStatus {
    /// Worktree created but not started.
    Pending,
    /// Worktree is actively being used.
    Active,
    /// Task completed successfully.
    Completed,
    /// Worktree is stale or orphaned.
    Stale,
}

/// Represents a git worktree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Worktree {
    /// Unique name for this worktree.
    pub name: String,
    /// Absolute path to the worktree directory.
    pub path: PathBuf,
    /// Branch name.
    pub branch: String,
    /// Current status.
    pub status: WorktreeStatus,
    /// Task description.
    pub task: String,
    /// When the worktree was created.
    pub created_at: DateTime<Utc>,
    /// Last activity timestamp.
    pub last_active: DateTime<Utc>,
    /// Base commit SHA when worktree was created.
    pub base_commit: String,
    /// Claude session ID for resume.
    pub session_id: Option<String>,
}
```

Create `src/worktree/manager.rs`:
```rust
//! Worktree manager.

use std::path::PathBuf;

use crate::config::WorktreeConfig;

use super::{Worktree, WorktreeError};

/// Manages git worktree lifecycle.
#[derive(Debug)]
pub struct WorktreeManager {
    /// Project root directory.
    project_root: PathBuf,
    /// Configuration.
    config: WorktreeConfig,
}

impl WorktreeManager {
    /// Create a new worktree manager.
    #[must_use]
    pub fn new(project_root: PathBuf, config: WorktreeConfig) -> Self {
        Self {
            project_root,
            config,
        }
    }

    /// Get the worktrees directory path.
    #[must_use]
    pub fn worktrees_dir(&self) -> PathBuf {
        self.project_root.join(&self.config.worktree_dir)
    }
}
```

Create `src/worktree/registry.rs`:
```rust
//! Worktree state registry.

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{Worktree, WorktreeError};

/// Persisted worktree registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeRegistry {
    /// Schema version for migrations.
    pub version: u32,
    /// Map of worktree name to worktree state.
    pub worktrees: HashMap<String, Worktree>,
    /// Last update timestamp.
    pub last_updated: DateTime<Utc>,
}

impl Default for WorktreeRegistry {
    fn default() -> Self {
        Self {
            version: 1,
            worktrees: HashMap::new(),
            last_updated: Utc::now(),
        }
    }
}

impl WorktreeRegistry {
    /// Create a new empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}
```

Add to `src/lib.rs`:
```rust
//! Claude Supervisor - Automated Claude Code with AI oversight.

pub mod ai;
pub mod cli;
pub mod commands;
pub mod config;
pub mod hooks;
pub mod ipc;
pub mod knowledge;
pub mod supervisor;
pub mod watcher;
pub mod worktree;
```

**Step 4: Verify pass**

Run: `cargo t worktree::tests::test_module_compiles -v`

Expected: PASS

Run: `cargo check`

Expected: No errors

**Step 5: Commit**
```bash
git add src/worktree/ src/lib.rs
git commit -m "feat(worktree): add module scaffold with types, error, manager, registry"
```

---

## Batch 2: ClaudeProcessBuilder Working Directory

**Goal:** Add working directory support to ClaudeProcessBuilder for running Claude in worktrees.

### Task 2.1: Add working_dir Field to ClaudeProcessBuilder

**Files:**
- Modify: `src/cli/process.rs:37-136`
- Test: `tests/cli/process_test.rs`

**Step 1: Write failing test**

Add to `tests/cli/process_test.rs`:
```rust
#[test]
fn test_builder_with_working_dir() {
    use std::path::PathBuf;
    use claude_supervisor::cli::ClaudeProcessBuilder;

    let builder = ClaudeProcessBuilder::new("test task")
        .working_dir("/tmp/worktree");

    // Verify the working_dir is set
    assert_eq!(builder.working_dir(), Some(&PathBuf::from("/tmp/worktree")));
}
```

**Step 2: Verify failure**

Run: `cargo t test_builder_with_working_dir -v`

Expected: FAIL with "no method named `working_dir`"

**Step 3: Implement**

Modify `src/cli/process.rs`:

Add to `ClaudeProcessBuilder` struct (after line 45):
```rust
/// Builder for configuring Claude Code process arguments.
#[derive(Debug, Clone, Default)]
pub struct ClaudeProcessBuilder {
    prompt: String,
    allowed_tools: Option<Vec<String>>,
    resume_session: Option<String>,
    max_turns: Option<u32>,
    append_system_prompt: Option<String>,
    system_prompt: Option<String>,
    working_dir: Option<std::path::PathBuf>,
}
```

Add builder method (after `system_prompt` method, around line 91):
```rust
    /// Set the working directory for the Claude process.
    #[must_use]
    pub fn working_dir(mut self, dir: impl Into<std::path::PathBuf>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }

    /// Get the working directory, if set.
    #[must_use]
    pub fn working_dir(&self) -> Option<&std::path::PathBuf> {
        self.working_dir.as_ref()
    }
```

Note: The getter method name conflicts with the builder method. Rename the getter:
```rust
    /// Get the working directory, if set.
    #[must_use]
    pub fn get_working_dir(&self) -> Option<&std::path::PathBuf> {
        self.working_dir.as_ref()
    }
```

Update the test to use `get_working_dir()`.

**Step 4: Verify pass**

Run: `cargo t test_builder_with_working_dir -v`

Expected: PASS

**Step 5: Commit**
```bash
git add src/cli/process.rs tests/cli/process_test.rs
git commit -m "feat(cli): add working_dir field to ClaudeProcessBuilder"
```

---

### Task 2.2: Apply working_dir in spawn_with_binary

**Files:**
- Modify: `src/cli/process.rs:159-173`
- Test: `tests/cli/process_test.rs`

**Step 1: Write failing test**

Add to `tests/cli/process_test.rs`:
```rust
#[tokio::test]
async fn test_spawn_with_working_dir() {
    use std::path::PathBuf;
    use claude_supervisor::cli::ClaudeProcessBuilder;

    // Use 'pwd' as a test binary to verify working directory
    let builder = ClaudeProcessBuilder::new("ignored")
        .working_dir("/tmp");

    // We can't easily test this without mocking, but we can verify
    // the code path compiles and the working_dir is accessible
    assert_eq!(builder.get_working_dir(), Some(&PathBuf::from("/tmp")));
}
```

**Step 2: Verify failure**

Run: `cargo t test_spawn_with_working_dir -v`

Expected: PASS (this test just verifies compilation)

**Step 3: Implement**

Modify `spawn_with_binary` in `src/cli/process.rs`:
```rust
    /// Spawn a process using a custom binary (for testing).
    ///
    /// # Errors
    ///
    /// Returns `SpawnError` if the process fails to spawn.
    pub fn spawn_with_binary(
        binary: &str,
        builder: &ClaudeProcessBuilder,
    ) -> Result<Self, SpawnError> {
        let args = builder.build_args();

        let mut cmd = Command::new(binary);
        cmd.args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Set working directory if specified
        if let Some(dir) = &builder.working_dir {
            cmd.current_dir(dir);
        }

        let child = cmd.spawn().map_err(SpawnError::from_io)?;

        Ok(Self { child })
    }
```

**Step 4: Verify pass**

Run: `cargo t -v`

Expected: All tests PASS

**Step 5: Commit**
```bash
git add src/cli/process.rs
git commit -m "feat(cli): apply working_dir in spawn_with_binary"
```

---

## Batch 3: WorktreeManager Core Operations

**Goal:** Implement create, list, and remove operations using git CLI.

### Task 3.1: Implement Worktree Creation

**Files:**
- Modify: `src/worktree/manager.rs`
- Test: `tests/worktree_integration.rs` (new file)

**Step 1: Write failing test**

Create `tests/worktree_integration.rs`:
```rust
//! Worktree integration tests.

use std::process::Command;

use claude_supervisor::config::WorktreeConfig;
use claude_supervisor::worktree::{WorktreeManager, WorktreeStatus};
use tempfile::TempDir;

fn setup_git_repo() -> TempDir {
    let dir = TempDir::new().unwrap();

    // Initialize git repo
    Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    // Configure git user for commits
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    // Create initial commit
    std::fs::write(dir.path().join("README.md"), "# Test").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    dir
}

#[tokio::test]
async fn test_create_worktree() {
    let repo = setup_git_repo();
    let config = WorktreeConfig::default();
    let manager = WorktreeManager::new(repo.path().to_path_buf(), config);

    let worktree = manager
        .create("test-feature", "Test task description")
        .await
        .unwrap();

    assert_eq!(worktree.name, "test-feature");
    assert_eq!(worktree.branch, "supervisor/test-feature");
    assert_eq!(worktree.status, WorktreeStatus::Pending);
    assert!(worktree.path.exists());
}
```

**Step 2: Verify failure**

Run: `cargo t test_create_worktree -v`

Expected: FAIL with "no method named `create`"

**Step 3: Implement**

Modify `src/worktree/manager.rs`:
```rust
//! Worktree manager.

use std::path::PathBuf;
use std::process::Stdio;

use chrono::Utc;
use tokio::process::Command;

use crate::config::WorktreeConfig;

use super::{Worktree, WorktreeError, WorktreeStatus};

/// Manages git worktree lifecycle.
#[derive(Debug)]
pub struct WorktreeManager {
    /// Project root directory.
    project_root: PathBuf,
    /// Configuration.
    config: WorktreeConfig,
}

impl WorktreeManager {
    /// Create a new worktree manager.
    #[must_use]
    pub fn new(project_root: PathBuf, config: WorktreeConfig) -> Self {
        Self {
            project_root,
            config,
        }
    }

    /// Get the worktrees directory path.
    #[must_use]
    pub fn worktrees_dir(&self) -> PathBuf {
        self.project_root.join(&self.config.worktree_dir)
    }

    /// Generate branch name from the pattern.
    fn branch_name(&self, name: &str) -> String {
        self.config.branch_pattern.replace("{name}", name)
    }

    /// Get the current HEAD commit SHA.
    async fn get_head_commit(&self) -> Result<String, WorktreeError> {
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&self.project_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::GitError(stderr.to_string()));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Create a new worktree.
    ///
    /// # Errors
    ///
    /// Returns error if the worktree cannot be created.
    pub async fn create(&self, name: &str, task: &str) -> Result<Worktree, WorktreeError> {
        let worktree_path = self.worktrees_dir().join(name);
        let branch = self.branch_name(name);

        // Check if worktree already exists
        if worktree_path.exists() {
            return Err(WorktreeError::AlreadyExists(worktree_path));
        }

        // Ensure worktrees directory exists
        tokio::fs::create_dir_all(self.worktrees_dir()).await?;

        // Get base commit before creating worktree
        let base_commit = self.get_head_commit().await?;

        // Create worktree with new branch
        let output = Command::new("git")
            .args([
                "worktree",
                "add",
                "-b",
                &branch,
                worktree_path.to_string_lossy().as_ref(),
            ])
            .current_dir(&self.project_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::GitError(stderr.to_string()));
        }

        let now = Utc::now();
        Ok(Worktree {
            name: name.to_string(),
            path: worktree_path,
            branch,
            status: WorktreeStatus::Pending,
            task: task.to_string(),
            created_at: now,
            last_active: now,
            base_commit,
            session_id: None,
        })
    }
}
```

**Step 4: Verify pass**

Run: `cargo t test_create_worktree -v`

Expected: PASS

**Step 5: Commit**
```bash
git add src/worktree/manager.rs tests/worktree_integration.rs
git commit -m "feat(worktree): implement worktree creation with git CLI"
```

---

### Task 3.2: Implement Worktree Listing

**Files:**
- Modify: `src/worktree/manager.rs`
- Test: `tests/worktree_integration.rs`

**Step 1: Write failing test**

Add to `tests/worktree_integration.rs`:
```rust
#[tokio::test]
async fn test_list_worktrees() {
    let repo = setup_git_repo();
    let config = WorktreeConfig::default();
    let manager = WorktreeManager::new(repo.path().to_path_buf(), config);

    // Create a worktree
    manager.create("feature-a", "Task A").await.unwrap();

    // List worktrees
    let worktrees = manager.list().await.unwrap();

    // Should have at least the main worktree and our new one
    assert!(worktrees.len() >= 2);

    // Find our worktree by branch
    let feature_wt = worktrees
        .iter()
        .find(|wt| wt.branch.contains("feature-a"));
    assert!(feature_wt.is_some());
}
```

**Step 2: Verify failure**

Run: `cargo t test_list_worktrees -v`

Expected: FAIL with "no method named `list`"

**Step 3: Implement**

Add to `src/worktree/manager.rs`:
```rust
/// Parsed worktree entry from git porcelain output.
#[derive(Debug, Default)]
struct GitWorktreeEntry {
    path: Option<PathBuf>,
    head: Option<String>,
    branch: Option<String>,
    bare: bool,
    detached: bool,
}

impl WorktreeManager {
    // ... existing methods ...

    /// List all git worktrees.
    ///
    /// # Errors
    ///
    /// Returns error if the git command fails.
    pub async fn list(&self) -> Result<Vec<Worktree>, WorktreeError> {
        let output = Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(&self.project_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::GitError(stderr.to_string()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let entries = self.parse_porcelain_output(&stdout);

        let now = Utc::now();
        let worktrees: Vec<Worktree> = entries
            .into_iter()
            .filter_map(|entry| {
                let path = entry.path?;
                let branch = entry.branch.unwrap_or_default();
                let name = path
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();

                Some(Worktree {
                    name,
                    path,
                    branch,
                    status: WorktreeStatus::Active,
                    task: String::new(),
                    created_at: now,
                    last_active: now,
                    base_commit: entry.head.unwrap_or_default(),
                    session_id: None,
                })
            })
            .collect();

        Ok(worktrees)
    }

    /// Parse git worktree list --porcelain output.
    fn parse_porcelain_output(&self, output: &str) -> Vec<GitWorktreeEntry> {
        let mut entries = Vec::new();
        let mut current = GitWorktreeEntry::default();

        for line in output.lines() {
            if line.is_empty() {
                if current.path.is_some() {
                    entries.push(current);
                    current = GitWorktreeEntry::default();
                }
                continue;
            }

            if let Some(path) = line.strip_prefix("worktree ") {
                current.path = Some(PathBuf::from(path));
            } else if let Some(head) = line.strip_prefix("HEAD ") {
                current.head = Some(head.to_string());
            } else if let Some(branch) = line.strip_prefix("branch ") {
                // Strip refs/heads/ prefix
                let branch_name = branch
                    .strip_prefix("refs/heads/")
                    .unwrap_or(branch);
                current.branch = Some(branch_name.to_string());
            } else if line == "bare" {
                current.bare = true;
            } else if line == "detached" {
                current.detached = true;
            }
        }

        // Don't forget the last entry
        if current.path.is_some() {
            entries.push(current);
        }

        entries
    }
}
```

**Step 4: Verify pass**

Run: `cargo t test_list_worktrees -v`

Expected: PASS

**Step 5: Commit**
```bash
git add src/worktree/manager.rs tests/worktree_integration.rs
git commit -m "feat(worktree): implement worktree listing with porcelain parsing"
```

---

### Task 3.3: Implement Worktree Removal

**Files:**
- Modify: `src/worktree/manager.rs`
- Test: `tests/worktree_integration.rs`

**Step 1: Write failing test**

Add to `tests/worktree_integration.rs`:
```rust
#[tokio::test]
async fn test_remove_worktree() {
    let repo = setup_git_repo();
    let config = WorktreeConfig::default();
    let manager = WorktreeManager::new(repo.path().to_path_buf(), config);

    // Create a worktree
    let worktree = manager.create("to-remove", "Task").await.unwrap();
    assert!(worktree.path.exists());

    // Remove it
    manager.remove("to-remove", false).await.unwrap();

    // Verify it's gone
    assert!(!worktree.path.exists());
}

#[tokio::test]
async fn test_remove_worktree_with_branch_cleanup() {
    let repo = setup_git_repo();
    let config = WorktreeConfig::default();
    let manager = WorktreeManager::new(repo.path().to_path_buf(), config);

    // Create a worktree
    let worktree = manager.create("cleanup-test", "Task").await.unwrap();

    // Remove with branch cleanup
    manager.remove("cleanup-test", true).await.unwrap();

    // Verify worktree is gone
    assert!(!worktree.path.exists());

    // Verify branch is deleted
    let output = std::process::Command::new("git")
        .args(["branch", "--list", "supervisor/cleanup-test"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&output.stdout).trim().is_empty());
}
```

**Step 2: Verify failure**

Run: `cargo t test_remove_worktree -v`

Expected: FAIL with "no method named `remove`"

**Step 3: Implement**

Add to `src/worktree/manager.rs`:
```rust
impl WorktreeManager {
    // ... existing methods ...

    /// Check if a worktree has uncommitted changes.
    async fn is_dirty(&self, path: &PathBuf) -> Result<bool, WorktreeError> {
        let output = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !output.status.success() {
            // If git status fails, assume dirty to be safe
            return Ok(true);
        }

        Ok(!output.stdout.is_empty())
    }

    /// Remove a worktree.
    ///
    /// # Arguments
    ///
    /// * `name` - The worktree name
    /// * `delete_branch` - Whether to also delete the associated branch
    ///
    /// # Errors
    ///
    /// Returns error if the worktree cannot be removed.
    pub async fn remove(&self, name: &str, delete_branch: bool) -> Result<(), WorktreeError> {
        let worktree_path = self.worktrees_dir().join(name);
        let branch = self.branch_name(name);

        if !worktree_path.exists() {
            return Err(WorktreeError::NotFound(name.to_string()));
        }

        // Check for uncommitted changes
        if self.is_dirty(&worktree_path).await? {
            return Err(WorktreeError::DirtyWorktree(worktree_path));
        }

        // Remove worktree
        let output = Command::new("git")
            .args([
                "worktree",
                "remove",
                worktree_path.to_string_lossy().as_ref(),
            ])
            .current_dir(&self.project_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::GitError(stderr.to_string()));
        }

        // Delete branch if requested
        if delete_branch {
            let output = Command::new("git")
                .args(["branch", "-D", &branch])
                .current_dir(&self.project_root)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                // Log warning but don't fail - worktree is already removed
                tracing::warn!(branch = %branch, error = %stderr, "Failed to delete branch");
            }
        }

        Ok(())
    }

    /// Force remove a worktree, even if dirty.
    ///
    /// # Errors
    ///
    /// Returns error if the worktree cannot be removed.
    pub async fn force_remove(&self, name: &str, delete_branch: bool) -> Result<(), WorktreeError> {
        let worktree_path = self.worktrees_dir().join(name);
        let branch = self.branch_name(name);

        if !worktree_path.exists() {
            // Prune stale worktree entry
            let _ = Command::new("git")
                .args(["worktree", "prune"])
                .current_dir(&self.project_root)
                .output()
                .await;
            return Ok(());
        }

        // Force remove worktree
        let output = Command::new("git")
            .args([
                "worktree",
                "remove",
                "--force",
                worktree_path.to_string_lossy().as_ref(),
            ])
            .current_dir(&self.project_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::GitError(stderr.to_string()));
        }

        // Delete branch if requested
        if delete_branch {
            let _ = Command::new("git")
                .args(["branch", "-D", &branch])
                .current_dir(&self.project_root)
                .output()
                .await;
        }

        Ok(())
    }
}
```

**Step 4: Verify pass**

Run: `cargo t test_remove_worktree -v`

Expected: PASS (both tests)

**Step 5: Commit**
```bash
git add src/worktree/manager.rs tests/worktree_integration.rs
git commit -m "feat(worktree): implement worktree removal with dirty check"
```

---

## Batch 4: WorktreeRegistry Persistence

**Goal:** Implement state persistence for worktree metadata.

### Task 4.1: Implement Registry Load/Save

**Files:**
- Modify: `src/worktree/registry.rs`
- Test: `tests/worktree_integration.rs`

**Step 1: Write failing test**

Add to `tests/worktree_integration.rs`:
```rust
use claude_supervisor::worktree::WorktreeRegistry;

#[tokio::test]
async fn test_registry_save_and_load() {
    let dir = TempDir::new().unwrap();
    let state_file = dir.path().join("state.json");

    // Create and save registry
    let mut registry = WorktreeRegistry::new();

    let worktree = claude_supervisor::worktree::Worktree {
        name: "test".to_string(),
        path: dir.path().join("test"),
        branch: "supervisor/test".to_string(),
        status: WorktreeStatus::Active,
        task: "Test task".to_string(),
        created_at: chrono::Utc::now(),
        last_active: chrono::Utc::now(),
        base_commit: "abc123".to_string(),
        session_id: None,
    };

    registry.add(worktree.clone());
    registry.save(&state_file).await.unwrap();

    // Load registry
    let loaded = WorktreeRegistry::load(&state_file).await.unwrap();
    assert_eq!(loaded.worktrees.len(), 1);
    assert!(loaded.worktrees.contains_key("test"));
    assert_eq!(loaded.worktrees["test"].branch, "supervisor/test");
}
```

**Step 2: Verify failure**

Run: `cargo t test_registry_save_and_load -v`

Expected: FAIL with "no method named `add`" or similar

**Step 3: Implement**

Modify `src/worktree/registry.rs`:
```rust
//! Worktree state registry.

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::AsyncWriteExt;

use super::{Worktree, WorktreeError, WorktreeStatus};

/// Persisted worktree registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeRegistry {
    /// Schema version for migrations.
    pub version: u32,
    /// Map of worktree name to worktree state.
    pub worktrees: HashMap<String, Worktree>,
    /// Last update timestamp.
    pub last_updated: DateTime<Utc>,
}

impl Default for WorktreeRegistry {
    fn default() -> Self {
        Self {
            version: 1,
            worktrees: HashMap::new(),
            last_updated: Utc::now(),
        }
    }
}

impl WorktreeRegistry {
    /// Create a new empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Load registry from a file.
    ///
    /// Returns a new empty registry if the file doesn't exist.
    ///
    /// # Errors
    ///
    /// Returns error if the file exists but cannot be read or parsed.
    pub async fn load(path: &Path) -> Result<Self, WorktreeError> {
        if !path.exists() {
            return Ok(Self::new());
        }

        let content = fs::read_to_string(path).await.map_err(|e| {
            WorktreeError::StateError(format!("Failed to read state file: {e}"))
        })?;

        serde_json::from_str(&content).map_err(|e| {
            WorktreeError::StateError(format!("Failed to parse state file: {e}"))
        })
    }

    /// Save registry to a file atomically.
    ///
    /// Uses write-to-temp-then-rename for atomic updates.
    ///
    /// # Errors
    ///
    /// Returns error if the file cannot be written.
    pub async fn save(&self, path: &Path) -> Result<(), WorktreeError> {
        let content = serde_json::to_string_pretty(&self).map_err(|e| {
            WorktreeError::StateError(format!("Failed to serialize state: {e}"))
        })?;

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Write to temp file first for atomic update
        let temp_path = path.with_extension("json.tmp");
        let mut file = fs::File::create(&temp_path).await?;
        file.write_all(content.as_bytes()).await?;
        file.sync_all().await?;

        // Atomic rename
        fs::rename(&temp_path, path).await?;

        Ok(())
    }

    /// Add or update a worktree in the registry.
    pub fn add(&mut self, worktree: Worktree) {
        self.worktrees.insert(worktree.name.clone(), worktree);
        self.last_updated = Utc::now();
    }

    /// Remove a worktree from the registry.
    pub fn remove(&mut self, name: &str) -> Option<Worktree> {
        let removed = self.worktrees.remove(name);
        if removed.is_some() {
            self.last_updated = Utc::now();
        }
        removed
    }

    /// Get a worktree by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Worktree> {
        self.worktrees.get(name)
    }

    /// Get a mutable reference to a worktree by name.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Worktree> {
        self.worktrees.get_mut(name)
    }

    /// Update a worktree's status.
    pub fn set_status(&mut self, name: &str, status: WorktreeStatus) {
        if let Some(wt) = self.worktrees.get_mut(name) {
            wt.status = status;
            wt.last_active = Utc::now();
            self.last_updated = Utc::now();
        }
    }

    /// Find stale worktrees (last active older than threshold).
    #[must_use]
    pub fn find_stale(&self, threshold: chrono::Duration) -> Vec<&Worktree> {
        let cutoff = Utc::now() - threshold;
        self.worktrees
            .values()
            .filter(|wt| wt.status == WorktreeStatus::Active && wt.last_active < cutoff)
            .collect()
    }
}
```

**Step 4: Verify pass**

Run: `cargo t test_registry_save_and_load -v`

Expected: PASS

**Step 5: Commit**
```bash
git add src/worktree/registry.rs tests/worktree_integration.rs
git commit -m "feat(worktree): implement WorktreeRegistry with load/save"
```

---

### Task 4.2: Integrate Registry with Manager

**Files:**
- Modify: `src/worktree/manager.rs`
- Test: `tests/worktree_integration.rs`

**Step 1: Write failing test**

Add to `tests/worktree_integration.rs`:
```rust
#[tokio::test]
async fn test_manager_with_registry() {
    let repo = setup_git_repo();
    let config = WorktreeConfig::default();
    let manager = WorktreeManager::new(repo.path().to_path_buf(), config);

    // Create worktree (should auto-register)
    manager.create_and_register("registered-wt", "Task").await.unwrap();

    // Load registry and verify
    let state_file = repo.path().join(".worktrees").join("state.json");
    let registry = WorktreeRegistry::load(&state_file).await.unwrap();

    assert!(registry.worktrees.contains_key("registered-wt"));
}
```

**Step 2: Verify failure**

Run: `cargo t test_manager_with_registry -v`

Expected: FAIL with "no method named `create_and_register`"

**Step 3: Implement**

Add to `src/worktree/manager.rs`:
```rust
impl WorktreeManager {
    // ... existing methods ...

    /// Get the path to the state file.
    #[must_use]
    pub fn state_file(&self) -> PathBuf {
        self.worktrees_dir().join("state.json")
    }

    /// Load the worktree registry.
    ///
    /// # Errors
    ///
    /// Returns error if the registry cannot be loaded.
    pub async fn load_registry(&self) -> Result<WorktreeRegistry, WorktreeError> {
        WorktreeRegistry::load(&self.state_file()).await
    }

    /// Save the worktree registry.
    ///
    /// # Errors
    ///
    /// Returns error if the registry cannot be saved.
    pub async fn save_registry(&self, registry: &WorktreeRegistry) -> Result<(), WorktreeError> {
        registry.save(&self.state_file()).await
    }

    /// Create a worktree and register it in the state file.
    ///
    /// # Errors
    ///
    /// Returns error if the worktree cannot be created or registered.
    pub async fn create_and_register(
        &self,
        name: &str,
        task: &str,
    ) -> Result<Worktree, WorktreeError> {
        // Create the worktree
        let worktree = self.create(name, task).await?;

        // Register in state file
        let mut registry = self.load_registry().await?;
        registry.add(worktree.clone());
        self.save_registry(&registry).await?;

        Ok(worktree)
    }

    /// Remove a worktree and unregister it from the state file.
    ///
    /// # Errors
    ///
    /// Returns error if the worktree cannot be removed.
    pub async fn remove_and_unregister(
        &self,
        name: &str,
        delete_branch: bool,
    ) -> Result<(), WorktreeError> {
        // Remove the worktree
        self.remove(name, delete_branch).await?;

        // Unregister from state file
        let mut registry = self.load_registry().await?;
        registry.remove(name);
        self.save_registry(&registry).await?;

        Ok(())
    }
}
```

Add import at top of `manager.rs`:
```rust
use super::{Worktree, WorktreeError, WorktreeRegistry, WorktreeStatus};
```

**Step 4: Verify pass**

Run: `cargo t test_manager_with_registry -v`

Expected: PASS

**Step 5: Commit**
```bash
git add src/worktree/manager.rs tests/worktree_integration.rs
git commit -m "feat(worktree): integrate WorktreeRegistry with WorktreeManager"
```

---

## Batch 5: CLI Integration

**Goal:** Add worktree CLI flags to the run command.

### Task 5.1: Add Worktree Flags to Run Command

**Files:**
- Modify: `src/main.rs:46-63`
- Test: Manual CLI testing

**Step 1: Write failing test**

Run: `cargo run -- run --help`

Expected: No `--worktree` flag in output

**Step 2: Implement**

Modify the `Commands::Run` variant in `src/main.rs`:
```rust
    /// Run Claude Code with supervision.
    Run {
        /// The task to execute (optional if --resume is used).
        task: Option<String>,
        /// Policy level (permissive, moderate, strict).
        #[arg(short, long, value_enum, default_value_t = PolicyArg::Permissive)]
        policy: PolicyArg,
        /// Auto-continue without user prompts.
        #[arg(long)]
        auto_continue: bool,
        /// Tools to auto-approve (comma-separated).
        #[arg(long, value_delimiter = ',')]
        allowed_tools: Option<Vec<String>>,
        /// Resume a previous session by ID.
        #[arg(long, conflicts_with = "task")]
        resume: Option<String>,
        /// Run in an isolated git worktree.
        #[arg(long)]
        worktree: bool,
        /// Custom worktree directory (default: .worktrees).
        #[arg(long)]
        worktree_dir: Option<std::path::PathBuf>,
        /// Auto-cleanup worktree after task completion.
        #[arg(long)]
        worktree_cleanup: bool,
    },
```

Update the match arm in `main()`:
```rust
        Commands::Run {
            task,
            policy,
            auto_continue,
            allowed_tools,
            resume,
            worktree,
            worktree_dir,
            worktree_cleanup,
        } => {
            // ... existing validation ...

            let mut config = SupervisorConfig {
                policy: policy.into(),
                auto_continue,
                ..Default::default()
            };

            // Wire allowed_tools to config
            if let Some(tools) = allowed_tools {
                config.allowed_tools = tools.into_iter().collect();
            }

            // Wire worktree config
            if worktree {
                config.worktree.enabled = true;
            }
            if let Some(dir) = worktree_dir {
                config.worktree.worktree_dir = dir;
            }
            if worktree_cleanup {
                config.worktree.auto_cleanup = true;
            }

            // Log based on task or resume mode
            if let Some(ref task_str) = task {
                tracing::info!(
                    task = %task_str,
                    policy = ?config.policy,
                    auto_continue = config.auto_continue,
                    allowed_tools = ?config.allowed_tools,
                    worktree = config.worktree.enabled,
                    "Starting Claude supervisor"
                );
            } else if let Some(ref session_id) = resume {
                tracing::info!(
                    session_id = %session_id,
                    policy = ?config.policy,
                    auto_continue = config.auto_continue,
                    allowed_tools = ?config.allowed_tools,
                    worktree = config.worktree.enabled,
                    "Resuming Claude supervisor session"
                );
            }

            tracing::warn!("Supervisor not yet implemented");
        }
```

**Step 3: Verify pass**

Run: `cargo run -- run --help`

Expected: Shows `--worktree`, `--worktree-dir`, and `--worktree-cleanup` flags

Run: `cargo run -- run "test" --worktree -v`

Expected: Log shows `worktree = true`

**Step 4: Commit**
```bash
git add src/main.rs
git commit -m "feat(cli): add worktree flags to run command"
```

---

### Task 5.2: Add Worktree Subcommand

**Files:**
- Modify: `src/main.rs`
- Test: Manual CLI testing

**Step 1: Write failing test**

Run: `cargo run -- worktree --help`

Expected: Error - no such subcommand

**Step 2: Implement**

Add new subcommand enum and variants in `src/main.rs`:
```rust
#[derive(Subcommand)]
enum WorktreeAction {
    /// List all worktrees.
    List,
    /// Remove a worktree.
    Remove {
        /// Name of the worktree to remove.
        name: String,
        /// Force removal even if dirty.
        #[arg(short, long)]
        force: bool,
        /// Also delete the branch.
        #[arg(long)]
        delete_branch: bool,
    },
    /// Clean up stale worktrees.
    Prune {
        /// Stale threshold in hours (default: 24).
        #[arg(long, default_value = "24")]
        hours: u64,
    },
}
```

Add to `Commands` enum:
```rust
    /// Manage git worktrees.
    Worktree {
        #[command(subcommand)]
        action: WorktreeAction,
    },
```

Add handler function:
```rust
async fn handle_worktree(action: WorktreeAction) {
    use claude_supervisor::config::WorktreeConfig;
    use claude_supervisor::worktree::WorktreeManager;

    let project_root = std::env::current_dir().unwrap_or_default();
    let config = WorktreeConfig::default();
    let manager = WorktreeManager::new(project_root, config);

    match action {
        WorktreeAction::List => {
            match manager.list().await {
                Ok(worktrees) => {
                    if worktrees.is_empty() {
                        println!("No worktrees found.");
                    } else {
                        println!("{:<20} {:<40} {}", "NAME", "PATH", "BRANCH");
                        println!("{}", "-".repeat(80));
                        for wt in worktrees {
                            println!(
                                "{:<20} {:<40} {}",
                                wt.name,
                                wt.path.display(),
                                wt.branch
                            );
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to list worktrees: {e}");
                    std::process::exit(1);
                }
            }
        }
        WorktreeAction::Remove { name, force, delete_branch } => {
            let result = if force {
                manager.force_remove(&name, delete_branch).await
            } else {
                manager.remove(&name, delete_branch).await
            };

            match result {
                Ok(()) => println!("Removed worktree: {name}"),
                Err(e) => {
                    eprintln!("Failed to remove worktree: {e}");
                    std::process::exit(1);
                }
            }
        }
        WorktreeAction::Prune { hours } => {
            let threshold = chrono::Duration::hours(hours as i64);
            match manager.load_registry().await {
                Ok(registry) => {
                    let stale = registry.find_stale(threshold);
                    if stale.is_empty() {
                        println!("No stale worktrees found.");
                    } else {
                        println!("Found {} stale worktrees:", stale.len());
                        for wt in stale {
                            println!("  - {} (last active: {})", wt.name, wt.last_active);
                        }
                        println!("\nUse 'worktree remove <name>' to clean up.");
                    }
                }
                Err(e) => {
                    eprintln!("Failed to load registry: {e}");
                    std::process::exit(1);
                }
            }
        }
    }
}
```

Add to main match:
```rust
        Commands::Worktree { action } => {
            handle_worktree(action).await;
        }
```

**Step 3: Verify pass**

Run: `cargo run -- worktree --help`

Expected: Shows list, remove, prune subcommands

Run: `cargo run -- worktree list`

Expected: Shows worktree list (may be empty)

**Step 4: Commit**
```bash
git add src/main.rs
git commit -m "feat(cli): add worktree management subcommand"
```

---

## Summary

| Batch | Tasks | Description |
|-------|-------|-------------|
| 1 | 3 | WorktreeConfig and module scaffold |
| 2 | 2 | ClaudeProcessBuilder working_dir |
| 3 | 3 | WorktreeManager core operations |
| 4 | 2 | WorktreeRegistry persistence |
| 5 | 2 | CLI integration |
| **Total** | **12** | |

## Dependencies

- `chrono` - Already in Cargo.toml
- `tempfile` - Add as dev dependency for tests

Add to `Cargo.toml` under `[dev-dependencies]`:
```toml
tempfile = "3"
```
