//! API response types for the dashboard HTTP endpoints.

use serde::{Deserialize, Serialize};

use super::SupervisorStatus;
use crate::audit::SessionMetrics;

/// Response for GET /api/status endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    /// Whether a client is connected to the SSE stream.
    pub connected: bool,
    /// Current supervisor status.
    #[serde(flatten)]
    pub status: SupervisorStatus,
}

impl StatusResponse {
    /// Create a new status response.
    #[must_use]
    pub fn new(status: SupervisorStatus, connected: bool) -> Self {
        Self { connected, status }
    }
}

/// Response for command endpoints (POST /api/stop, /api/continue, /api/kill).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResponse {
    /// Whether the command was successful.
    pub success: bool,
    /// Message describing the result.
    pub message: String,
    /// Optional error details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl CommandResponse {
    /// Create a success response.
    #[must_use]
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            error: None,
        }
    }

    /// Create an error response.
    #[must_use]
    pub fn error(message: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            error: Some(error.into()),
        }
    }
}

/// Query parameters for GET /api/events endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct EventsQuery {
    /// Maximum number of events to return.
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Number of events to skip.
    #[serde(default)]
    pub offset: usize,
}

impl EventsQuery {
    /// Get the effective limit, capped at `MAX_EVENTS_LIMIT`.
    #[must_use]
    pub fn effective_limit(&self) -> usize {
        self.limit.min(MAX_EVENTS_LIMIT)
    }
}

impl Default for EventsQuery {
    fn default() -> Self {
        Self {
            limit: default_limit(),
            offset: 0,
        }
    }
}

/// Maximum allowed limit for pagination.
pub const MAX_EVENTS_LIMIT: usize = 1000;

const fn default_limit() -> usize {
    100
}

/// Response for GET /api/metrics endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsResponse {
    /// Total number of events recorded.
    pub total_events: u64,
    /// Number of allowed actions.
    pub allowed: u64,
    /// Number of denied actions.
    pub denied: u64,
    /// Session-specific metrics, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<SessionMetricsResponse>,
}

impl MetricsResponse {
    /// Create a new metrics response without session data.
    #[must_use]
    pub fn new(total_events: u64, allowed: u64, denied: u64) -> Self {
        Self {
            total_events,
            allowed,
            denied,
            session: None,
        }
    }

    /// Create a metrics response with session data.
    #[must_use]
    pub fn with_session(
        total_events: u64,
        allowed: u64,
        denied: u64,
        session: SessionMetricsResponse,
    ) -> Self {
        Self {
            total_events,
            allowed,
            denied,
            session: Some(session),
        }
    }
}

/// Session-specific metrics in the metrics response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetricsResponse {
    /// Session identifier.
    pub session_id: String,
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

impl From<SessionMetrics> for SessionMetricsResponse {
    fn from(metrics: SessionMetrics) -> Self {
        Self {
            session_id: metrics.session_id.to_string(),
            input_tokens: metrics.input_tokens,
            output_tokens: metrics.output_tokens,
            api_calls: metrics.api_calls,
            cache_hits: metrics.cache_hits,
            estimated_cost_cents: metrics.estimated_cost_cents,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::SupervisorStatus;

    #[test]
    fn test_command_response_success() {
        let response = CommandResponse::success("Command executed");

        assert!(response.success);
        assert_eq!(response.message, "Command executed");
        assert!(response.error.is_none());

        // Verify serialization omits null error
        let json = serde_json::to_string(&response).unwrap();
        assert!(!json.contains("error"));
    }

    #[test]
    fn test_command_response_error() {
        let response = CommandResponse::error("Command failed", "Channel closed");

        assert!(!response.success);
        assert_eq!(response.message, "Command failed");
        assert_eq!(response.error, Some("Channel closed".to_string()));

        // Verify error is included in serialization
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\":\"Channel closed\""));
    }

    #[test]
    fn test_events_query_defaults() {
        let query = EventsQuery::default();

        assert_eq!(query.limit, 100);
        assert_eq!(query.offset, 0);
    }

    #[test]
    fn test_events_query_deserialize_with_defaults() {
        let json = "{}";
        let query: EventsQuery = serde_json::from_str(json).unwrap();

        assert_eq!(query.limit, 100);
        assert_eq!(query.offset, 0);
    }

    #[test]
    fn test_events_query_deserialize_custom() {
        let json = r#"{"limit": 50, "offset": 10}"#;
        let query: EventsQuery = serde_json::from_str(json).unwrap();

        assert_eq!(query.limit, 50);
        assert_eq!(query.offset, 10);
    }

    #[test]
    fn test_status_response_serialization() {
        let status = SupervisorStatus {
            session_id: Some("test-123".to_string()),
            state: "running".to_string(),
            tool_calls: 5,
            approvals: 4,
            denials: 1,
            task: Some("Fix bug".to_string()),
        };
        let response = StatusResponse::new(status, true);

        let json = serde_json::to_string(&response).unwrap();

        // Verify connected is present
        assert!(json.contains("\"connected\":true"));
        // Verify status fields are flattened
        assert!(json.contains("\"session_id\":\"test-123\""));
        assert!(json.contains("\"state\":\"running\""));
        assert!(json.contains("\"tool_calls\":5"));
    }

    #[test]
    fn test_metrics_response_without_session() {
        let response = MetricsResponse::new(100, 80, 20);

        assert_eq!(response.total_events, 100);
        assert_eq!(response.allowed, 80);
        assert_eq!(response.denied, 20);
        assert!(response.session.is_none());

        let json = serde_json::to_string(&response).unwrap();
        assert!(!json.contains("session"));
    }

    #[test]
    fn test_metrics_response_with_session() {
        let session = SessionMetricsResponse {
            session_id: "abc-123".to_string(),
            input_tokens: 5000,
            output_tokens: 2000,
            api_calls: 10,
            cache_hits: 3,
            estimated_cost_cents: 5,
        };
        let response = MetricsResponse::with_session(100, 80, 20, session);

        assert!(response.session.is_some());
        let session = response.session.unwrap();
        assert_eq!(session.session_id, "abc-123");
        assert_eq!(session.input_tokens, 5000);
    }

    #[test]
    fn test_session_metrics_response_from() {
        use uuid::Uuid;

        let session_id = Uuid::new_v4();
        let mut metrics = SessionMetrics::new(session_id);
        metrics.add_tokens(1000, 500);
        metrics.record_api_call();
        metrics.record_cache_hit();

        let response: SessionMetricsResponse = metrics.into();

        assert_eq!(response.session_id, session_id.to_string());
        assert_eq!(response.input_tokens, 1000);
        assert_eq!(response.output_tokens, 500);
        assert_eq!(response.api_calls, 1);
        assert_eq!(response.cache_hits, 1);
    }
}
