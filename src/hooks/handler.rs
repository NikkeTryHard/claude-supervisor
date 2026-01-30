//! Hook handler that processes Claude Code hook events.

use crate::config::StopConfig;
use crate::ipc::{EscalationRequest, EscalationResponse, IpcClient};
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
    ipc_client: Option<IpcClient>,
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
            ipc_client: None,
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
            ipc_client: None,
        }
    }

    /// Add an IPC client for escalation to supervisor.
    #[must_use]
    pub fn with_ipc_client(mut self, client: IpcClient) -> Self {
        self.ipc_client = Some(client);
        self
    }

    /// Returns whether an IPC client is configured.
    #[must_use]
    pub fn has_ipc_client(&self) -> bool {
        self.ipc_client.is_some()
    }

    /// Returns a reference to the IPC client if configured.
    #[must_use]
    pub fn ipc_client(&self) -> Option<&IpcClient> {
        self.ipc_client.as_ref()
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

    /// Attempt to escalate a tool call to the supervisor via IPC.
    ///
    /// Returns `None` if no IPC client is configured or the supervisor is not running.
    /// Returns `Some(response)` if the supervisor responded to the escalation.
    ///
    /// This method is designed to be called when a policy decision results in
    /// escalation and a supervisor is available to make the final decision.
    pub async fn try_escalate(
        &self,
        session_id: &str,
        tool_name: &str,
        tool_input: &serde_json::Value,
        reason: &str,
    ) -> Option<EscalationResponse> {
        let client = self.ipc_client.as_ref()?;

        if !client.is_supervisor_running() {
            tracing::debug!("Supervisor not running, skipping escalation");
            return None;
        }

        let request = EscalationRequest {
            session_id: session_id.to_string(),
            tool_name: tool_name.to_string(),
            tool_input: tool_input.clone(),
            reason: reason.to_string(),
        };

        tracing::debug!(
            session_id = %session_id,
            tool_name = %tool_name,
            reason = %reason,
            "Escalating to supervisor"
        );

        match client.escalate(&request).await {
            Ok(response) => {
                tracing::info!(
                    session_id = %session_id,
                    tool_name = %tool_name,
                    response = ?response,
                    "Received supervisor response"
                );
                Some(response)
            }
            Err(e) => {
                tracing::warn!(
                    session_id = %session_id,
                    tool_name = %tool_name,
                    error = %e,
                    "Failed to escalate to supervisor"
                );
                None
            }
        }
    }

    /// Attempt to escalate a Stop event to the supervisor via IPC.
    pub async fn try_escalate_stop(
        &self,
        session_id: &str,
        final_message: &str,
        task: Option<&str>,
        iteration: u32,
    ) -> Option<crate::ipc::StopEscalationResponse> {
        let client = self.ipc_client.as_ref()?;

        if !client.is_supervisor_running() {
            tracing::debug!("Supervisor not running, skipping stop escalation");
            return None;
        }

        let request = crate::ipc::StopEscalationRequest {
            session_id: session_id.to_string(),
            final_message: final_message.to_string(),
            task: task.map(String::from),
            iteration,
        };

        tracing::debug!(
            session_id = %session_id,
            iteration = iteration,
            "Escalating stop to supervisor"
        );

        match client.escalate_stop(&request).await {
            Ok(response) => {
                tracing::info!(
                    session_id = %session_id,
                    response = ?response,
                    "Received supervisor stop response"
                );
                Some(response)
            }
            Err(e) => {
                tracing::warn!(
                    session_id = %session_id,
                    error = %e,
                    "Failed to escalate stop to supervisor"
                );
                None
            }
        }
    }

    /// Handle a Stop event asynchronously with optional escalation.
    ///
    /// # Errors
    ///
    /// Returns an error if the response cannot be serialized.
    pub async fn handle_stop_async(
        &self,
        input: &super::input::HookInput,
        task: Option<&str>,
    ) -> Result<HookResult, HookError> {
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

        // Try escalation to supervisor if available
        if self.ipc_client.is_some() {
            let final_message = "";
            if let Some(escalation_response) = self
                .try_escalate_stop(&input.session_id, final_message, task, iteration)
                .await
            {
                let response = match escalation_response {
                    crate::ipc::StopEscalationResponse::Allow => StopResponse::allow(),
                    crate::ipc::StopEscalationResponse::Continue { reason } => {
                        StopResponse::block(reason)
                    }
                };
                let response_json = serde_json::to_string(&response)?;
                return Ok(HookResult {
                    response: response_json,
                    should_deny: false,
                });
            }
        }

        // Fallback: If force_continue is enabled, block the stop
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

    #[test]
    fn test_with_ipc_client() {
        let handler = create_handler(PolicyLevel::Permissive);
        assert!(!handler.has_ipc_client());
        assert!(handler.ipc_client().is_none());

        let client = IpcClient::with_path("/tmp/test.sock");
        let handler = handler.with_ipc_client(client);
        assert!(handler.has_ipc_client());
        assert!(handler.ipc_client().is_some());
    }

    #[tokio::test]
    async fn test_try_escalate_no_client() {
        let handler = create_handler(PolicyLevel::Permissive);

        let result = handler
            .try_escalate(
                "session-1",
                "Bash",
                &serde_json::json!({"command": "ls"}),
                "Test escalation",
            )
            .await;

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_try_escalate_supervisor_not_running() {
        let client = IpcClient::with_path("/nonexistent/socket.sock");
        let handler = create_handler(PolicyLevel::Permissive).with_ipc_client(client);

        let result = handler
            .try_escalate(
                "session-1",
                "Bash",
                &serde_json::json!({"command": "ls"}),
                "Test escalation",
            )
            .await;

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_try_escalate_stop_no_client() {
        let handler = create_handler(PolicyLevel::Permissive);
        let result = handler
            .try_escalate_stop("session-1", "Task done", Some("Fix bug"), 1)
            .await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_try_escalate_stop_supervisor_not_running() {
        let client = IpcClient::with_path("/nonexistent/socket.sock");
        let handler = create_handler(PolicyLevel::Permissive).with_ipc_client(client);
        let result = handler
            .try_escalate_stop("session-1", "Task done", Some("Fix bug"), 1)
            .await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_handle_stop_async_no_client_fallback() {
        let handler = create_handler(PolicyLevel::Permissive);
        let input = HookInput {
            hook_event_name: "Stop".to_string(),
            session_id: "test".to_string(),
            cwd: None,
            transcript_path: None,
            permission_mode: None,
            tool_name: None,
            tool_use_id: None,
            tool_input: None,
            tool_result: None,
            stop_hook_active: Some(false),
        };
        let result = handler.handle_stop_async(&input, None).await.unwrap();
        assert!(result.response.contains("\"decision\":\"allow\""));
    }
}
