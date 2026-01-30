//! Supervisor runner for orchestrating Claude Code execution.
//!
//! This module provides the main orchestration layer that connects the
//! process spawner, stream parser, and policy engine together.

use std::time::Duration;

use tokio::sync::mpsc::Receiver;

use crate::ai::{AiClient, AiError, SupervisorDecision};
use crate::cli::{
    ClaudeEvent, ClaudeProcess, ResultEvent, StreamParser, ToolUse, DEFAULT_CHANNEL_BUFFER,
};
use crate::supervisor::{
    PolicyDecision, PolicyEngine, SessionState, SessionStateMachine, SessionStats,
};

/// Default timeout for graceful process termination.
pub const DEFAULT_TERMINATE_TIMEOUT: Duration = Duration::from_secs(5);

/// Error type for supervisor operations.
#[derive(thiserror::Error, Debug)]
pub enum SupervisorError {
    /// Process stdout was not available.
    #[error("Process stdout not available")]
    NoStdout,
    /// Failed to terminate the process.
    #[error("Failed to terminate process: {0}")]
    TerminateError(#[from] std::io::Error),
    /// Event channel closed unexpectedly.
    #[error("Event channel closed unexpectedly")]
    ChannelClosed,
}

/// Result of a supervised session.
#[derive(Debug, Clone)]
pub enum SupervisorResult {
    /// Session completed normally.
    Completed {
        /// Session identifier.
        session_id: Option<String>,
        /// Total cost in USD.
        cost_usd: Option<f64>,
    },
    /// Session was killed by the supervisor.
    Killed {
        /// Reason for killing.
        reason: String,
    },
    /// Process exited (channel closed).
    ProcessExited,
}

impl SupervisorResult {
    /// Create a Completed result from a `ResultEvent`.
    #[must_use]
    pub fn from_result_event(event: &ResultEvent) -> Self {
        Self::Completed {
            session_id: Some(event.session_id.clone()),
            cost_usd: event.cost_usd,
        }
    }
}

/// Supervisor for orchestrating Claude Code execution with policy enforcement.
pub struct Supervisor {
    process: Option<ClaudeProcess>,
    policy: PolicyEngine,
    event_rx: Receiver<ClaudeEvent>,
    state: SessionStateMachine,
    session_id: Option<String>,
    ai_client: Option<AiClient>,
}

impl Supervisor {
    /// Create a new supervisor with just a policy and event receiver.
    ///
    /// Use this when you want to manage the process separately.
    #[must_use]
    pub fn new(policy: PolicyEngine, event_rx: Receiver<ClaudeEvent>) -> Self {
        Self {
            process: None,
            policy,
            event_rx,
            state: SessionStateMachine::new(),
            session_id: None,
            ai_client: None,
        }
    }

    /// Create a new supervisor with an AI client for escalation handling.
    #[must_use]
    pub fn with_ai_client(
        policy: PolicyEngine,
        event_rx: Receiver<ClaudeEvent>,
        ai_client: AiClient,
    ) -> Self {
        Self {
            process: None,
            policy,
            event_rx,
            state: SessionStateMachine::new(),
            session_id: None,
            ai_client: Some(ai_client),
        }
    }

    /// Create a supervisor with an attached process.
    #[must_use]
    pub fn with_process(
        process: ClaudeProcess,
        policy: PolicyEngine,
        event_rx: Receiver<ClaudeEvent>,
    ) -> Self {
        Self {
            process: Some(process),
            policy,
            event_rx,
            state: SessionStateMachine::new(),
            session_id: None,
            ai_client: None,
        }
    }

    /// Create a supervisor with an attached process and AI client.
    #[must_use]
    pub fn with_process_and_ai(
        process: ClaudeProcess,
        policy: PolicyEngine,
        event_rx: Receiver<ClaudeEvent>,
        ai_client: AiClient,
    ) -> Self {
        Self {
            process: Some(process),
            policy,
            event_rx,
            state: SessionStateMachine::new(),
            session_id: None,
            ai_client: Some(ai_client),
        }
    }

    /// Create a supervisor from a process, extracting stdout and setting up the event channel.
    ///
    /// This is a convenience constructor that:
    /// 1. Takes ownership of the process's stdout
    /// 2. Creates a stream parser channel from it
    /// 3. Returns a fully configured supervisor
    ///
    /// # Errors
    ///
    /// Returns `SupervisorError::NoStdout` if the process stdout is not available.
    pub fn from_process(
        mut process: ClaudeProcess,
        policy: PolicyEngine,
    ) -> Result<Self, SupervisorError> {
        let stdout = process.take_stdout().ok_or(SupervisorError::NoStdout)?;
        let event_rx = StreamParser::into_channel(stdout, DEFAULT_CHANNEL_BUFFER);

        Ok(Self {
            process: Some(process),
            policy,
            event_rx,
            state: SessionStateMachine::new(),
            session_id: None,
            ai_client: None,
        })
    }

    /// Create a supervisor from a process with an AI client.
    ///
    /// # Errors
    ///
    /// Returns `SupervisorError::NoStdout` if the process stdout is not available.
    pub fn from_process_with_ai(
        mut process: ClaudeProcess,
        policy: PolicyEngine,
        ai_client: AiClient,
    ) -> Result<Self, SupervisorError> {
        let stdout = process.take_stdout().ok_or(SupervisorError::NoStdout)?;
        let event_rx = StreamParser::into_channel(stdout, DEFAULT_CHANNEL_BUFFER);

        Ok(Self {
            process: Some(process),
            policy,
            event_rx,
            state: SessionStateMachine::new(),
            session_id: None,
            ai_client: Some(ai_client),
        })
    }

    /// Check if AI supervision is available.
    #[must_use]
    pub fn has_ai_supervisor(&self) -> bool {
        self.ai_client.is_some()
    }

    /// Ask the AI supervisor for a decision on an escalated tool call.
    ///
    /// Returns the decision or an error if the AI client is not available.
    async fn ask_ai_supervisor(
        &self,
        tool_use: &ToolUse,
        reason: &str,
    ) -> Result<SupervisorDecision, AiError> {
        let ai_client = self.ai_client.as_ref().ok_or(AiError::MissingApiKey)?;

        let context = format!(
            "Escalation reason: {reason}\nSession: {}",
            self.session_id.as_deref().unwrap_or("unknown")
        );

        ai_client
            .ask_supervisor(&tool_use.name, &tool_use.input, &context)
            .await
    }

    /// Handle an escalation by consulting the AI supervisor.
    ///
    /// Returns whether to allow or deny the tool call.
    async fn handle_escalation(&self, tool_use: &ToolUse, reason: &str) -> EscalationResult {
        match self.ask_ai_supervisor(tool_use, reason).await {
            Ok(SupervisorDecision::Allow { reason }) => {
                tracing::info!(
                    tool = %tool_use.name,
                    %reason,
                    "AI supervisor allowed tool call"
                );
                EscalationResult::Allow
            }
            Ok(SupervisorDecision::Deny { reason }) => {
                tracing::warn!(
                    tool = %tool_use.name,
                    %reason,
                    "AI supervisor denied tool call"
                );
                EscalationResult::Deny(reason)
            }
            Ok(SupervisorDecision::Guide { reason, guidance }) => {
                // For now, treat guidance as an allow with logged guidance
                tracing::info!(
                    tool = %tool_use.name,
                    %reason,
                    %guidance,
                    "AI supervisor provided guidance - allowing"
                );
                EscalationResult::Allow
            }
            Err(e) => {
                tracing::error!(
                    tool = %tool_use.name,
                    error = %e,
                    "AI supervisor error - denying for safety"
                );
                EscalationResult::Deny(format!("AI supervisor error: {e}"))
            }
        }
    }

    /// Run the supervisor loop without an attached process.
    ///
    /// Processes events from the channel until completion or error.
    ///
    /// # Errors
    ///
    /// This function currently does not return errors, but the signature
    /// allows for future error handling additions.
    pub async fn run_without_process(&mut self) -> Result<SupervisorResult, SupervisorError> {
        self.state.transition(SessionState::Running);

        loop {
            if let Some(event) = self.event_rx.recv().await {
                let action = self.handle_event(&event);
                match action {
                    EventAction::Continue => {}
                    EventAction::Complete(result) => {
                        self.state.transition(SessionState::Completed);
                        return Ok(result);
                    }
                    EventAction::Kill(reason) => {
                        self.state.transition(SessionState::Failed);
                        return Ok(SupervisorResult::Killed { reason });
                    }
                    EventAction::Escalate { tool_use, reason } => {
                        // Handle AI supervisor escalation
                        match self.handle_escalation(&tool_use, &reason).await {
                            EscalationResult::Allow => {
                                self.state.record_approval();
                                self.state.transition(SessionState::Running);
                            }
                            EscalationResult::Deny(deny_reason) => {
                                self.state.record_denial();
                                self.state.transition(SessionState::Failed);
                                return Ok(SupervisorResult::Killed {
                                    reason: deny_reason,
                                });
                            }
                        }
                    }
                }
            } else {
                // Channel closed
                self.state.transition(SessionState::Completed);
                return Ok(SupervisorResult::ProcessExited);
            }
        }
    }

    /// Run the supervisor loop with an attached process.
    ///
    /// Processes events and terminates the process if a policy violation occurs.
    ///
    /// # Errors
    ///
    /// Returns `SupervisorError::TerminateError` if the process cannot be terminated.
    pub async fn run(&mut self) -> Result<SupervisorResult, SupervisorError> {
        self.state.transition(SessionState::Running);

        loop {
            if let Some(event) = self.event_rx.recv().await {
                let action = self.handle_event(&event);
                match action {
                    EventAction::Continue => {}
                    EventAction::Complete(result) => {
                        self.state.transition(SessionState::Completed);
                        return Ok(result);
                    }
                    EventAction::Kill(reason) => {
                        self.state.transition(SessionState::Failed);
                        self.terminate_process().await?;
                        return Ok(SupervisorResult::Killed { reason });
                    }
                    EventAction::Escalate { tool_use, reason } => {
                        // Handle AI supervisor escalation
                        match self.handle_escalation(&tool_use, &reason).await {
                            EscalationResult::Allow => {
                                self.state.record_approval();
                                self.state.transition(SessionState::Running);
                            }
                            EscalationResult::Deny(deny_reason) => {
                                self.state.record_denial();
                                self.state.transition(SessionState::Failed);
                                self.terminate_process().await?;
                                return Ok(SupervisorResult::Killed {
                                    reason: deny_reason,
                                });
                            }
                        }
                    }
                }
            } else {
                // Channel closed, process likely exited
                self.state.transition(SessionState::Completed);
                return Ok(SupervisorResult::ProcessExited);
            }
        }
    }

    /// Handle a single event and return the action to take.
    fn handle_event(&mut self, event: &ClaudeEvent) -> EventAction {
        // Extract session ID if available
        if let Some(id) = event.session_id() {
            self.session_id = Some(id.to_string());
        }

        match event {
            ClaudeEvent::System(init) => {
                tracing::info!(
                    session_id = %init.session_id,
                    model = %init.model,
                    tools = ?init.tools,
                    "Session initialized"
                );
                EventAction::Continue
            }
            ClaudeEvent::ToolUse(tool_use) => {
                self.state.record_tool_call();
                self.evaluate_tool_use(tool_use)
            }
            ClaudeEvent::Result(result) => {
                tracing::info!(
                    session_id = %result.session_id,
                    cost_usd = ?result.cost_usd,
                    is_error = result.is_error,
                    "Session completed"
                );
                EventAction::Complete(SupervisorResult::from_result_event(result))
            }
            ClaudeEvent::MessageStop => EventAction::Complete(SupervisorResult::Completed {
                session_id: self.session_id.clone(),
                cost_usd: None,
            }),
            _ => EventAction::Continue,
        }
    }

    /// Evaluate a tool use against the policy.
    fn evaluate_tool_use(&mut self, tool_use: &ToolUse) -> EventAction {
        let decision = self.policy.evaluate(&tool_use.name, &tool_use.input);

        match decision {
            PolicyDecision::Allow => {
                self.state.record_approval();
                tracing::debug!(tool = %tool_use.name, "Tool call allowed");
                EventAction::Continue
            }
            PolicyDecision::AllowWithModification(_) => {
                // In the runner context, we treat modified input as a simple allow
                // The actual modification is handled by the hook handler
                self.state.record_approval();
                tracing::debug!(tool = %tool_use.name, "Tool call allowed with modification");
                EventAction::Continue
            }
            PolicyDecision::Deny(reason) => {
                self.state.record_denial();
                tracing::warn!(tool = %tool_use.name, reason = %reason, "Tool call denied");
                EventAction::Kill(reason)
            }
            PolicyDecision::Escalate(reason) => {
                self.state.transition(SessionState::WaitingForSupervisor);
                // Check if AI supervisor is available for escalation
                if self.ai_client.is_some() {
                    tracing::info!(
                        tool = %tool_use.name,
                        id = %tool_use.id,
                        %reason,
                        "Tool call escalated to AI supervisor"
                    );
                    // Return a pending escalation action that will be handled asynchronously
                    EventAction::Escalate {
                        tool_use: tool_use.clone(),
                        reason,
                    }
                } else {
                    tracing::warn!(
                        tool = %tool_use.name,
                        id = %tool_use.id,
                        %reason,
                        "Tool call escalated but no AI supervisor available - denying"
                    );
                    self.state.record_denial();
                    EventAction::Kill(format!("Escalation denied (no AI supervisor): {reason}"))
                }
            }
        }
    }

    /// Terminate the attached process.
    async fn terminate_process(&mut self) -> Result<(), SupervisorError> {
        if let Some(ref mut process) = self.process {
            process
                .graceful_terminate(DEFAULT_TERMINATE_TIMEOUT)
                .await?;
        }
        Ok(())
    }

    /// Get the current session state.
    #[must_use]
    pub fn state(&self) -> SessionState {
        self.state.state()
    }

    /// Get session statistics.
    #[must_use]
    pub fn stats(&self) -> SessionStats {
        self.state.stats()
    }

    /// Get the session ID, if available.
    #[must_use]
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }
}

/// Result of an AI supervisor escalation.
enum EscalationResult {
    /// Allow the tool call to proceed.
    Allow,
    /// Deny the tool call with a reason.
    Deny(String),
}

/// Internal action type for event handling.
enum EventAction {
    /// Continue processing events.
    Continue,
    /// Complete the session with a result.
    Complete(SupervisorResult),
    /// Kill the process with a reason.
    Kill(String),
    /// Escalate to AI supervisor for decision.
    Escalate { tool_use: ToolUse, reason: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{ResultEvent, SystemInit};
    use crate::supervisor::PolicyLevel;
    use tokio::sync::mpsc;

    fn create_test_supervisor() -> (Supervisor, tokio::sync::mpsc::Sender<ClaudeEvent>) {
        let (tx, rx) = mpsc::channel(32);
        let policy = PolicyEngine::new(PolicyLevel::Permissive);
        let supervisor = Supervisor::new(policy, rx);
        (supervisor, tx)
    }

    #[tokio::test]
    async fn test_supervisor_new() {
        let (supervisor, _tx) = create_test_supervisor();
        assert_eq!(supervisor.state(), SessionState::Idle);
        assert!(supervisor.session_id().is_none());
    }

    #[tokio::test]
    async fn test_supervisor_stats() {
        let (supervisor, _tx) = create_test_supervisor();
        let stats = supervisor.stats();
        assert_eq!(stats.tool_calls, 0);
        assert_eq!(stats.approvals, 0);
        assert_eq!(stats.denials, 0);
    }

    #[tokio::test]
    async fn test_supervisor_handles_system_init() {
        let (mut supervisor, tx) = create_test_supervisor();

        let init = ClaudeEvent::System(SystemInit {
            cwd: "/test".to_string(),
            tools: vec!["Read".to_string(), "Write".to_string()],
            model: "claude-3".to_string(),
            session_id: "test-session".to_string(),
            mcp_servers: vec![],
            subtype: None,
        });

        tx.send(init).await.unwrap();
        drop(tx);

        let result = supervisor.run_without_process().await.unwrap();
        assert!(matches!(result, SupervisorResult::ProcessExited));
        assert_eq!(supervisor.session_id(), Some("test-session"));
    }

    #[tokio::test]
    async fn test_supervisor_allows_safe_tool() {
        let (mut supervisor, tx) = create_test_supervisor();

        let tool_use = ClaudeEvent::ToolUse(ToolUse {
            id: "tool-1".to_string(),
            name: "Read".to_string(),
            input: serde_json::json!({ "file_path": "/test/file.txt" }),
        });

        tx.send(tool_use).await.unwrap();
        drop(tx);

        let result = supervisor.run_without_process().await.unwrap();
        assert!(matches!(result, SupervisorResult::ProcessExited));
        assert_eq!(supervisor.stats().tool_calls, 1);
        assert_eq!(supervisor.stats().approvals, 1);
    }

    #[tokio::test]
    async fn test_supervisor_denies_dangerous_command() {
        let (mut supervisor, tx) = create_test_supervisor();

        let tool_use = ClaudeEvent::ToolUse(ToolUse {
            id: "tool-1".to_string(),
            name: "Bash".to_string(),
            input: serde_json::json!({ "command": "rm -rf /" }),
        });

        tx.send(tool_use).await.unwrap();

        let result = supervisor.run_without_process().await.unwrap();
        assert!(matches!(result, SupervisorResult::Killed { .. }));
        assert_eq!(supervisor.stats().denials, 1);
    }

    #[tokio::test]
    async fn test_supervisor_handles_result() {
        let (mut supervisor, tx) = create_test_supervisor();

        let result_event = ClaudeEvent::Result(ResultEvent {
            result: "Task completed".to_string(),
            session_id: "test-session".to_string(),
            is_error: false,
            cost_usd: Some(0.05),
            duration_ms: Some(1000),
        });

        tx.send(result_event).await.unwrap();

        let result = supervisor.run_without_process().await.unwrap();
        match result {
            SupervisorResult::Completed {
                session_id,
                cost_usd,
            } => {
                assert_eq!(session_id, Some("test-session".to_string()));
                assert_eq!(cost_usd, Some(0.05));
            }
            _ => panic!("Expected Completed result"),
        }
    }

    #[tokio::test]
    async fn test_supervisor_handles_message_stop() {
        let (mut supervisor, tx) = create_test_supervisor();

        tx.send(ClaudeEvent::MessageStop).await.unwrap();

        let result = supervisor.run_without_process().await.unwrap();
        assert!(matches!(result, SupervisorResult::Completed { .. }));
    }

    #[tokio::test]
    async fn test_supervisor_with_strict_policy() {
        let (tx, rx) = mpsc::channel(32);
        let policy = PolicyEngine::new(PolicyLevel::Strict);
        let mut supervisor = Supervisor::new(policy, rx);

        let tool_use = ClaudeEvent::ToolUse(ToolUse {
            id: "tool-1".to_string(),
            name: "UnknownTool".to_string(),
            input: serde_json::json!({}),
        });

        tx.send(tool_use).await.unwrap();
        drop(tx);

        // Strict policy escalates unknown tools, which denies in Phase 1
        let result = supervisor.run_without_process().await.unwrap();
        assert!(matches!(result, SupervisorResult::Killed { .. }));
        assert_eq!(supervisor.stats().denials, 1);
    }

    #[tokio::test]
    async fn test_supervisor_channel_closed() {
        let (mut supervisor, tx) = create_test_supervisor();
        drop(tx);

        let result = supervisor.run_without_process().await.unwrap();
        assert!(matches!(result, SupervisorResult::ProcessExited));
    }
}
