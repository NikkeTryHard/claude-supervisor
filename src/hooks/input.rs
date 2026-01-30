//! Hook input types for Claude Code events.

use serde::{Deserialize, Serialize};

/// Input received from Claude Code hook events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookInput {
    /// The hook event name (`PreToolUse`, `PostToolUse`, `Stop`, etc.).
    pub hook_event_name: String,

    /// The session ID for the current Claude Code session.
    pub session_id: String,

    /// Current working directory.
    #[serde(default)]
    pub cwd: Option<String>,

    /// Path to the transcript file.
    #[serde(default)]
    pub transcript_path: Option<String>,

    /// Permission mode (e.g., "default", "acceptEdits").
    #[serde(default)]
    pub permission_mode: Option<String>,

    /// Tool name (for PreToolUse/PostToolUse events).
    #[serde(default)]
    pub tool_name: Option<String>,

    /// Tool use ID (for PreToolUse/PostToolUse events).
    #[serde(default)]
    pub tool_use_id: Option<String>,

    /// Tool input parameters (for `PreToolUse` events).
    #[serde(default)]
    pub tool_input: Option<serde_json::Value>,

    /// Tool result (for `PostToolUse` events).
    #[serde(default)]
    pub tool_result: Option<serde_json::Value>,

    /// Whether the stop hook is active (for Stop events).
    #[serde(default)]
    pub stop_hook_active: Option<bool>,
}

impl HookInput {
    /// Check if this is a `PreToolUse` event.
    #[must_use]
    pub fn is_pre_tool_use(&self) -> bool {
        self.hook_event_name == "PreToolUse"
    }

    /// Check if this is a Stop event.
    #[must_use]
    pub fn is_stop(&self) -> bool {
        self.hook_event_name == "Stop"
    }

    /// Get the tool name if available.
    #[must_use]
    pub fn get_tool_name(&self) -> Option<&str> {
        self.tool_name.as_deref()
    }

    /// Get the tool input if available.
    #[must_use]
    pub fn get_tool_input(&self) -> Option<&serde_json::Value> {
        self.tool_input.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_pre_tool_use() {
        let json = r#"{
            "hook_event_name": "PreToolUse",
            "session_id": "abc123",
            "cwd": "/home/user/project",
            "tool_name": "Bash",
            "tool_use_id": "tool_001",
            "tool_input": {"command": "ls -la"}
        }"#;

        let input: HookInput = serde_json::from_str(json).unwrap();
        assert!(input.is_pre_tool_use());
        assert_eq!(input.session_id, "abc123");
        assert_eq!(input.get_tool_name(), Some("Bash"));
    }

    #[test]
    fn test_deserialize_stop_event() {
        let json = r#"{
            "hook_event_name": "Stop",
            "session_id": "abc123",
            "stop_hook_active": true
        }"#;

        let input: HookInput = serde_json::from_str(json).unwrap();
        assert!(input.is_stop());
        assert_eq!(input.stop_hook_active, Some(true));
    }

    #[test]
    fn test_deserialize_minimal() {
        let json = r#"{
            "hook_event_name": "PreToolUse",
            "session_id": "xyz"
        }"#;

        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.hook_event_name, "PreToolUse");
        assert!(input.tool_name.is_none());
    }
}
