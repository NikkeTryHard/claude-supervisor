//! Dashboard HTTP server with axum router and graceful shutdown.

use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use super::handlers::{
    get_events_sse, get_metrics, get_status, post_continue, post_kill, post_stop, AppState,
};
use super::state::DashboardState;
use crate::audit::AuditLog;

/// Default port for the dashboard server.
pub const DEFAULT_PORT: u16 = 3000;

/// Configuration for the dashboard server.
#[derive(Debug, Clone)]
pub struct DashboardConfig {
    /// Port to listen on.
    pub port: u16,
    /// Host address to bind to.
    pub host: String,
    /// Whether to enable permissive CORS.
    pub cors_permissive: bool,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            host: "127.0.0.1".to_string(),
            cors_permissive: true,
        }
    }
}

/// Dashboard HTTP server for monitoring and controlling the supervisor.
pub struct DashboardServer {
    /// Server configuration.
    config: DashboardConfig,
    /// Application state shared across handlers.
    state: AppState,
}

impl DashboardServer {
    /// Create a new dashboard server with default configuration.
    #[must_use]
    pub fn new(dashboard: DashboardState, audit: Option<Arc<AuditLog>>) -> Self {
        let dashboard_arc = Arc::new(dashboard);
        let state = match audit {
            Some(audit) => AppState::with_audit(dashboard_arc, audit),
            None => AppState::new(dashboard_arc),
        };

        Self {
            config: DashboardConfig::default(),
            state,
        }
    }

    /// Set the server configuration (builder pattern).
    #[must_use]
    pub fn with_config(mut self, config: DashboardConfig) -> Self {
        self.config = config;
        self
    }

    /// Get the configured address as a string.
    #[must_use]
    pub fn address(&self) -> String {
        format!("{}:{}", self.config.host, self.config.port)
    }

    /// Build the axum router with all routes and middleware.
    pub fn build_router(&self) -> Router {
        let router = Router::new()
            .route("/api/status", get(get_status))
            .route("/api/events", get(get_events_sse))
            .route("/api/metrics", get(get_metrics))
            .route("/api/stop", post(post_stop))
            .route("/api/continue", post(post_continue))
            .route("/api/kill", post(post_kill))
            .with_state(self.state.clone())
            .layer(TraceLayer::new_for_http());

        if self.config.cors_permissive {
            router.layer(CorsLayer::permissive())
        } else {
            router
        }
    }

    /// Run the server, binding to the configured address.
    ///
    /// The server will run until the cancellation token is triggered,
    /// at which point it will perform a graceful shutdown.
    ///
    /// # Errors
    ///
    /// Returns an error if the server fails to bind or serve.
    pub async fn run(self) -> std::io::Result<()> {
        let addr = self.address();
        let cancel = self.state.dashboard.cancel.clone();
        let app = self.build_router();

        tracing::info!(address = %addr, "Starting dashboard server");

        let listener = TcpListener::bind(&addr).await?;

        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                cancel.cancelled().await;
                tracing::info!("Dashboard server shutting down gracefully");
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::create_dashboard_channels;

    #[test]
    fn test_dashboard_config_default() {
        let config = DashboardConfig::default();

        assert_eq!(config.port, DEFAULT_PORT);
        assert_eq!(config.port, 3000);
        assert_eq!(config.host, "127.0.0.1");
        assert!(config.cors_permissive);
    }

    #[test]
    fn test_dashboard_server_address() {
        let (dashboard_state, _handles) = create_dashboard_channels();
        let server = DashboardServer::new(dashboard_state, None);

        assert_eq!(server.address(), "127.0.0.1:3000");
    }

    #[test]
    fn test_dashboard_server_with_config() {
        let (dashboard_state, _handles) = create_dashboard_channels();
        let server = DashboardServer::new(dashboard_state, None);

        let custom_config = DashboardConfig {
            port: 8080,
            host: "0.0.0.0".to_string(),
            cors_permissive: false,
        };

        let server = server.with_config(custom_config);

        assert_eq!(server.address(), "0.0.0.0:8080");
        assert_eq!(server.config.port, 8080);
        assert_eq!(server.config.host, "0.0.0.0");
        assert!(!server.config.cors_permissive);
    }

    #[test]
    fn test_build_router() {
        let (dashboard_state, _handles) = create_dashboard_channels();
        let server = DashboardServer::new(dashboard_state, None);

        // Just verify the router builds without panicking
        let _router = server.build_router();
    }

    #[test]
    fn test_build_router_without_cors() {
        let (dashboard_state, _handles) = create_dashboard_channels();
        let server = DashboardServer::new(dashboard_state, None).with_config(DashboardConfig {
            port: 3000,
            host: "127.0.0.1".to_string(),
            cors_permissive: false,
        });

        // Verify the router builds without CORS layer
        let _router = server.build_router();
    }

    #[tokio::test]
    async fn test_server_with_audit() {
        let (dashboard_state, _handles) = create_dashboard_channels();
        let temp_dir = tempfile::tempdir().unwrap();
        let audit_path = temp_dir.path().join("test.db");
        let audit = AuditLog::open(&audit_path).await.unwrap();

        let server = DashboardServer::new(dashboard_state, Some(Arc::new(audit)));

        assert!(server.state.audit.is_some());
        assert_eq!(server.address(), "127.0.0.1:3000");
    }
}
