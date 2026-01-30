//! Audit event types for supervisor logging.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Type of audit event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    /// Session started.
    SessionStart,
    /// Session ended.
    SessionEnd,
    /// Tool was used.
    ToolUse,
    /// Policy decision was made.
    PolicyDecision,
    /// Decision was escalated to AI supervisor.
    AiEscalation,
    /// An error occurred.
    Error,
}

impl EventType {
    /// Returns the string representation for database storage.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SessionStart => "session_start",
            Self::SessionEnd => "session_end",
            Self::ToolUse => "tool_use",
            Self::PolicyDecision => "policy_decision",
            Self::AiEscalation => "ai_escalation",
            Self::Error => "error",
        }
    }
}

/// Decision made by the policy engine or AI supervisor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    /// Action was allowed.
    Allow,
    /// Action was denied.
    Deny,
    /// Decision was escalated to AI supervisor.
    Escalate,
}

impl Decision {
    /// Returns the string representation for database storage.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
            Self::Escalate => "escalate",
        }
    }
}

/// An audit event recording a supervisor action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Unique event ID.
    pub id: Uuid,
    /// Session this event belongs to.
    pub session_id: Uuid,
    /// When the event occurred.
    pub timestamp: DateTime<Utc>,
    /// Type of event.
    pub event_type: EventType,
    /// Name of the tool involved, if any.
    pub tool_name: Option<String>,
    /// Tool input parameters as JSON, if any.
    pub tool_input: Option<serde_json::Value>,
    /// Decision made, if applicable.
    pub decision: Option<Decision>,
    /// Reason for the decision.
    pub reason: Option<String>,
}

impl AuditEvent {
    /// Create a new builder for an audit event.
    #[must_use]
    pub fn builder(session_id: Uuid, event_type: EventType) -> AuditEventBuilder {
        AuditEventBuilder::new(session_id, event_type)
    }
}

/// Builder for creating audit events.
#[derive(Debug, Clone)]
pub struct AuditEventBuilder {
    session_id: Uuid,
    timestamp: DateTime<Utc>,
    event_type: EventType,
    tool_name: Option<String>,
    tool_input: Option<serde_json::Value>,
    decision: Option<Decision>,
    reason: Option<String>,
}

impl AuditEventBuilder {
    /// Create a new builder with required fields.
    pub fn new(session_id: Uuid, event_type: EventType) -> Self {
        Self {
            session_id,
            timestamp: Utc::now(),
            event_type,
            tool_name: None,
            tool_input: None,
            decision: None,
            reason: None,
        }
    }

    /// Set a custom timestamp.
    pub fn timestamp(mut self, timestamp: DateTime<Utc>) -> Self {
        self.timestamp = timestamp;
        self
    }

    /// Set the tool name.
    pub fn tool_name(mut self, name: impl Into<String>) -> Self {
        self.tool_name = Some(name.into());
        self
    }

    /// Set the tool input.
    pub fn tool_input(mut self, input: serde_json::Value) -> Self {
        self.tool_input = Some(input);
        self
    }

    /// Set the decision.
    pub fn decision(mut self, decision: Decision) -> Self {
        self.decision = Some(decision);
        self
    }

    /// Set the reason.
    pub fn reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    /// Build the audit event.
    pub fn build(self) -> AuditEvent {
        AuditEvent {
            id: Uuid::new_v4(),
            session_id: self.session_id,
            timestamp: self.timestamp,
            event_type: self.event_type,
            tool_name: self.tool_name,
            tool_input: self.tool_input,
            decision: self.decision,
            reason: self.reason,
        }
    }
}

/// An audit session representing a supervisor run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditSession {
    /// Unique session ID.
    pub id: Uuid,
    /// When the session started.
    pub started_at: DateTime<Utc>,
    /// When the session ended, if finished.
    pub ended_at: Option<DateTime<Utc>>,
    /// The task being supervised.
    pub task: String,
    /// The result of the session, if finished.
    pub result: Option<String>,
}

impl AuditSession {
    /// Create a new session with the given task.
    pub fn new(task: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            started_at: Utc::now(),
            ended_at: None,
            task: task.into(),
            result: None,
        }
    }

    /// Create a session with a specific ID.
    pub fn with_id(id: Uuid, task: impl Into<String>) -> Self {
        Self {
            id,
            started_at: Utc::now(),
            ended_at: None,
            task: task.into(),
            result: None,
        }
    }

    /// Mark the session as ended with a result.
    pub fn end(&mut self, result: impl Into<String>) {
        self.ended_at = Some(Utc::now());
        self.result = Some(result.into());
    }
}

/// Metrics for a session's resource usage.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionMetrics {
    /// Session this metrics belongs to.
    pub session_id: Uuid,
    /// Total input tokens used.
    pub input_tokens: u64,
    /// Total output tokens used.
    pub output_tokens: u64,
    /// Number of API calls made.
    pub api_calls: u64,
    /// Number of cache hits.
    pub cache_hits: u64,
    /// Estimated cost in USD cents.
    pub estimated_cost_cents: u64,
}

impl SessionMetrics {
    /// Create new metrics for a session.
    #[must_use]
    pub fn new(session_id: Uuid) -> Self {
        Self {
            session_id,
            ..Default::default()
        }
    }

    /// Add token usage to the metrics.
    pub fn add_tokens(&mut self, input: u64, output: u64) {
        self.input_tokens += input;
        self.output_tokens += output;
    }

    /// Record an API call.
    pub fn record_api_call(&mut self) {
        self.api_calls += 1;
    }

    /// Record a cache hit.
    pub fn record_cache_hit(&mut self) {
        self.cache_hits += 1;
    }

    /// Calculate and set estimated cost based on token usage.
    /// Uses approximate Claude pricing: $3/1M input, $15/1M output tokens.
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    pub fn calculate_cost(&mut self) {
        // Cost in cents: input=$0.003/1K, output=$0.015/1K
        let input_cost = (self.input_tokens as f64 * 0.3) / 1000.0;
        let output_cost = (self.output_tokens as f64 * 1.5) / 1000.0;
        self.estimated_cost_cents = (input_cost + output_cost).ceil() as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_as_str() {
        assert_eq!(EventType::SessionStart.as_str(), "session_start");
        assert_eq!(EventType::SessionEnd.as_str(), "session_end");
        assert_eq!(EventType::ToolUse.as_str(), "tool_use");
        assert_eq!(EventType::PolicyDecision.as_str(), "policy_decision");
        assert_eq!(EventType::AiEscalation.as_str(), "ai_escalation");
        assert_eq!(EventType::Error.as_str(), "error");
    }

    #[test]
    fn test_decision_as_str() {
        assert_eq!(Decision::Allow.as_str(), "allow");
        assert_eq!(Decision::Deny.as_str(), "deny");
        assert_eq!(Decision::Escalate.as_str(), "escalate");
    }

    #[test]
    fn test_event_type_serialize() {
        let json = serde_json::to_string(&EventType::ToolUse).unwrap();
        assert_eq!(json, "\"tool_use\"");

        let parsed: EventType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, EventType::ToolUse);
    }

    #[test]
    fn test_decision_serialize() {
        let json = serde_json::to_string(&Decision::Allow).unwrap();
        assert_eq!(json, "\"allow\"");

        let parsed: Decision = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, Decision::Allow);
    }

    #[test]
    fn test_audit_event_builder_minimal() {
        let session_id = Uuid::new_v4();
        let event = AuditEvent::builder(session_id, EventType::SessionStart).build();

        assert_eq!(event.session_id, session_id);
        assert_eq!(event.event_type, EventType::SessionStart);
        assert!(event.tool_name.is_none());
        assert!(event.tool_input.is_none());
        assert!(event.decision.is_none());
        assert!(event.reason.is_none());
    }

    #[test]
    fn test_audit_event_builder_full() {
        let session_id = Uuid::new_v4();
        let timestamp = Utc::now();
        let input = serde_json::json!({"command": "ls -la"});

        let event = AuditEvent::builder(session_id, EventType::ToolUse)
            .timestamp(timestamp)
            .tool_name("Bash")
            .tool_input(input.clone())
            .decision(Decision::Allow)
            .reason("Command is safe")
            .build();

        assert_eq!(event.session_id, session_id);
        assert_eq!(event.timestamp, timestamp);
        assert_eq!(event.event_type, EventType::ToolUse);
        assert_eq!(event.tool_name.as_deref(), Some("Bash"));
        assert_eq!(event.tool_input, Some(input));
        assert_eq!(event.decision, Some(Decision::Allow));
        assert_eq!(event.reason.as_deref(), Some("Command is safe"));
    }

    #[test]
    fn test_audit_event_serialize() {
        let session_id = Uuid::new_v4();
        let event = AuditEvent::builder(session_id, EventType::PolicyDecision)
            .decision(Decision::Deny)
            .reason("Destructive command")
            .build();

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"event_type\":\"policy_decision\""));
        assert!(json.contains("\"decision\":\"deny\""));

        let parsed: AuditEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.session_id, session_id);
        assert_eq!(parsed.event_type, EventType::PolicyDecision);
        assert_eq!(parsed.decision, Some(Decision::Deny));
    }

    #[test]
    fn test_audit_session_new() {
        let session = AuditSession::new("Fix the auth bug");

        assert!(!session.id.is_nil());
        assert_eq!(session.task, "Fix the auth bug");
        assert!(session.ended_at.is_none());
        assert!(session.result.is_none());
    }

    #[test]
    fn test_audit_session_with_id() {
        let id = Uuid::new_v4();
        let session = AuditSession::with_id(id, "Refactor database");

        assert_eq!(session.id, id);
        assert_eq!(session.task, "Refactor database");
    }

    #[test]
    fn test_audit_session_end() {
        let mut session = AuditSession::new("Test task");
        assert!(session.ended_at.is_none());

        session.end("Success");
        assert!(session.ended_at.is_some());
        assert_eq!(session.result.as_deref(), Some("Success"));
    }

    #[test]
    fn test_audit_session_serialize() {
        let mut session = AuditSession::new("Build feature");
        session.end("Completed");

        let json = serde_json::to_string(&session).unwrap();
        assert!(json.contains("\"task\":\"Build feature\""));
        assert!(json.contains("\"result\":\"Completed\""));

        let parsed: AuditSession = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.task, "Build feature");
        assert_eq!(parsed.result, Some("Completed".to_string()));
    }

    #[test]
    fn test_session_metrics_new() {
        let session_id = Uuid::new_v4();
        let metrics = SessionMetrics::new(session_id);

        assert_eq!(metrics.session_id, session_id);
        assert_eq!(metrics.input_tokens, 0);
        assert_eq!(metrics.output_tokens, 0);
        assert_eq!(metrics.api_calls, 0);
        assert_eq!(metrics.cache_hits, 0);
        assert_eq!(metrics.estimated_cost_cents, 0);
    }

    #[test]
    fn test_session_metrics_add_tokens() {
        let mut metrics = SessionMetrics::new(Uuid::new_v4());

        metrics.add_tokens(1000, 500);
        assert_eq!(metrics.input_tokens, 1000);
        assert_eq!(metrics.output_tokens, 500);

        metrics.add_tokens(500, 250);
        assert_eq!(metrics.input_tokens, 1500);
        assert_eq!(metrics.output_tokens, 750);
    }

    #[test]
    fn test_session_metrics_record_calls() {
        let mut metrics = SessionMetrics::new(Uuid::new_v4());

        metrics.record_api_call();
        metrics.record_api_call();
        metrics.record_cache_hit();

        assert_eq!(metrics.api_calls, 2);
        assert_eq!(metrics.cache_hits, 1);
    }

    #[test]
    fn test_session_metrics_calculate_cost() {
        let mut metrics = SessionMetrics::new(Uuid::new_v4());
        metrics.add_tokens(10_000, 1_000);
        metrics.calculate_cost();

        // 10K input = $0.03 = 3 cents
        // 1K output = $0.015 = 1.5 cents
        // Total = 4.5 cents, ceil = 5 cents
        assert_eq!(metrics.estimated_cost_cents, 5);
    }

    #[test]
    fn test_session_metrics_serialize() {
        let mut metrics = SessionMetrics::new(Uuid::new_v4());
        metrics.add_tokens(5000, 2000);
        metrics.record_api_call();

        let json = serde_json::to_string(&metrics).unwrap();
        assert!(json.contains("\"input_tokens\":5000"));
        assert!(json.contains("\"output_tokens\":2000"));
        assert!(json.contains("\"api_calls\":1"));

        let parsed: SessionMetrics = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.input_tokens, 5000);
        assert_eq!(parsed.output_tokens, 2000);
    }
}
