//! Claude Code settings.json types.
//!
//! This module provides types for reading and writing Claude Code's
//! settings.json file, specifically for managing hook configurations.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Claude Code settings from ~/.claude/settings.json.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClaudeSettings {
    /// Hook configuration section.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<HooksConfig>,
    /// Other fields we preserve but don't interpret.
    #[serde(flatten)]
    pub other: HashMap<String, serde_json::Value>,
}

/// Hook configuration section.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HooksConfig {
    /// `PreToolUse` hooks.
    #[serde(rename = "PreToolUse", skip_serializing_if = "Option::is_none")]
    pub pre_tool_use: Option<Vec<HookEntry>>,
    /// `PostToolUse` hooks.
    #[serde(rename = "PostToolUse", skip_serializing_if = "Option::is_none")]
    pub post_tool_use: Option<Vec<HookEntry>>,
    /// Stop hooks.
    #[serde(rename = "Stop", skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<HookEntry>>,
    /// Other hook types we preserve but don't interpret.
    #[serde(flatten)]
    pub other: HashMap<String, serde_json::Value>,
}

/// A single hook entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HookEntry {
    /// Hook type (always "command" for our hooks).
    #[serde(rename = "type")]
    pub hook_type: String,
    /// Command to execute.
    pub command: String,
    /// Timeout in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,
}

impl HookEntry {
    /// Creates a new command hook entry.
    #[must_use]
    pub fn command(cmd: impl Into<String>, timeout: u32) -> Self {
        Self {
            hook_type: "command".to_string(),
            command: cmd.into(),
            timeout: Some(timeout),
        }
    }

    /// Checks if this hook entry was created by claude-supervisor.
    #[must_use]
    pub fn is_supervisor_hook(&self) -> bool {
        self.command.contains("claude-supervisor")
    }
}

impl ClaudeSettings {
    /// Returns the default path for Claude settings.json.
    #[must_use]
    pub fn default_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".claude").join("settings.json"))
    }

    /// Loads settings from the given path.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    pub fn load_from(path: &PathBuf) -> Result<Self, SettingsError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path).map_err(|e| SettingsError::ReadError {
            path: path.clone(),
            source: e,
        })?;
        serde_json::from_str(&content).map_err(|e| SettingsError::ParseError {
            path: path.clone(),
            source: e,
        })
    }

    /// Saves settings to the given path.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn save_to(&self, path: &PathBuf) -> Result<(), SettingsError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| SettingsError::WriteError {
                path: path.clone(),
                source: e,
            })?;
        }
        let content = serde_json::to_string_pretty(self).map_err(SettingsError::SerializeError)?;
        std::fs::write(path, content).map_err(|e| SettingsError::WriteError {
            path: path.clone(),
            source: e,
        })
    }
}

/// Errors that can occur when working with Claude settings.
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    /// Could not determine home directory.
    #[error("Could not determine home directory")]
    NoHomeDir,
    /// Failed to read settings file.
    #[error("Failed to read settings from {path}: {source}")]
    ReadError {
        /// Path to the settings file.
        path: PathBuf,
        /// Underlying IO error.
        source: std::io::Error,
    },
    /// Failed to parse settings file.
    #[error("Failed to parse settings from {path}: {source}")]
    ParseError {
        /// Path to the settings file.
        path: PathBuf,
        /// Underlying JSON error.
        source: serde_json::Error,
    },
    /// Failed to write settings file.
    #[error("Failed to write settings to {path}: {source}")]
    WriteError {
        /// Path to the settings file.
        path: PathBuf,
        /// Underlying IO error.
        source: std::io::Error,
    },
    /// Failed to serialize settings.
    #[error("Failed to serialize settings: {0}")]
    SerializeError(serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_empty_settings() {
        let json = "{}";
        let settings: ClaudeSettings = serde_json::from_str(json).unwrap();
        assert!(settings.hooks.is_none());
        assert!(settings.other.is_empty());
    }

    #[test]
    fn parse_settings_with_hooks() {
        let json = r#"{
            "hooks": {
                "PreToolUse": [
                    {"type": "command", "command": "echo test", "timeout": 5000}
                ],
                "Stop": [
                    {"type": "command", "command": "echo stop"}
                ]
            }
        }"#;
        let settings: ClaudeSettings = serde_json::from_str(json).unwrap();
        let hooks = settings.hooks.unwrap();
        assert_eq!(hooks.pre_tool_use.as_ref().unwrap().len(), 1);
        assert_eq!(hooks.stop.as_ref().unwrap().len(), 1);
        assert!(hooks.post_tool_use.is_none());
    }

    #[test]
    fn preserve_other_fields() {
        let json = r#"{
            "someOtherField": "value",
            "nested": {"key": 123},
            "hooks": {
                "PreToolUse": [],
                "CustomHook": [{"type": "command", "command": "custom"}]
            }
        }"#;
        let settings: ClaudeSettings = serde_json::from_str(json).unwrap();

        // Check that other fields are preserved
        assert_eq!(settings.other.get("someOtherField"), Some(&json!("value")));
        assert_eq!(settings.other.get("nested"), Some(&json!({"key": 123})));

        // Check that unknown hook types are preserved
        let hooks = settings.hooks.unwrap();
        assert!(hooks.other.contains_key("CustomHook"));
    }

    #[test]
    fn roundtrip_serialization() {
        let original = ClaudeSettings {
            hooks: Some(HooksConfig {
                pre_tool_use: Some(vec![HookEntry::command("test", 5000)]),
                post_tool_use: None,
                stop: Some(vec![HookEntry::command("stop", 3000)]),
                other: HashMap::new(),
            }),
            other: {
                let mut map = HashMap::new();
                map.insert("customField".to_string(), json!("customValue"));
                map
            },
        };

        let json = serde_json::to_string(&original).unwrap();
        let parsed: ClaudeSettings = serde_json::from_str(&json).unwrap();

        assert_eq!(
            parsed.hooks.as_ref().unwrap().pre_tool_use,
            original.hooks.as_ref().unwrap().pre_tool_use
        );
        assert_eq!(
            parsed.hooks.as_ref().unwrap().stop,
            original.hooks.as_ref().unwrap().stop
        );
        assert_eq!(parsed.other.get("customField"), Some(&json!("customValue")));
    }

    #[test]
    fn hook_entry_command_constructor() {
        let entry = HookEntry::command("my-command arg1", 5000);
        assert_eq!(entry.hook_type, "command");
        assert_eq!(entry.command, "my-command arg1");
        assert_eq!(entry.timeout, Some(5000));
    }

    #[test]
    fn hook_entry_is_supervisor_hook() {
        let supervisor = HookEntry::command("claude-supervisor hook pre-tool-use", 5000);
        assert!(supervisor.is_supervisor_hook());

        let other = HookEntry::command("some-other-command", 5000);
        assert!(!other.is_supervisor_hook());
    }

    #[test]
    fn load_from_nonexistent_returns_default() {
        let path = PathBuf::from("/nonexistent/path/settings.json");
        let settings = ClaudeSettings::load_from(&path).unwrap();
        assert!(settings.hooks.is_none());
        assert!(settings.other.is_empty());
    }

    #[test]
    fn default_path_returns_home_claude_settings() {
        if let Some(path) = ClaudeSettings::default_path() {
            assert!(path.ends_with(".claude/settings.json"));
        }
        // If home_dir returns None, default_path returns None - that's OK
    }
}
