//! Integration tests for worktree management.

use std::path::PathBuf;

use claude_supervisor::config::WorktreeConfig;
use claude_supervisor::worktree::{
    Worktree, WorktreeError, WorktreeManager, WorktreeRegistry, WorktreeStatus,
};
use tempfile::TempDir;

/// Create a temporary git repository for testing.
async fn create_test_repo() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path();

    // Initialize git repo
    tokio::process::Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .output()
        .await
        .unwrap();

    // Configure git user for commits
    tokio::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo_path)
        .output()
        .await
        .unwrap();

    tokio::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .output()
        .await
        .unwrap();

    // Create initial commit (required for worktrees)
    std::fs::write(repo_path.join("README.md"), "# Test Repo").unwrap();

    tokio::process::Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .output()
        .await
        .unwrap();

    tokio::process::Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .output()
        .await
        .unwrap();

    temp_dir
}

#[tokio::test]
async fn test_worktree_manager_create() {
    let temp_dir = create_test_repo().await;
    let repo_path = temp_dir.path().to_path_buf();

    let config = WorktreeConfig::default();
    let manager = WorktreeManager::new(repo_path.clone(), config).unwrap();

    let worktree = manager.create("test-session").await.unwrap();

    assert_eq!(worktree.name, "test-session");
    assert_eq!(worktree.branch, "supervisor/test-session");
    assert!(worktree.path.exists());
    assert_eq!(worktree.status, WorktreeStatus::Idle);
}

#[tokio::test]
async fn test_worktree_manager_create_already_exists() {
    let temp_dir = create_test_repo().await;
    let repo_path = temp_dir.path().to_path_buf();

    let config = WorktreeConfig::default();
    let manager = WorktreeManager::new(repo_path, config).unwrap();

    // Create first worktree
    manager.create("duplicate").await.unwrap();

    // Try to create again - should fail
    let result = manager.create("duplicate").await;
    assert!(matches!(result, Err(WorktreeError::AlreadyExists(_))));
}

#[tokio::test]
async fn test_worktree_manager_create_invalid_name() {
    let temp_dir = create_test_repo().await;
    let repo_path = temp_dir.path().to_path_buf();

    let config = WorktreeConfig::default();
    let manager = WorktreeManager::new(repo_path, config).unwrap();

    // Empty name
    let result = manager.create("").await;
    assert!(matches!(result, Err(WorktreeError::InvalidName(_))));

    // Name with slash
    let result = manager.create("path/to/worktree").await;
    assert!(matches!(result, Err(WorktreeError::InvalidName(_))));
}

#[tokio::test]
async fn test_worktree_manager_list() {
    let temp_dir = create_test_repo().await;
    let repo_path = temp_dir.path().to_path_buf();

    let config = WorktreeConfig::default();
    let manager = WorktreeManager::new(repo_path, config).unwrap();

    // Create some worktrees
    manager.create("wt-alpha").await.unwrap();
    manager.create("wt-beta").await.unwrap();

    let worktrees = manager.list().await.unwrap();

    // Should have at least our two worktrees (might also include main worktree)
    let names: Vec<_> = worktrees.iter().map(|wt| wt.name.as_str()).collect();
    assert!(names.contains(&"wt-alpha"));
    assert!(names.contains(&"wt-beta"));
}

#[tokio::test]
async fn test_worktree_manager_remove() {
    let temp_dir = create_test_repo().await;
    let repo_path = temp_dir.path().to_path_buf();

    let config = WorktreeConfig::default();
    let manager = WorktreeManager::new(repo_path, config).unwrap();

    // Create and then remove
    let worktree = manager.create("to-remove").await.unwrap();
    assert!(worktree.path.exists());

    manager.remove("to-remove", false).await.unwrap();

    // Path should no longer exist
    assert!(!worktree.path.exists());
}

#[tokio::test]
async fn test_worktree_manager_remove_not_found() {
    let temp_dir = create_test_repo().await;
    let repo_path = temp_dir.path().to_path_buf();

    let config = WorktreeConfig::default();
    let manager = WorktreeManager::new(repo_path, config).unwrap();

    let result = manager.remove("nonexistent", false).await;
    assert!(matches!(result, Err(WorktreeError::NotFound(_))));
}

#[tokio::test]
async fn test_worktree_manager_remove_dirty_fails() {
    let temp_dir = create_test_repo().await;
    let repo_path = temp_dir.path().to_path_buf();

    let config = WorktreeConfig::default();
    let manager = WorktreeManager::new(repo_path, config).unwrap();

    // Create worktree and make it dirty
    let worktree = manager.create("dirty-wt").await.unwrap();
    std::fs::write(worktree.path.join("dirty-file.txt"), "uncommitted changes").unwrap();

    // Remove without force should fail
    let result = manager.remove("dirty-wt", false).await;
    assert!(matches!(result, Err(WorktreeError::DirtyWorktree { .. })));

    // Remove with force should succeed
    manager.remove("dirty-wt", true).await.unwrap();
    assert!(!worktree.path.exists());
}

#[tokio::test]
async fn test_worktree_manager_delete_branch() {
    let temp_dir = create_test_repo().await;
    let repo_path = temp_dir.path().to_path_buf();

    let config = WorktreeConfig::default();
    let manager = WorktreeManager::new(repo_path.clone(), config).unwrap();

    // Create worktree (creates branch)
    let worktree = manager.create("branch-test").await.unwrap();
    let branch_name = worktree.branch.clone();

    // Remove worktree first
    manager.remove("branch-test", false).await.unwrap();

    // Delete branch
    manager.delete_branch(&branch_name, false).await.unwrap();

    // Verify branch is gone
    let output = tokio::process::Command::new("git")
        .args(["branch", "--list", &branch_name])
        .current_dir(&repo_path)
        .output()
        .await
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.trim().is_empty());
}

#[tokio::test]
async fn test_worktree_manager_custom_config() {
    let temp_dir = create_test_repo().await;
    let repo_path = temp_dir.path().to_path_buf();

    let config = WorktreeConfig {
        enabled: true,
        worktree_dir: PathBuf::from(".custom-worktrees"),
        auto_cleanup: true,
        branch_pattern: "agent/{name}".to_string(),
    };

    let manager = WorktreeManager::new(repo_path.clone(), config).unwrap();
    let worktree = manager.create("custom").await.unwrap();

    // Should use custom branch pattern
    assert_eq!(worktree.branch, "agent/custom");

    // Should be in custom directory
    assert!(worktree
        .path
        .starts_with(repo_path.join(".custom-worktrees")));
}

#[tokio::test]
async fn test_worktree_registry_integration() {
    let temp_dir = create_test_repo().await;
    let repo_path = temp_dir.path().to_path_buf();

    let config = WorktreeConfig::default();
    let manager = WorktreeManager::new(repo_path.clone(), config).unwrap();

    // Create worktree
    let worktree = manager.create("registry-test").await.unwrap();

    // Save to registry
    let registry_path = WorktreeRegistry::default_path(&manager.worktree_dir());
    let mut registry = WorktreeRegistry::new();
    registry.upsert(worktree.clone());
    registry.save(&registry_path).unwrap();

    // Load and verify
    let loaded = WorktreeRegistry::load(&registry_path).unwrap();
    let retrieved = loaded.get("registry-test").unwrap();
    assert_eq!(retrieved.name, "registry-test");
    assert_eq!(retrieved.branch, worktree.branch);
}

#[test]
fn test_worktree_status_transitions() {
    let mut wt = Worktree::new("test", PathBuf::from("/tmp/test"), "main");

    assert_eq!(wt.status, WorktreeStatus::Idle);
    assert!(!wt.is_active());

    wt.activate("session-123");
    assert_eq!(wt.status, WorktreeStatus::Active);
    assert!(wt.is_active());
    assert_eq!(wt.session_id, Some("session-123".to_string()));

    wt.deactivate();
    assert_eq!(wt.status, WorktreeStatus::Idle);
    assert!(!wt.is_active());
    assert!(wt.session_id.is_none());

    wt.mark_for_cleanup();
    assert_eq!(wt.status, WorktreeStatus::PendingCleanup);
}

#[test]
fn test_worktree_manager_not_git_repo() {
    let temp_dir = TempDir::new().unwrap();
    let config = WorktreeConfig::default();

    let result = WorktreeManager::new(temp_dir.path().to_path_buf(), config);
    assert!(matches!(result, Err(WorktreeError::NotGitRepo)));
}
