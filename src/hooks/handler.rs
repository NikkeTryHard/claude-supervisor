//! Hook handler that processes Claude Code hook events.

use crate::config::StopConfig;
use crate::supervisor::{PolicyDecision, PolicyEngine};

use super::completion::CompletionDetector;
use super::input::HookInput;
use super::iteration::IterationTracker;
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
    stop_config: StopConfig,
    iterations: IterationTracker,
    completion: CompletionDetector,
}

impl HookHandler {
    /// Create a new hook handler with the given policy engine.
    #[must_use]
    pub fn new(policy: PolicyEngine) -> Self {
        Self {
            policy,
            stop_config: StopConfig::default(),
            iterations: IterationTracker::new(),
            completion: CompletionDetector::default(),
        }
    }

    /// Create a new hook handler with custom stop configuration.
    #[must_use]
    pub fn with_config(policy: PolicyEngine, stop_config: StopConfig) -> Self {
        let completion = CompletionDetector::new(
            stop_config.completion_phrases.clone(),
            stop_config.incomplete_phrases.clone(),
        );
        Self {
            policy,
            stop_config,
            iterations: IterationTracker::new(),
            completion,
        }
    }

    /// Get the stop configuration.
    #[must_use]
    pub fn stop_config(&self) -> &StopConfig {
        &self.stop_config
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
            PolicyDecision::AllowWithModification(updated_input) => {
                tracing::info!(tool = %tool_name, decision = "allow_modified", "Tool call approved with modified input");
                (
                    PreToolUseResponse::allow_with_modification(updated_input),
                    false,
                )
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
    fn handle_stop(&self, input: &HookInput) -> Result<HookResult, HookError> {
        // If stop_hook_active is true, allow to prevent infinite loops
        if input.stop_hook_active == Some(true) {
            tracing::debug!("Stop hook already active, allowing to prevent infinite loop");
            let response = StopResponse::allow();
            let response_json = serde_json::to_string(&response)?;
            return Ok(HookResult {
                response: response_json,
                should_deny: false,
            });
        }

        // Increment iteration count
        let iteration = self.iterations.increment(&input.session_id);
        tracing::debug!(session = %input.session_id, iteration = iteration, "Stop event iteration");

        // If we've exceeded max iterations, allow stop
        if iteration > self.stop_config.max_iterations {
            tracing::info!(
                session = %input.session_id,
                iteration = iteration,
                max = self.stop_config.max_iterations,
                "Max iterations exceeded, allowing stop"
            );
            let response = StopResponse::allow();
            let response_json = serde_json::to_string(&response)?;
            return Ok(HookResult {
                response: response_json,
                should_deny: false,
            });
        }

        // If force_continue is enabled, block the stop
        if self.stop_config.force_continue {
            tracing::info!(session = %input.session_id, "Force continue enabled, blocking stop");
            let response = StopResponse::block("Continue working on the task.");
            let response_json = serde_json::to_string(&response)?;
            return Ok(HookResult {
                response: response_json,
                should_deny: false,
            });
        }

        // Default: allow stop
        tracing::debug!(session = %input.session_id, "Stop event allowed");
        let response = StopResponse::allow();
        let response_json = serde_json::to_string(&response)?;

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

    /// Get the iteration tracker.
    #[must_use]
    pub fn iterations(&self) -> &IterationTracker {
        &self.iterations
    }

    /// Get the completion detector.
    #[must_use]
    pub fn completion(&self) -> &CompletionDetector {
        &self.completion
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

    #[test]
    fn test_handle_stop_force_continue() {
        let stop_config = StopConfig {
            force_continue: true,
            ..StopConfig::default()
        };
        let handler =
            HookHandler::with_config(PolicyEngine::new(PolicyLevel::Permissive), stop_config);
        let input = r#"{
            "hook_event_name": "Stop",
            "session_id": "test",
            "stop_hook_active": false
        }"#;

        let result = handler.handle_json(input).unwrap();
        assert!(!result.should_deny);
        assert!(result.response.contains("\"decision\":\"block\""));
        assert!(result.response.contains("Continue working on the task."));
    }

    #[test]
    fn test_handle_stop_max_iterations_exceeded() {
        let stop_config = StopConfig {
            max_iterations: 2,
            ..StopConfig::default()
        };
        let handler =
            HookHandler::with_config(PolicyEngine::new(PolicyLevel::Permissive), stop_config);

        // First two iterations should allow (force_continue is false by default)
        for _ in 0..2 {
            let input = r#"{
                "hook_event_name": "Stop",
                "session_id": "test_max",
                "stop_hook_active": false
            }"#;
            let result = handler.handle_json(input).unwrap();
            assert!(result.response.contains("\"decision\":\"allow\""));
        }

        // Third iteration exceeds max, should allow
        let input = r#"{
            "hook_event_name": "Stop",
            "session_id": "test_max",
            "stop_hook_active": false
        }"#;
        let result = handler.handle_json(input).unwrap();
        assert!(result.response.contains("\"decision\":\"allow\""));
    }

    #[test]
    fn test_handle_stop_hook_active_prevents_loop() {
        let stop_config = StopConfig {
            force_continue: true,
            ..StopConfig::default()
        };
        let handler =
            HookHandler::with_config(PolicyEngine::new(PolicyLevel::Permissive), stop_config);

        // Even with force_continue, stop_hook_active=true should allow
        let input = r#"{
            "hook_event_name": "Stop",
            "session_id": "test",
            "stop_hook_active": true
        }"#;

        let result = handler.handle_json(input).unwrap();
        assert!(result.response.contains("\"decision\":\"allow\""));
    }

    #[test]
    fn test_with_config_constructor() {
        let stop_config = StopConfig {
            max_iterations: 100,
            force_continue: true,
            completion_phrases: vec!["done".to_string()],
            incomplete_phrases: vec!["pending".to_string()],
        };
        let handler = HookHandler::with_config(PolicyEngine::new(PolicyLevel::Strict), stop_config);

        assert_eq!(handler.stop_config().max_iterations, 100);
        assert!(handler.stop_config().force_continue);
    }

    #[test]
    fn test_iteration_tracking() {
        let handler = create_handler(PolicyLevel::Permissive);

        let input = r#"{
            "hook_event_name": "Stop",
            "session_id": "track_test",
            "stop_hook_active": false
        }"#;

        handler.handle_json(input).unwrap();
        assert_eq!(handler.iterations().get("track_test"), 1);

        handler.handle_json(input).unwrap();
        assert_eq!(handler.iterations().get("track_test"), 2);
    }
}
