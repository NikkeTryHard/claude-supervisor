//! Supervisor runner for orchestrating Claude Code execution.
//!
//! This module provides the main orchestration layer that connects the
//! process spawner, stream parser, and policy engine together.

use std::collections::VecDeque;
use std::path::Path;
use std::time::Duration;

use tokio::sync::mpsc::Receiver;
use tokio_util::sync::CancellationToken;

use crate::ai::{AiClient, AiError, ContextCompressor, SupervisorContext, SupervisorDecision};
use crate::cli::{
    ClaudeEvent, ClaudeProcess, ContentDelta, ResultEvent, StreamParser, ToolUse,
    DEFAULT_CHANNEL_BUFFER,
};
use crate::display;
use crate::knowledge::{
    ClaudeMdSource, KnowledgeAggregator, KnowledgeSource, MemorySource, SessionHistorySource,
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
    /// Session was cancelled via cancellation token.
    Cancelled,
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

/// Timeout for AI supervisor API calls.
const AI_SUPERVISOR_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum number of events to keep in history for context.
const MAX_EVENT_HISTORY: usize = 50;

/// Supervisor for orchestrating Claude Code execution with policy enforcement.
pub struct Supervisor {
    process: Option<ClaudeProcess>,
    policy: PolicyEngine,
    event_rx: Receiver<ClaudeEvent>,
    state: SessionStateMachine,
    session_id: Option<String>,
    ai_client: Option<AiClient>,
    event_history: VecDeque<ClaudeEvent>,
    cwd: Option<String>,
    task: Option<String>,
    knowledge: Option<KnowledgeAggregator>,
    cancel: Option<CancellationToken>,
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
            event_history: VecDeque::new(),
            cwd: None,
            task: None,
            knowledge: None,
            cancel: None,
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
            event_history: VecDeque::new(),
            cwd: None,
            task: None,
            knowledge: None,
            cancel: None,
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
            event_history: VecDeque::new(),
            cwd: None,
            task: None,
            knowledge: None,
            cancel: None,
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
            event_history: VecDeque::new(),
            cwd: None,
            task: None,
            knowledge: None,
            cancel: None,
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
            event_history: VecDeque::new(),
            cwd: None,
            task: None,
            knowledge: None,
            cancel: None,
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
            event_history: VecDeque::new(),
            cwd: None,
            task: None,
            knowledge: None,
            cancel: None,
        })
    }

    /// Check if AI supervision is available.
    #[must_use]
    pub fn has_ai_supervisor(&self) -> bool {
        self.ai_client.is_some()
    }

    /// Initialize knowledge sources from a project directory.
    ///
    /// Loads CLAUDE.md (project and global) and session history.
    pub async fn init_knowledge(&mut self, project_dir: &Path) {
        let mut aggregator = KnowledgeAggregator::new();

        // Load CLAUDE.md sources (project + global)
        let claude_md = ClaudeMdSource::load_with_global(project_dir).await;
        if claude_md.context_summary().is_some() {
            tracing::info!("Loaded CLAUDE.md knowledge source");
            aggregator.add_source(Box::new(claude_md));
        }

        // Load session history
        let history = SessionHistorySource::load(project_dir).await;
        if history.context_summary().is_some() {
            tracing::info!(
                pairs = history.pairs.len(),
                "Loaded session history knowledge source"
            );
            aggregator.add_source(Box::new(history));
        }

        // Load memory file
        let memory = MemorySource::load(project_dir).await;
        if memory.context_summary().is_some() {
            tracing::info!(facts = memory.len(), "Loaded memory knowledge source");
            aggregator.add_source(Box::new(memory));
        }

        if aggregator.has_knowledge() {
            self.knowledge = Some(aggregator);
        } else {
            tracing::debug!("No knowledge sources available");
        }
    }

    /// Set a pre-built knowledge aggregator.
    pub fn set_knowledge(&mut self, knowledge: KnowledgeAggregator) {
        self.knowledge = Some(knowledge);
    }

    /// Check if knowledge sources are available.
    #[must_use]
    pub fn has_knowledge(&self) -> bool {
        self.knowledge
            .as_ref()
            .is_some_and(KnowledgeAggregator::has_knowledge)
    }

    /// Get recent events from history (most recent first).
    #[must_use]
    pub fn recent_events(&self, n: usize) -> Vec<&ClaudeEvent> {
        self.event_history.iter().rev().take(n).collect()
    }

    /// Set the task being performed.
    pub fn set_task(&mut self, task: impl Into<String>) {
        self.task = Some(task.into());
    }

    /// Set a cancellation token for graceful shutdown.
    #[must_use]
    pub fn with_cancellation(mut self, cancel: CancellationToken) -> Self {
        self.cancel = Some(cancel);
        self
    }

    /// Check if this supervisor has been cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancel
            .as_ref()
            .is_some_and(CancellationToken::is_cancelled)
    }

    /// Ask the AI supervisor for a decision on an escalated tool call.
    ///
    /// Returns the decision or an error if the AI client is not available.
    /// Times out after `AI_SUPERVISOR_TIMEOUT` seconds.
    async fn ask_ai_supervisor(
        &self,
        tool_use: &ToolUse,
        reason: &str,
    ) -> Result<SupervisorDecision, AiError> {
        let ai_client = self.ai_client.as_ref().ok_or(AiError::MissingApiKey(
            "AI client not configured".to_string(),
        ))?;

        // Build context using SupervisorContext builder
        let context = SupervisorContext::new()
            .with_task(self.task.as_deref().unwrap_or("unknown"))
            .with_cwd(self.cwd.as_deref().unwrap_or("unknown"))
            .with_session_id(self.session_id.as_deref().unwrap_or("unknown"));

        // Compress event history for context
        let compressor = ContextCompressor::default();
        let events: Vec<ClaudeEvent> = self.event_history.iter().cloned().collect();
        let compressed_history = compressor.compress(&events);

        // Build knowledge context if available
        let knowledge_context = self
            .knowledge
            .as_ref()
            .map(KnowledgeAggregator::build_context)
            .filter(|s| !s.is_empty());

        let context_str = if let Some(knowledge) = knowledge_context {
            tracing::debug!("Including knowledge context in AI escalation");
            format!(
                "Escalation reason: {reason}\n\n{}\n\n## Project Knowledge\n\n{knowledge}\n\n## Recent Activity\n\n{compressed_history}",
                context.build()
            )
        } else {
            format!(
                "Escalation reason: {reason}\n\n{}\n\nRecent Activity:\n{compressed_history}",
                context.build()
            )
        };

        tokio::time::timeout(
            AI_SUPERVISOR_TIMEOUT,
            ai_client.ask_supervisor(&tool_use.name, &tool_use.input, &context_str),
        )
        .await
        .map_err(|_| AiError::Timeout)?
    }

    /// Handle an escalation by consulting the AI supervisor.
    ///
    /// Returns whether to allow or deny the tool call.
    async fn handle_escalation(&self, tool_use: &ToolUse, reason: &str) -> EscalationResult {
        match self.ask_ai_supervisor(tool_use, reason).await {
            Ok(SupervisorDecision::Allow { reason }) => {
                display::print_supervisor_decision("ALLOW", &tool_use.name);
                tracing::info!(
                    tool = %tool_use.name,
                    %reason,
                    "AI supervisor allowed tool call"
                );
                EscalationResult::Allow
            }
            Ok(SupervisorDecision::Deny { reason }) => {
                display::print_supervisor_decision("DENY", &tool_use.name);
                tracing::warn!(
                    tool = %tool_use.name,
                    %reason,
                    "AI supervisor denied tool call"
                );
                EscalationResult::Deny(reason)
            }
            Ok(SupervisorDecision::Guide { reason, guidance }) => {
                // For now, treat guidance as an allow with logged guidance
                display::print_supervisor_decision("GUIDE", &tool_use.name);
                tracing::info!(
                    tool = %tool_use.name,
                    %reason,
                    %guidance,
                    "AI supervisor provided guidance - allowing"
                );
                EscalationResult::Allow
            }
            Err(e) => {
                display::print_error(&format!("AI supervisor error: {e}"));
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
            if let Some(ref cancel) = self.cancel {
                tokio::select! {
                    biased;

                    () = cancel.cancelled() => {
                        tracing::info!("Session cancelled via token");
                        self.state.transition(SessionState::Completed);
                        return Ok(SupervisorResult::Cancelled);
                    }
                    event = self.event_rx.recv() => {
                        if let Some(event) = event {
                            let action = self.handle_event(&event);
                            if let Some(result) = self.process_action(action).await? {
                                return Ok(result);
                            }
                        } else {
                            self.state.transition(SessionState::Completed);
                            return Ok(SupervisorResult::ProcessExited);
                        }
                    }
                }
            } else {
                // Original behavior without cancellation
                let Some(event) = self.event_rx.recv().await else {
                    self.state.transition(SessionState::Completed);
                    return Ok(SupervisorResult::ProcessExited);
                };
                let action = self.handle_event(&event);
                if let Some(result) = self.process_action(action).await? {
                    return Ok(result);
                }
            }
        }
    }

    /// Process an event action and return the result if the loop should exit.
    async fn process_action(
        &mut self,
        action: EventAction,
    ) -> Result<Option<SupervisorResult>, SupervisorError> {
        match action {
            EventAction::Continue => Ok(None),
            EventAction::Complete(result) => {
                self.state.transition(SessionState::Completed);
                Ok(Some(result))
            }
            EventAction::Kill(reason) => {
                self.state.transition(SessionState::Failed);
                Ok(Some(SupervisorResult::Killed { reason }))
            }
            EventAction::Escalate { tool_use, reason } => {
                match self.handle_escalation(&tool_use, &reason).await {
                    EscalationResult::Allow => {
                        self.state.record_approval();
                        self.state.transition(SessionState::Running);
                        Ok(None)
                    }
                    EscalationResult::Deny(deny_reason) => {
                        self.state.record_denial();
                        self.state.transition(SessionState::Failed);
                        Ok(Some(SupervisorResult::Killed {
                            reason: deny_reason,
                        }))
                    }
                }
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
            if let Some(ref cancel) = self.cancel {
                tokio::select! {
                    biased;

                    () = cancel.cancelled() => {
                        tracing::info!("Session cancelled via token");
                        self.terminate_process().await?;
                        self.state.transition(SessionState::Completed);
                        return Ok(SupervisorResult::Cancelled);
                    }
                    event = self.event_rx.recv() => {
                        if let Some(event) = event {
                            let action = self.handle_event(&event);
                            if let Some(result) = self.process_action_with_terminate(action).await? {
                                return Ok(result);
                            }
                        } else {
                            self.state.transition(SessionState::Completed);
                            return Ok(SupervisorResult::ProcessExited);
                        }
                    }
                }
            } else {
                // Original behavior without cancellation
                let Some(event) = self.event_rx.recv().await else {
                    // Channel closed, process likely exited
                    self.state.transition(SessionState::Completed);
                    return Ok(SupervisorResult::ProcessExited);
                };
                let action = self.handle_event(&event);
                if let Some(result) = self.process_action_with_terminate(action).await? {
                    return Ok(result);
                }
            }
        }
    }

    /// Process an event action with process termination on kill.
    async fn process_action_with_terminate(
        &mut self,
        action: EventAction,
    ) -> Result<Option<SupervisorResult>, SupervisorError> {
        match action {
            EventAction::Continue => Ok(None),
            EventAction::Complete(result) => {
                self.state.transition(SessionState::Completed);
                Ok(Some(result))
            }
            EventAction::Kill(reason) => {
                self.state.transition(SessionState::Failed);
                self.terminate_process().await?;
                Ok(Some(SupervisorResult::Killed { reason }))
            }
            EventAction::Escalate { tool_use, reason } => {
                match self.handle_escalation(&tool_use, &reason).await {
                    EscalationResult::Allow => {
                        self.state.record_approval();
                        self.state.transition(SessionState::Running);
                        Ok(None)
                    }
                    EscalationResult::Deny(deny_reason) => {
                        self.state.record_denial();
                        self.state.transition(SessionState::Failed);
                        self.terminate_process().await?;
                        Ok(Some(SupervisorResult::Killed {
                            reason: deny_reason,
                        }))
                    }
                }
            }
        }
    }

    /// Handle a single event and return the action to take.
    fn handle_event(&mut self, event: &ClaudeEvent) -> EventAction {
        // Store event in history
        self.event_history.push_back(event.clone());
        if self.event_history.len() > MAX_EVENT_HISTORY {
            self.event_history.pop_front();
        }

        // Extract session ID if available
        if let Some(id) = event.session_id() {
            self.session_id = Some(id.to_string());
        }

        match event {
            ClaudeEvent::System(init) => {
                self.cwd = Some(init.cwd.clone());
                display::print_session_start(&init.model, &init.session_id);
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
                display::print_tool_request(&tool_use.name, &tool_use.input);
                self.evaluate_tool_use(tool_use)
            }
            ClaudeEvent::Result(result) => {
                display::print_session_end(
                    result.cost_usd,
                    result.is_error,
                    Some(&result.session_id),
                    Some(&result.result),
                );
                tracing::info!(
                    session_id = %result.session_id,
                    cost_usd = ?result.cost_usd,
                    is_error = result.is_error,
                    "Session completed"
                );
                EventAction::Complete(SupervisorResult::from_result_event(result))
            }
            ClaudeEvent::ContentBlockDelta { delta, .. } => {
                match delta {
                    ContentDelta::ThinkingDelta { thinking } => {
                        display::print_thinking(thinking);
                    }
                    ContentDelta::TextDelta { text } => {
                        display::print_text(text);
                    }
                    _ => {}
                }
                EventAction::Continue
            }
            ClaudeEvent::MessageStop => EventAction::Complete(SupervisorResult::Completed {
                session_id: self.session_id.clone(),
                cost_usd: None,
            }),
            ClaudeEvent::ToolResult(result) => {
                display::print_tool_result(&result.tool_use_id, &result.content, result.is_error);
                tracing::debug!(
                    tool_use_id = %result.tool_use_id,
                    is_error = result.is_error,
                    content_len = result.content.len(),
                    "Tool result received"
                );
                EventAction::Continue
            }
            ClaudeEvent::User {
                tool_use_result, ..
            } => {
                // User events contain tool results from Claude Code
                if let Some(result_value) = tool_use_result {
                    let result_text = match &result_value {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    let is_error = result_text.contains("error") || result_text.contains("Error");
                    display::print_tool_result("user", &result_text, is_error);
                    tracing::debug!(
                        content_len = result_text.len(),
                        "Tool result from user event"
                    );
                }
                EventAction::Continue
            }
            _ => EventAction::Continue,
        }
    }

    /// Evaluate a tool use against the policy.
    fn evaluate_tool_use(&mut self, tool_use: &ToolUse) -> EventAction {
        let decision = self.policy.evaluate(&tool_use.name, &tool_use.input);

        match decision {
            PolicyDecision::Allow => {
                self.state.record_approval();
                display::print_allow(&tool_use.name);
                tracing::debug!(tool = %tool_use.name, "Tool call allowed");
                EventAction::Continue
            }
            PolicyDecision::AllowWithModification(_) => {
                // In the runner context, we treat modified input as a simple allow
                // The actual modification is handled by the hook handler
                self.state.record_approval();
                display::print_allow(&tool_use.name);
                tracing::debug!(tool = %tool_use.name, "Tool call allowed with modification");
                EventAction::Continue
            }
            PolicyDecision::Deny(reason) => {
                self.state.record_denial();
                display::print_deny(&tool_use.name, &reason);
                tracing::warn!(tool = %tool_use.name, reason = %reason, "Tool call denied");
                EventAction::Kill(reason)
            }
            PolicyDecision::Escalate(reason) => {
                self.state.transition(SessionState::WaitingForSupervisor);
                display::print_escalate(&tool_use.name, &reason);
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
            ..Default::default()
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
    async fn test_supervisor_handles_tool_result() {
        let (mut supervisor, tx) = create_test_supervisor();

        tx.send(ClaudeEvent::ToolResult(crate::cli::ToolResult {
            tool_use_id: "tool-123".to_string(),
            content: "File contents here".to_string(),
            is_error: false,
        }))
        .await
        .unwrap();

        drop(tx);

        let result = supervisor.run_without_process().await.unwrap();
        assert!(matches!(result, SupervisorResult::ProcessExited));

        let recent = supervisor.recent_events(10);
        assert_eq!(recent.len(), 1);
        assert!(matches!(recent[0], ClaudeEvent::ToolResult(_)));
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

    #[tokio::test]
    async fn test_supervisor_tracks_event_history() {
        let (mut supervisor, tx) = create_test_supervisor();

        // Send multiple events
        let init = ClaudeEvent::System(SystemInit {
            cwd: "/test".to_string(),
            tools: vec!["Read".to_string()],
            model: "claude-3".to_string(),
            session_id: "test-session".to_string(),
            mcp_servers: vec![],
            subtype: None,
            ..Default::default()
        });
        tx.send(init).await.unwrap();

        let tool_use = ClaudeEvent::ToolUse(ToolUse {
            id: "tool-1".to_string(),
            name: "Read".to_string(),
            input: serde_json::json!({"file_path": "/test.txt"}),
        });
        tx.send(tool_use).await.unwrap();

        drop(tx);
        let _ = supervisor.run_without_process().await.unwrap();

        // Check that events were tracked
        let recent = supervisor.recent_events(10);
        assert_eq!(recent.len(), 2);
    }

    #[tokio::test]
    async fn test_supervisor_extracts_cwd_from_init() {
        let (mut supervisor, tx) = create_test_supervisor();

        let init = ClaudeEvent::System(SystemInit {
            cwd: "/home/user/project".to_string(),
            tools: vec![],
            model: "claude-3".to_string(),
            session_id: "test-session".to_string(),
            mcp_servers: vec![],
            subtype: None,
            ..Default::default()
        });
        tx.send(init).await.unwrap();
        drop(tx);

        let _ = supervisor.run_without_process().await.unwrap();
        assert_eq!(supervisor.cwd, Some("/home/user/project".to_string()));
    }

    #[tokio::test]
    async fn test_supervisor_set_task() {
        let (mut supervisor, _tx) = create_test_supervisor();
        supervisor.set_task("Fix the authentication bug");
        assert_eq!(
            supervisor.task,
            Some("Fix the authentication bug".to_string())
        );
    }

    #[test]
    fn test_ai_error_timeout_variant() {
        let error = AiError::Timeout;
        assert_eq!(error.to_string(), "AI supervisor request timed out");
    }

    #[test]
    fn test_supervisor_has_knowledge_default_false() {
        let (supervisor, _tx) = create_test_supervisor();
        assert!(!supervisor.has_knowledge());
    }

    #[test]
    fn test_supervisor_set_knowledge() {
        use crate::knowledge::KnowledgeFact;

        struct MockSource;
        impl KnowledgeSource for MockSource {
            fn source_name(&self) -> &'static str {
                "mock"
            }
            fn query(&self, _: &str) -> Option<KnowledgeFact> {
                Some(KnowledgeFact {
                    source: "mock".to_string(),
                    content: "test fact".to_string(),
                    relevance: 1.0,
                })
            }
            fn context_summary(&self) -> Option<String> {
                Some("Mock context".to_string())
            }
        }

        let (mut supervisor, _tx) = create_test_supervisor();
        let mut aggregator = KnowledgeAggregator::new();
        aggregator.add_source(Box::new(MockSource));
        supervisor.set_knowledge(aggregator);

        assert!(supervisor.has_knowledge());
    }

    #[tokio::test]
    async fn test_supervisor_init_knowledge_empty_dir() {
        let (mut supervisor, _tx) = create_test_supervisor();
        let temp_dir = std::env::temp_dir().join("supervisor_test_empty_nonexistent_12345");

        // Use a path that definitely won't have CLAUDE.md
        // Note: This may still load global ~/.claude/CLAUDE.md if it exists
        supervisor.init_knowledge(&temp_dir).await;

        // The test verifies init_knowledge doesn't panic on missing directories
        // has_knowledge() may be true if global CLAUDE.md exists
    }
}
