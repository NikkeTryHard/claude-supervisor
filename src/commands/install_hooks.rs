//! Hook installer command.
//!
//! This module provides functionality to install and uninstall claude-supervisor
//! hooks into Claude Code's settings.json.

use std::path::PathBuf;

use crate::config::{ClaudeSettings, HookEntry, HooksConfig, SettingsError};

/// Default timeout for hooks in milliseconds.
const DEFAULT_HOOK_TIMEOUT: u32 = 5000;

/// Result of a hook installation operation.
#[derive(Debug)]
pub struct InstallResult {
    /// Path to the settings file that was modified.
    pub settings_path: PathBuf,
    /// Whether `PreToolUse` hook was installed.
    pub pre_tool_use_installed: bool,
    /// Whether Stop hook was installed.
    pub stop_installed: bool,
    /// Whether any existing hooks were replaced.
    pub replaced_existing: bool,
}

/// Result of a hook uninstallation operation.
#[derive(Debug)]
pub struct UninstallResult {
    /// Path to the settings file that was modified.
    pub settings_path: PathBuf,
    /// Whether `PreToolUse` hook was removed.
    pub pre_tool_use_removed: bool,
    /// Whether Stop hook was removed.
    pub stop_removed: bool,
}

/// Errors that can occur during hook installation.
#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    /// Could not determine home directory.
    #[error("Could not determine home directory")]
    NoHomeDir,
    /// Settings file error.
    #[error("Settings error: {0}")]
    SettingsError(#[from] SettingsError),
    /// Could not determine current executable path.
    #[error("Could not determine current executable path: {0}")]
    CurrentExeError(std::io::Error),
}

/// Installs claude-supervisor hooks into Claude Code settings.
#[derive(Debug)]
pub struct HookInstaller {
    /// Path to the claude-supervisor binary.
    binary_path: PathBuf,
    /// Path to Claude settings.json.
    settings_path: PathBuf,
    /// Timeout for hooks in milliseconds.
    timeout: u32,
}

impl HookInstaller {
    /// Creates a new hook installer with the given binary path.
    ///
    /// # Errors
    ///
    /// Returns an error if the home directory cannot be determined.
    pub fn new(binary_path: PathBuf) -> Result<Self, InstallError> {
        let settings_path = ClaudeSettings::default_path().ok_or(InstallError::NoHomeDir)?;
        Ok(Self {
            binary_path,
            settings_path,
            timeout: DEFAULT_HOOK_TIMEOUT,
        })
    }

    /// Creates a new hook installer using the current executable path.
    ///
    /// # Errors
    ///
    /// Returns an error if the current executable path or home directory
    /// cannot be determined.
    pub fn from_current_exe() -> Result<Self, InstallError> {
        let binary_path = std::env::current_exe().map_err(InstallError::CurrentExeError)?;
        Self::new(binary_path)
    }

    /// Sets a custom settings path (useful for testing).
    #[must_use]
    pub fn with_settings_path(mut self, path: PathBuf) -> Self {
        self.settings_path = path;
        self
    }

    /// Sets a custom timeout for hooks.
    #[must_use]
    pub fn with_timeout(mut self, timeout: u32) -> Self {
        self.timeout = timeout;
        self
    }

    /// Returns the binary path.
    #[must_use]
    pub fn binary_path(&self) -> &PathBuf {
        &self.binary_path
    }

    /// Returns the settings path.
    #[must_use]
    pub fn settings_path(&self) -> &PathBuf {
        &self.settings_path
    }

    /// Generates the hook command for a given event type.
    #[must_use]
    pub fn generate_hook_command(&self, event: &str) -> String {
        format!("{} hook {}", self.binary_path.display(), event)
    }

    /// Installs `PreToolUse` and Stop hooks into Claude settings.
    ///
    /// # Errors
    ///
    /// Returns an error if settings cannot be read or written.
    pub fn install(&self) -> Result<InstallResult, InstallError> {
        let mut settings = ClaudeSettings::load_from(&self.settings_path)?;
        let mut replaced_existing = false;

        // Initialize hooks config if not present
        let hooks = settings.hooks.get_or_insert_with(HooksConfig::default);

        // Install PreToolUse hook
        let pre_tool_use_cmd = self.generate_hook_command("pre-tool-use");
        let pre_tool_use_entry = HookEntry::command(&pre_tool_use_cmd, self.timeout);

        let pre_tool_use_installed = Self::install_hook_entry(
            &mut hooks.pre_tool_use,
            pre_tool_use_entry,
            &mut replaced_existing,
        );

        // Install Stop hook
        let stop_cmd = self.generate_hook_command("stop");
        let stop_entry = HookEntry::command(&stop_cmd, self.timeout);

        let stop_installed =
            Self::install_hook_entry(&mut hooks.stop, stop_entry, &mut replaced_existing);

        // Save settings
        settings.save_to(&self.settings_path)?;

        Ok(InstallResult {
            settings_path: self.settings_path.clone(),
            pre_tool_use_installed,
            stop_installed,
            replaced_existing,
        })
    }

    /// Installs a hook entry, replacing any existing supervisor hook.
    fn install_hook_entry(
        hooks: &mut Option<Vec<HookEntry>>,
        entry: HookEntry,
        replaced: &mut bool,
    ) -> bool {
        let hook_list = hooks.get_or_insert_with(Vec::new);

        // Check if we already have a supervisor hook
        if let Some(idx) = hook_list.iter().position(HookEntry::is_supervisor_hook) {
            if hook_list[idx] != entry {
                hook_list[idx] = entry;
                *replaced = true;
            }
            true
        } else {
            // Add new hook
            hook_list.push(entry);
            true
        }
    }

    /// Uninstalls claude-supervisor hooks from Claude settings.
    ///
    /// # Errors
    ///
    /// Returns an error if settings cannot be read or written.
    pub fn uninstall(&self) -> Result<UninstallResult, InstallError> {
        let mut settings = ClaudeSettings::load_from(&self.settings_path)?;

        let mut pre_tool_use_removed = false;
        let mut stop_removed = false;

        if let Some(ref mut hooks) = settings.hooks {
            // Remove PreToolUse supervisor hooks
            if let Some(ref mut list) = hooks.pre_tool_use {
                let before_len = list.len();
                list.retain(|h| !h.is_supervisor_hook());
                pre_tool_use_removed = list.len() < before_len;

                // Clean up empty list
                if list.is_empty() {
                    hooks.pre_tool_use = None;
                }
            }

            // Remove Stop supervisor hooks
            if let Some(ref mut list) = hooks.stop {
                let before_len = list.len();
                list.retain(|h| !h.is_supervisor_hook());
                stop_removed = list.len() < before_len;

                // Clean up empty list
                if list.is_empty() {
                    hooks.stop = None;
                }
            }

            // Clean up empty hooks config
            if hooks.pre_tool_use.is_none()
                && hooks.post_tool_use.is_none()
                && hooks.stop.is_none()
                && hooks.other.is_empty()
            {
                settings.hooks = None;
            }
        }

        // Save settings
        settings.save_to(&self.settings_path)?;

        Ok(UninstallResult {
            settings_path: self.settings_path.clone(),
            pre_tool_use_removed,
            stop_removed,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_temp_settings(content: &str) -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let settings_path = temp_dir.path().join("settings.json");
        fs::write(&settings_path, content).unwrap();
        (temp_dir, settings_path)
    }

    #[test]
    fn generate_hook_command_pre_tool_use() {
        let installer = HookInstaller::new(PathBuf::from("/usr/bin/claude-supervisor"))
            .unwrap()
            .with_settings_path(PathBuf::from("/tmp/test.json"));

        let cmd = installer.generate_hook_command("pre-tool-use");
        assert_eq!(cmd, "/usr/bin/claude-supervisor hook pre-tool-use");
    }

    #[test]
    fn generate_hook_command_stop() {
        let installer = HookInstaller::new(PathBuf::from("/usr/bin/claude-supervisor"))
            .unwrap()
            .with_settings_path(PathBuf::from("/tmp/test.json"));

        let cmd = installer.generate_hook_command("stop");
        assert_eq!(cmd, "/usr/bin/claude-supervisor hook stop");
    }

    #[test]
    fn install_to_empty_settings() {
        let (_temp_dir, settings_path) = create_temp_settings("{}");

        let installer = HookInstaller::new(PathBuf::from("/usr/bin/claude-supervisor"))
            .unwrap()
            .with_settings_path(settings_path.clone());

        let result = installer.install().unwrap();

        assert!(result.pre_tool_use_installed);
        assert!(result.stop_installed);
        assert!(!result.replaced_existing);

        // Verify settings were written
        let settings = ClaudeSettings::load_from(&settings_path).unwrap();
        let hooks = settings.hooks.unwrap();
        assert_eq!(hooks.pre_tool_use.as_ref().unwrap().len(), 1);
        assert_eq!(hooks.stop.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn install_preserves_existing_hooks() {
        let (_temp_dir, settings_path) = create_temp_settings(
            r#"{
            "hooks": {
                "PreToolUse": [
                    {"type": "command", "command": "other-tool", "timeout": 3000}
                ]
            }
        }"#,
        );

        let installer = HookInstaller::new(PathBuf::from("/usr/bin/claude-supervisor"))
            .unwrap()
            .with_settings_path(settings_path.clone());

        installer.install().unwrap();

        let settings = ClaudeSettings::load_from(&settings_path).unwrap();
        let hooks = settings.hooks.unwrap();
        let pre_tool_use = hooks.pre_tool_use.unwrap();

        // Should have both the existing hook and the new one
        assert_eq!(pre_tool_use.len(), 2);
        assert!(pre_tool_use.iter().any(|h| h.command == "other-tool"));
        assert!(pre_tool_use.iter().any(HookEntry::is_supervisor_hook));
    }

    #[test]
    fn install_replaces_existing_supervisor_hook() {
        let (_temp_dir, settings_path) = create_temp_settings(
            r#"{
            "hooks": {
                "PreToolUse": [
                    {"type": "command", "command": "claude-supervisor hook pre-tool-use", "timeout": 3000}
                ]
            }
        }"#,
        );

        let installer = HookInstaller::new(PathBuf::from("/new/path/claude-supervisor"))
            .unwrap()
            .with_settings_path(settings_path.clone());

        let result = installer.install().unwrap();

        assert!(result.replaced_existing);

        let settings = ClaudeSettings::load_from(&settings_path).unwrap();
        let hooks = settings.hooks.unwrap();
        let pre_tool_use = hooks.pre_tool_use.unwrap();

        // Should only have the new hook
        assert_eq!(pre_tool_use.len(), 1);
        assert!(pre_tool_use[0]
            .command
            .contains("/new/path/claude-supervisor"));
    }

    #[test]
    fn uninstall_removes_supervisor_hooks() {
        let (_temp_dir, settings_path) = create_temp_settings(
            r#"{
            "hooks": {
                "PreToolUse": [
                    {"type": "command", "command": "other-tool", "timeout": 3000},
                    {"type": "command", "command": "claude-supervisor hook pre-tool-use", "timeout": 5000}
                ],
                "Stop": [
                    {"type": "command", "command": "claude-supervisor hook stop", "timeout": 5000}
                ]
            }
        }"#,
        );

        let installer = HookInstaller::new(PathBuf::from("/usr/bin/claude-supervisor"))
            .unwrap()
            .with_settings_path(settings_path.clone());

        let result = installer.uninstall().unwrap();

        assert!(result.pre_tool_use_removed);
        assert!(result.stop_removed);

        let settings = ClaudeSettings::load_from(&settings_path).unwrap();
        let hooks = settings.hooks.unwrap();

        // PreToolUse should still have the other hook
        let pre_tool_use = hooks.pre_tool_use.unwrap();
        assert_eq!(pre_tool_use.len(), 1);
        assert_eq!(pre_tool_use[0].command, "other-tool");

        // Stop should be removed entirely
        assert!(hooks.stop.is_none());
    }

    #[test]
    fn uninstall_cleans_up_empty_hooks() {
        let (_temp_dir, settings_path) = create_temp_settings(
            r#"{
            "hooks": {
                "PreToolUse": [
                    {"type": "command", "command": "claude-supervisor hook pre-tool-use", "timeout": 5000}
                ]
            }
        }"#,
        );

        let installer = HookInstaller::new(PathBuf::from("/usr/bin/claude-supervisor"))
            .unwrap()
            .with_settings_path(settings_path.clone());

        installer.uninstall().unwrap();

        let settings = ClaudeSettings::load_from(&settings_path).unwrap();
        // hooks should be None since all hooks were removed
        assert!(settings.hooks.is_none());
    }

    #[test]
    fn install_preserves_other_settings_fields() {
        let (_temp_dir, settings_path) = create_temp_settings(
            r#"{
            "someOtherField": "value",
            "nested": {"key": 123}
        }"#,
        );

        let installer = HookInstaller::new(PathBuf::from("/usr/bin/claude-supervisor"))
            .unwrap()
            .with_settings_path(settings_path.clone());

        installer.install().unwrap();

        let settings = ClaudeSettings::load_from(&settings_path).unwrap();
        assert!(settings.hooks.is_some());
        assert_eq!(
            settings.other.get("someOtherField"),
            Some(&serde_json::json!("value"))
        );
        assert_eq!(
            settings.other.get("nested"),
            Some(&serde_json::json!({"key": 123}))
        );
    }

    #[test]
    fn install_to_nonexistent_file() {
        let temp_dir = TempDir::new().unwrap();
        let settings_path = temp_dir.path().join("settings.json");

        let installer = HookInstaller::new(PathBuf::from("/usr/bin/claude-supervisor"))
            .unwrap()
            .with_settings_path(settings_path.clone());

        let result = installer.install().unwrap();

        assert!(result.pre_tool_use_installed);
        assert!(result.stop_installed);
        assert!(settings_path.exists());
    }

    #[test]
    fn with_timeout_sets_custom_timeout() {
        let (_temp_dir, settings_path) = create_temp_settings("{}");

        let installer = HookInstaller::new(PathBuf::from("/usr/bin/claude-supervisor"))
            .unwrap()
            .with_settings_path(settings_path.clone())
            .with_timeout(10000);

        installer.install().unwrap();

        let settings = ClaudeSettings::load_from(&settings_path).unwrap();
        let hooks = settings.hooks.unwrap();
        let pre_tool_use = hooks.pre_tool_use.unwrap();
        assert_eq!(pre_tool_use[0].timeout, Some(10000));
    }
}
