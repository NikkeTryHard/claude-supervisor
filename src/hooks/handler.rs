//! Hook handler that processes Claude Code hook events.

use crate::supervisor::{PolicyDecision, PolicyEngine};

use super::input::HookInput;
use super::pre_tool_use::PreToolUseResponse;
use super::stop::StopResponse;

/// Errors that can occur during hook handling.
#[derive(Debug, thiserror::Error)]
pub enum HookError {
    #[error("Failed to parse hook input: {0}")]
    ParseError(#[from] serde_json::Error),

    #[error("Unknown hook event: {0}")]
    UnknownEvent(String),

    #[error("Missing required field: {0}")]
    MissingField(String),
}

/// Result of handling a hook event.
#[derive(Debug)]
pub struct HookResult {
    /// The JSON response to send back to Claude Code.
    pub response: String,
    /// Whether the hook should deny/block (exit code 2).
    pub should_deny: bool,
}

/// Handler for Claude Code hook events.
#[derive(Debug)]
pub struct HookHandler {
    policy: PolicyEngine,
}

impl HookHandler {
    /// Create a new hook handler with the given policy engine.
    #[must_use]
    pub fn new(policy: PolicyEngine) -> Self {
        Self { policy }
    }

    /// Handle a JSON hook input and return a JSON response.
    ///
    /// # Errors
    ///
    /// Returns an error if the input cannot be parsed or is invalid.
    pub fn handle_json(&self, input: &str) -> Result<HookResult, HookError> {
        let hook_input: HookInput = serde_json::from_str(input)?;
        self.handle(&hook_input)
    }

    /// Handle a hook input and return a result.
    ///
    /// # Errors
    ///
    /// Returns an error if the hook event is unknown or required fields are missing.
    pub fn handle(&self, input: &HookInput) -> Result<HookResult, HookError> {
        match input.hook_event_name.as_str() {
            "PreToolUse" => self.handle_pre_tool_use(input),
            "Stop" => self.handle_stop(input),
            other => Err(HookError::UnknownEvent(other.to_string())),
        }
    }

    /// Handle a `PreToolUse` event.
    fn handle_pre_tool_use(&self, input: &HookInput) -> Result<HookResult, HookError> {
        let tool_name = input
            .tool_name
            .as_deref()
            .ok_or_else(|| HookError::MissingField("tool_name".to_string()))?;

        let tool_input = input
            .tool_input
            .clone()
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

        let decision = self.policy.evaluate(tool_name, &tool_input);

        let (response, should_deny) = match decision {
            PolicyDecision::Allow => {
                tracing::info!(tool = %tool_name, decision = "allow", "Tool call approved");
                (PreToolUseResponse::allow(), false)
            }
            PolicyDecision::Deny(reason) => {
                tracing::warn!(tool = %tool_name, reason = %reason, "Tool call denied");
                (PreToolUseResponse::deny(&reason), true)
            }
            PolicyDecision::Escalate(reason) => {
                tracing::info!(tool = %tool_name, reason = %reason, "Tool call escalated");
                (PreToolUseResponse::ask(&reason), false)
            }
        };

        let response_json = serde_json::to_string(&response)?;

        Ok(HookResult {
            response: response_json,
            should_deny,
        })
    }

    /// Handle a `Stop` event.
    #[allow(clippy::unused_self)]
    fn handle_stop(&self, _input: &HookInput) -> Result<HookResult, HookError> {
        // For now, always allow stop events
        // Future: Could check if there are pending tasks
        let response = StopResponse::allow();
        let response_json = serde_json::to_string(&response)?;

        tracing::debug!("Stop event allowed");

        Ok(HookResult {
            response: response_json,
            should_deny: false,
        })
    }

    /// Get the policy engine.
    #[must_use]
    pub fn policy(&self) -> &PolicyEngine {
        &self.policy
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::supervisor::PolicyLevel;

    fn create_handler(level: PolicyLevel) -> HookHandler {
        HookHandler::new(PolicyEngine::new(level))
    }

    #[test]
    fn test_handle_pre_tool_use_allow() {
        let handler = create_handler(PolicyLevel::Permissive);
        let input = r#"{
            "hook_event_name": "PreToolUse",
            "session_id": "test",
            "tool_name": "Read",
            "tool_input": {"file_path": "/tmp/test.txt"}
        }"#;

        let result = handler.handle_json(input).unwrap();
        assert!(!result.should_deny);
        assert!(result.response.contains("\"permissionDecision\":\"allow\""));
    }

    #[test]
    fn test_handle_pre_tool_use_deny_dangerous() {
        let handler = create_handler(PolicyLevel::Permissive);
        let input = r#"{
            "hook_event_name": "PreToolUse",
            "session_id": "test",
            "tool_name": "Bash",
            "tool_input": {"command": "rm -rf /"}
        }"#;

        let result = handler.handle_json(input).unwrap();
        assert!(result.should_deny);
        assert!(result.response.contains("\"permissionDecision\":\"deny\""));
    }

    #[test]
    fn test_handle_pre_tool_use_escalate_moderate() {
        let handler = create_handler(PolicyLevel::Moderate);
        let input = r#"{
            "hook_event_name": "PreToolUse",
            "session_id": "test",
            "tool_name": "UnknownTool",
            "tool_input": {}
        }"#;

        let result = handler.handle_json(input).unwrap();
        assert!(!result.should_deny);
        assert!(result.response.contains("\"permissionDecision\":\"ask\""));
    }

    #[test]
    fn test_handle_stop_event() {
        let handler = create_handler(PolicyLevel::Permissive);
        let input = r#"{
            "hook_event_name": "Stop",
            "session_id": "test",
            "stop_hook_active": true
        }"#;

        let result = handler.handle_json(input).unwrap();
        assert!(!result.should_deny);
        assert!(result.response.contains("\"decision\":\"allow\""));
    }

    #[test]
    fn test_handle_unknown_event() {
        let handler = create_handler(PolicyLevel::Permissive);
        let input = r#"{
            "hook_event_name": "UnknownEvent",
            "session_id": "test"
        }"#;

        let result = handler.handle_json(input);
        assert!(matches!(result, Err(HookError::UnknownEvent(_))));
    }

    #[test]
    fn test_handle_missing_tool_name() {
        let handler = create_handler(PolicyLevel::Permissive);
        let input = r#"{
            "hook_event_name": "PreToolUse",
            "session_id": "test"
        }"#;

        let result = handler.handle_json(input);
        assert!(matches!(result, Err(HookError::MissingField(_))));
    }
}
