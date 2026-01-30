//! Integration tests for hook installer.

use std::fs;

use claude_supervisor::commands::HookInstaller;
use claude_supervisor::config::ClaudeSettings;
use tempfile::TempDir;

/// Test install/uninstall roundtrip.
#[test]
fn install_uninstall_roundtrip() {
    let temp_dir = TempDir::new().unwrap();
    let settings_path = temp_dir.path().join("settings.json");

    // Start with empty settings
    fs::write(&settings_path, "{}").unwrap();

    let installer = HookInstaller::new("/usr/bin/claude-supervisor".into())
        .unwrap()
        .with_settings_path(settings_path.clone());

    // Install hooks
    let install_result = installer.install().unwrap();
    assert!(install_result.pre_tool_use_installed);
    assert!(install_result.stop_installed);
    assert!(!install_result.replaced_existing);

    // Verify hooks are in settings
    let settings = ClaudeSettings::load_from(&settings_path).unwrap();
    let hooks = settings.hooks.as_ref().unwrap();
    assert!(hooks.pre_tool_use.is_some());
    assert!(hooks.stop.is_some());

    // Uninstall hooks
    let uninstall_result = installer.uninstall().unwrap();
    assert!(uninstall_result.pre_tool_use_removed);
    assert!(uninstall_result.stop_removed);

    // Verify hooks are removed
    let settings = ClaudeSettings::load_from(&settings_path).unwrap();
    assert!(settings.hooks.is_none());
}

/// Test preserving existing settings fields during install/uninstall.
#[test]
fn preserves_existing_settings() {
    let temp_dir = TempDir::new().unwrap();
    let settings_path = temp_dir.path().join("settings.json");

    // Start with settings containing other fields
    let initial_settings = r#"{
        "apiKey": "sk-ant-xxx",
        "theme": "dark",
        "customField": {"nested": true}
    }"#;
    fs::write(&settings_path, initial_settings).unwrap();

    let installer = HookInstaller::new("/usr/bin/claude-supervisor".into())
        .unwrap()
        .with_settings_path(settings_path.clone());

    // Install hooks
    installer.install().unwrap();

    // Verify other fields are preserved
    let settings = ClaudeSettings::load_from(&settings_path).unwrap();
    assert!(settings.hooks.is_some());
    assert!(settings.other.contains_key("apiKey"));
    assert!(settings.other.contains_key("theme"));
    assert!(settings.other.contains_key("customField"));

    // Uninstall hooks
    installer.uninstall().unwrap();

    // Verify other fields are still preserved
    let settings = ClaudeSettings::load_from(&settings_path).unwrap();
    assert!(settings.hooks.is_none());
    assert!(settings.other.contains_key("apiKey"));
    assert!(settings.other.contains_key("theme"));
    assert!(settings.other.contains_key("customField"));
}

/// Test preserving existing hooks during install.
#[test]
fn preserves_existing_hooks() {
    let temp_dir = TempDir::new().unwrap();
    let settings_path = temp_dir.path().join("settings.json");

    // Start with settings containing other hooks
    let initial_settings = r#"{
        "hooks": {
            "PreToolUse": [
                {"type": "command", "command": "other-tool --check", "timeout": 3000}
            ],
            "PostToolUse": [
                {"type": "command", "command": "post-tool-hook", "timeout": 2000}
            ]
        }
    }"#;
    fs::write(&settings_path, initial_settings).unwrap();

    let installer = HookInstaller::new("/usr/bin/claude-supervisor".into())
        .unwrap()
        .with_settings_path(settings_path.clone());

    // Install hooks
    installer.install().unwrap();

    // Verify existing hooks are preserved and new ones added
    let settings = ClaudeSettings::load_from(&settings_path).unwrap();
    let hooks = settings.hooks.as_ref().unwrap();

    // PreToolUse should have both hooks
    let pre_tool_use = hooks.pre_tool_use.as_ref().unwrap();
    assert_eq!(pre_tool_use.len(), 2);
    assert!(pre_tool_use
        .iter()
        .any(|h| h.command == "other-tool --check"));
    assert!(pre_tool_use
        .iter()
        .any(|h| h.command.contains("claude-supervisor")));

    // PostToolUse should be unchanged
    let post_tool_use = hooks.post_tool_use.as_ref().unwrap();
    assert_eq!(post_tool_use.len(), 1);
    assert_eq!(post_tool_use[0].command, "post-tool-hook");

    // Uninstall hooks
    installer.uninstall().unwrap();

    // Verify only supervisor hooks are removed
    let settings = ClaudeSettings::load_from(&settings_path).unwrap();
    let hooks = settings.hooks.as_ref().unwrap();

    // PreToolUse should still have the other hook
    let pre_tool_use = hooks.pre_tool_use.as_ref().unwrap();
    assert_eq!(pre_tool_use.len(), 1);
    assert_eq!(pre_tool_use[0].command, "other-tool --check");

    // PostToolUse should be unchanged
    assert!(hooks.post_tool_use.is_some());
}

/// Test reinstalling updates existing supervisor hooks.
#[test]
fn reinstall_updates_hooks() {
    let temp_dir = TempDir::new().unwrap();
    let settings_path = temp_dir.path().join("settings.json");

    fs::write(&settings_path, "{}").unwrap();

    // First install
    let installer1 = HookInstaller::new("/old/path/claude-supervisor".into())
        .unwrap()
        .with_settings_path(settings_path.clone());
    let result1 = installer1.install().unwrap();
    assert!(!result1.replaced_existing);

    // Verify first install
    let settings = ClaudeSettings::load_from(&settings_path).unwrap();
    let hooks = settings.hooks.as_ref().unwrap();
    let pre_tool_use = hooks.pre_tool_use.as_ref().unwrap();
    assert!(pre_tool_use[0].command.contains("/old/path/"));

    // Second install with different path
    let installer2 = HookInstaller::new("/new/path/claude-supervisor".into())
        .unwrap()
        .with_settings_path(settings_path.clone());
    let result2 = installer2.install().unwrap();
    assert!(result2.replaced_existing);

    // Verify second install replaced the hook
    let settings = ClaudeSettings::load_from(&settings_path).unwrap();
    let hooks = settings.hooks.as_ref().unwrap();
    let pre_tool_use = hooks.pre_tool_use.as_ref().unwrap();
    assert_eq!(pre_tool_use.len(), 1);
    assert!(pre_tool_use[0].command.contains("/new/path/"));
}

/// Test installing to nonexistent file creates it.
#[test]
fn install_creates_settings_file() {
    let temp_dir = TempDir::new().unwrap();
    let settings_path = temp_dir.path().join("new_dir").join("settings.json");

    assert!(!settings_path.exists());

    let installer = HookInstaller::new("/usr/bin/claude-supervisor".into())
        .unwrap()
        .with_settings_path(settings_path.clone());

    installer.install().unwrap();

    assert!(settings_path.exists());

    let settings = ClaudeSettings::load_from(&settings_path).unwrap();
    assert!(settings.hooks.is_some());
}

/// Test custom timeout is applied to hooks.
#[test]
fn custom_timeout_applied() {
    let temp_dir = TempDir::new().unwrap();
    let settings_path = temp_dir.path().join("settings.json");

    fs::write(&settings_path, "{}").unwrap();

    let installer = HookInstaller::new("/usr/bin/claude-supervisor".into())
        .unwrap()
        .with_settings_path(settings_path.clone())
        .with_timeout(10000);

    installer.install().unwrap();

    let settings = ClaudeSettings::load_from(&settings_path).unwrap();
    let hooks = settings.hooks.as_ref().unwrap();
    let pre_tool_use = hooks.pre_tool_use.as_ref().unwrap();
    assert_eq!(pre_tool_use[0].timeout, Some(10000));
}

/// Test uninstalling when no hooks exist is idempotent.
#[test]
fn uninstall_idempotent() {
    let temp_dir = TempDir::new().unwrap();
    let settings_path = temp_dir.path().join("settings.json");

    fs::write(&settings_path, "{}").unwrap();

    let installer = HookInstaller::new("/usr/bin/claude-supervisor".into())
        .unwrap()
        .with_settings_path(settings_path.clone());

    // Uninstall when no hooks exist
    let result = installer.uninstall().unwrap();
    assert!(!result.pre_tool_use_removed);
    assert!(!result.stop_removed);

    // Settings should still be valid
    let settings = ClaudeSettings::load_from(&settings_path).unwrap();
    assert!(settings.hooks.is_none());
}
