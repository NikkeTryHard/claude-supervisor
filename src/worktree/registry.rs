//! Worktree registry for persistent state.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::error::WorktreeError;
use super::types::Worktree;

/// Current registry format version.
const REGISTRY_VERSION: u32 = 1;

/// Persistent registry of worktrees.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeRegistry {
    /// Format version for migrations.
    pub version: u32,

    /// Map of worktree name to worktree data.
    pub worktrees: HashMap<String, Worktree>,
}

impl Default for WorktreeRegistry {
    fn default() -> Self {
        Self {
            version: REGISTRY_VERSION,
            worktrees: HashMap::new(),
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
    /// If the file doesn't exist, returns an empty registry.
    ///
    /// # Errors
    ///
    /// Returns an error if the file exists but cannot be read or parsed.
    pub fn load(path: &PathBuf) -> Result<Self, WorktreeError> {
        if !path.exists() {
            return Ok(Self::new());
        }

        let content = fs::read_to_string(path)?;
        let registry: Self = serde_json::from_str(&content)?;
        Ok(registry)
    }

    /// Save registry to a file atomically.
    ///
    /// Writes to a temporary file first, then renames to avoid corruption.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn save(&self, path: &PathBuf) -> Result<(), WorktreeError> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Write to temp file first
        let temp_path = path.with_extension("tmp");
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&temp_path, content)?;

        // Atomic rename
        fs::rename(&temp_path, path)?;

        Ok(())
    }

    /// Add or update a worktree in the registry.
    pub fn upsert(&mut self, worktree: Worktree) {
        self.worktrees.insert(worktree.name.clone(), worktree);
    }

    /// Remove a worktree from the registry.
    pub fn remove(&mut self, name: &str) -> Option<Worktree> {
        self.worktrees.remove(name)
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

    /// List all worktrees.
    #[must_use]
    pub fn list(&self) -> Vec<&Worktree> {
        self.worktrees.values().collect()
    }

    /// Get the default registry path for a worktree directory.
    #[must_use]
    pub fn default_path(worktree_dir: &std::path::Path) -> PathBuf {
        worktree_dir.join("state.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_registry_new() {
        let registry = WorktreeRegistry::new();
        assert_eq!(registry.version, REGISTRY_VERSION);
        assert!(registry.worktrees.is_empty());
    }

    #[test]
    fn test_registry_upsert_and_get() {
        let mut registry = WorktreeRegistry::new();
        let wt = Worktree::new("test", PathBuf::from("/tmp/test"), "main");

        registry.upsert(wt.clone());

        let retrieved = registry.get("test").unwrap();
        assert_eq!(retrieved.name, "test");
        assert_eq!(retrieved.path, PathBuf::from("/tmp/test"));
    }

    #[test]
    fn test_registry_remove() {
        let mut registry = WorktreeRegistry::new();
        let wt = Worktree::new("test", PathBuf::from("/tmp/test"), "main");
        registry.upsert(wt);

        let removed = registry.remove("test");
        assert!(removed.is_some());
        assert!(registry.get("test").is_none());
    }

    #[test]
    fn test_registry_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("state.json");

        let mut registry = WorktreeRegistry::new();
        let wt = Worktree::new("test", PathBuf::from("/tmp/test"), "main");
        registry.upsert(wt);

        registry.save(&path).unwrap();
        assert!(path.exists());

        let loaded = WorktreeRegistry::load(&path).unwrap();
        assert_eq!(loaded.version, REGISTRY_VERSION);
        assert!(loaded.get("test").is_some());
    }

    #[test]
    fn test_registry_load_nonexistent() {
        let path = PathBuf::from("/nonexistent/path/state.json");
        let registry = WorktreeRegistry::load(&path).unwrap();
        assert!(registry.worktrees.is_empty());
    }

    #[test]
    fn test_registry_default_path() {
        let worktree_dir = PathBuf::from("/repo/.worktrees");
        let path = WorktreeRegistry::default_path(&worktree_dir);
        assert_eq!(path, PathBuf::from("/repo/.worktrees/state.json"));
    }
}
