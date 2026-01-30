//! Dashboard state types and channel management.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, watch};
use tokio_util::sync::CancellationToken;

/// Commands that can be sent from the dashboard to the supervisor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DashboardCommand {
    /// Stop the current session gracefully.
    Stop,
    /// Continue execution (approve pending action).
    Continue,
    /// Force kill the Claude process.
    ForceKill,
}

/// Current status of the supervisor session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupervisorStatus {
    /// Session identifier.
    pub session_id: Option<String>,
    /// Current state (e.g., "running", "waiting", "stopped").
    pub state: String,
    /// Total number of tool calls.
    pub tool_calls: u64,
    /// Number of approved tool calls.
    pub approvals: u64,
    /// Number of denied tool calls.
    pub denials: u64,
    /// Current task description.
    pub task: Option<String>,
}

impl Default for SupervisorStatus {
    fn default() -> Self {
        Self {
            session_id: None,
            state: "idle".to_string(),
            tool_calls: 0,
            approvals: 0,
            denials: 0,
            task: None,
        }
    }
}

/// Event sent to dashboard clients via SSE.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardEvent {
    /// Type of event (e.g., `tool_call`, `approval`, `denial`, `output`).
    pub event_type: String,
    /// When the event occurred.
    pub timestamp: DateTime<Utc>,
    /// Event payload as JSON value.
    pub data: serde_json::Value,
}

impl DashboardEvent {
    /// Create a new dashboard event.
    #[must_use]
    pub fn new(event_type: impl Into<String>, data: serde_json::Value) -> Self {
        Self {
            event_type: event_type.into(),
            timestamp: Utc::now(),
            data,
        }
    }
}

/// State held by the dashboard server.
pub struct DashboardState {
    /// Receiver for supervisor status updates.
    pub status_rx: watch::Receiver<SupervisorStatus>,
    /// Sender for commands to the supervisor.
    pub command_tx: mpsc::Sender<DashboardCommand>,
    /// Sender for broadcasting events to SSE clients.
    pub event_tx: broadcast::Sender<DashboardEvent>,
    /// Cancellation token for graceful shutdown.
    pub cancel: CancellationToken,
}

/// Handles held by the supervisor for dashboard communication.
pub struct DashboardHandles {
    /// Sender for status updates to the dashboard.
    pub status_tx: watch::Sender<SupervisorStatus>,
    /// Receiver for commands from the dashboard.
    pub command_rx: mpsc::Receiver<DashboardCommand>,
    /// Sender for broadcasting events (supervisor can also send events).
    pub event_tx: broadcast::Sender<DashboardEvent>,
    /// Cancellation token for graceful shutdown.
    pub cancel: CancellationToken,
}

/// Default capacity for the event broadcast channel.
pub const DEFAULT_EVENT_CHANNEL_CAPACITY: usize = 256;

/// Create paired channels for dashboard-supervisor communication.
///
/// Returns a tuple of `(DashboardState, DashboardHandles)` that should be
/// used by the dashboard server and supervisor respectively.
#[must_use]
pub fn create_dashboard_channels() -> (DashboardState, DashboardHandles) {
    let (status_tx, status_rx) = watch::channel(SupervisorStatus::default());
    let (command_tx, command_rx) = mpsc::channel(32);
    let (event_tx, _) = broadcast::channel(DEFAULT_EVENT_CHANNEL_CAPACITY);
    let cancel = CancellationToken::new();

    let dashboard_state = DashboardState {
        status_rx,
        command_tx,
        event_tx: event_tx.clone(),
        cancel: cancel.clone(),
    };

    let dashboard_handles = DashboardHandles {
        status_tx,
        command_rx,
        event_tx,
        cancel,
    };

    (dashboard_state, dashboard_handles)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dashboard_command_variants() {
        let stop = DashboardCommand::Stop;
        let cont = DashboardCommand::Continue;
        let kill = DashboardCommand::ForceKill;

        assert_eq!(stop, DashboardCommand::Stop);
        assert_eq!(cont, DashboardCommand::Continue);
        assert_eq!(kill, DashboardCommand::ForceKill);

        // Test serialization
        let json = serde_json::to_string(&stop).unwrap();
        assert_eq!(json, "\"Stop\"");
    }

    #[test]
    fn test_supervisor_status_default() {
        let status = SupervisorStatus::default();

        assert!(status.session_id.is_none());
        assert_eq!(status.state, "idle");
        assert_eq!(status.tool_calls, 0);
        assert_eq!(status.approvals, 0);
        assert_eq!(status.denials, 0);
        assert!(status.task.is_none());
    }

    #[test]
    fn test_dashboard_event_creation() {
        let event = DashboardEvent::new("tool_call", serde_json::json!({"tool": "Read"}));

        assert_eq!(event.event_type, "tool_call");
        assert!(event.timestamp <= Utc::now());
        assert_eq!(event.data["tool"], "Read");
    }

    #[test]
    fn test_create_dashboard_channels() {
        let (state, handles) = create_dashboard_channels();

        // Verify initial status
        let status = state.status_rx.borrow();
        assert_eq!(status.state, "idle");
        drop(status);

        // Verify channels are connected
        assert!(!state.cancel.is_cancelled());
        assert!(!handles.cancel.is_cancelled());
    }

    #[tokio::test]
    async fn test_event_broadcast() {
        let (state, handles) = create_dashboard_channels();

        // Subscribe before sending
        let mut rx = state.event_tx.subscribe();

        // Send event from handles (supervisor side)
        let event = DashboardEvent::new("test", serde_json::json!({"msg": "hello"}));
        handles.event_tx.send(event.clone()).unwrap();

        // Receive on subscriber
        let received = rx.recv().await.unwrap();
        assert_eq!(received.event_type, "test");
        assert_eq!(received.data["msg"], "hello");
    }

    #[tokio::test]
    async fn test_status_updates() {
        let (state, handles) = create_dashboard_channels();

        // Update status from supervisor side
        handles
            .status_tx
            .send(SupervisorStatus {
                session_id: Some("test-123".to_string()),
                state: "running".to_string(),
                tool_calls: 5,
                approvals: 4,
                denials: 1,
                task: Some("Fix bug".to_string()),
            })
            .unwrap();

        // Read from dashboard side
        let status = state.status_rx.borrow();
        assert_eq!(status.session_id, Some("test-123".to_string()));
        assert_eq!(status.state, "running");
        assert_eq!(status.tool_calls, 5);
    }

    #[tokio::test]
    async fn test_command_channel() {
        let (state, mut handles) = create_dashboard_channels();

        // Send command from dashboard
        state.command_tx.send(DashboardCommand::Stop).await.unwrap();

        // Receive on supervisor side
        let cmd = handles.command_rx.recv().await.unwrap();
        assert_eq!(cmd, DashboardCommand::Stop);
    }
}
