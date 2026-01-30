//! HTTP handlers for the dashboard API.

use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use futures_util::stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use super::api::{CommandResponse, MetricsResponse, StatusResponse};
use super::state::{DashboardCommand, DashboardState};
use crate::audit::AuditLog;

/// Application state shared across all handlers.
#[derive(Clone)]
pub struct AppState {
    /// Dashboard state for status and commands.
    pub dashboard: Arc<DashboardState>,
    /// Optional audit log for metrics.
    pub audit: Option<Arc<AuditLog>>,
}

impl AppState {
    /// Create new app state with dashboard state only.
    #[must_use]
    pub fn new(dashboard: Arc<DashboardState>) -> Self {
        Self {
            dashboard,
            audit: None,
        }
    }

    /// Create new app state with audit log.
    #[must_use]
    pub fn with_audit(dashboard: Arc<DashboardState>, audit: Arc<AuditLog>) -> Self {
        Self {
            dashboard,
            audit: Some(audit),
        }
    }
}

/// GET /api/status - Get current supervisor status.
pub async fn get_status(State(state): State<AppState>) -> Json<StatusResponse> {
    let status = state.dashboard.status_rx.borrow().clone();
    // Check if there are any SSE subscribers
    let connected = state.dashboard.event_tx.receiver_count() > 0;

    Json(StatusResponse::new(status, connected))
}

/// GET /api/events - SSE stream of dashboard events.
pub async fn get_events_sse(
    State(state): State<AppState>,
) -> Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.dashboard.event_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| async move {
        match result {
            Ok(event) => {
                let data = serde_json::to_string(&event).ok()?;
                Some(Ok(Event::default().event(&event.event_type).data(data)))
            }
            Err(_) => None, // Skip lagged messages
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// GET /api/metrics - Get aggregated metrics.
pub async fn get_metrics(State(state): State<AppState>) -> Json<MetricsResponse> {
    let status = state.dashboard.status_rx.borrow();

    // Use status counters as base metrics
    let response = MetricsResponse::new(status.tool_calls, status.approvals, status.denials);

    // TODO: Integrate with audit log for historical metrics when session tracking is added

    Json(response)
}

/// POST /api/stop - Stop the current session gracefully.
pub async fn post_stop(State(state): State<AppState>) -> Json<CommandResponse> {
    match state
        .dashboard
        .command_tx
        .send(DashboardCommand::Stop)
        .await
    {
        Ok(()) => Json(CommandResponse::success("Stop command sent")),
        Err(e) => Json(CommandResponse::error(
            "Failed to send stop command",
            e.to_string(),
        )),
    }
}

/// POST /api/continue - Continue execution (approve pending action).
pub async fn post_continue(State(state): State<AppState>) -> Json<CommandResponse> {
    match state
        .dashboard
        .command_tx
        .send(DashboardCommand::Continue)
        .await
    {
        Ok(()) => Json(CommandResponse::success("Continue command sent")),
        Err(e) => Json(CommandResponse::error(
            "Failed to send continue command",
            e.to_string(),
        )),
    }
}

/// POST /api/kill - Force kill the Claude process.
pub async fn post_kill(State(state): State<AppState>) -> Json<CommandResponse> {
    match state
        .dashboard
        .command_tx
        .send(DashboardCommand::ForceKill)
        .await
    {
        Ok(()) => Json(CommandResponse::success("Kill command sent")),
        Err(e) => Json(CommandResponse::error(
            "Failed to send kill command",
            e.to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::{create_dashboard_channels, SupervisorStatus};

    #[tokio::test]
    async fn test_get_status() {
        let (dashboard_state, handles) = create_dashboard_channels();

        // Update status
        handles
            .status_tx
            .send(SupervisorStatus {
                session_id: Some("test-session".to_string()),
                state: "running".to_string(),
                tool_calls: 10,
                approvals: 8,
                denials: 2,
                task: Some("Test task".to_string()),
            })
            .unwrap();

        let state = AppState::new(Arc::new(dashboard_state));
        let Json(response) = get_status(State(state)).await;

        assert!(!response.connected); // No SSE subscribers
        assert_eq!(response.status.session_id, Some("test-session".to_string()));
        assert_eq!(response.status.state, "running");
        assert_eq!(response.status.tool_calls, 10);
    }

    #[tokio::test]
    async fn test_get_metrics_no_audit() {
        let (dashboard_state, handles) = create_dashboard_channels();

        handles
            .status_tx
            .send(SupervisorStatus {
                session_id: None,
                state: "running".to_string(),
                tool_calls: 100,
                approvals: 80,
                denials: 20,
                task: None,
            })
            .unwrap();

        let state = AppState::new(Arc::new(dashboard_state));
        let Json(response) = get_metrics(State(state)).await;

        assert_eq!(response.total_events, 100);
        assert_eq!(response.allowed, 80);
        assert_eq!(response.denied, 20);
        assert!(response.session.is_none());
    }

    #[tokio::test]
    async fn test_post_stop() {
        let (dashboard_state, mut handles) = create_dashboard_channels();
        let state = AppState::new(Arc::new(dashboard_state));

        let Json(response) = post_stop(State(state)).await;

        assert!(response.success);
        assert_eq!(response.message, "Stop command sent");

        // Verify command was received
        let cmd = handles.command_rx.recv().await.unwrap();
        assert_eq!(cmd, DashboardCommand::Stop);
    }

    #[tokio::test]
    async fn test_post_continue() {
        let (dashboard_state, mut handles) = create_dashboard_channels();
        let state = AppState::new(Arc::new(dashboard_state));

        let Json(response) = post_continue(State(state)).await;

        assert!(response.success);
        assert_eq!(response.message, "Continue command sent");

        // Verify command was received
        let cmd = handles.command_rx.recv().await.unwrap();
        assert_eq!(cmd, DashboardCommand::Continue);
    }

    #[tokio::test]
    async fn test_post_kill() {
        let (dashboard_state, mut handles) = create_dashboard_channels();
        let state = AppState::new(Arc::new(dashboard_state));

        let Json(response) = post_kill(State(state)).await;

        assert!(response.success);
        assert_eq!(response.message, "Kill command sent");

        // Verify command was received
        let cmd = handles.command_rx.recv().await.unwrap();
        assert_eq!(cmd, DashboardCommand::ForceKill);
    }

    #[tokio::test]
    async fn test_command_error_on_closed_channel() {
        let (dashboard_state, handles) = create_dashboard_channels();

        // Drop the receiver to close the channel
        drop(handles);

        let state = AppState::new(Arc::new(dashboard_state));
        let Json(response) = post_stop(State(state)).await;

        assert!(!response.success);
        assert_eq!(response.message, "Failed to send stop command");
        assert!(response.error.is_some());
    }

    #[tokio::test]
    async fn test_app_state_with_audit() {
        let (dashboard_state, _handles) = create_dashboard_channels();
        let temp_dir = tempfile::tempdir().unwrap();
        let audit_path = temp_dir.path().join("test.db");
        let audit = AuditLog::open(&audit_path).await.unwrap();

        let state = AppState::with_audit(Arc::new(dashboard_state), Arc::new(audit));

        assert!(state.audit.is_some());
    }
}
