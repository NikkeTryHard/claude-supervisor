//! Web dashboard module for monitoring and controlling the supervisor.

mod api;
mod error;
mod handlers;
mod state;

pub use api::{
    CommandResponse, EventsQuery, MetricsResponse, SessionMetricsResponse, StatusResponse,
};
pub use error::DashboardError;
pub use handlers::{
    get_events_sse, get_metrics, get_status, post_continue, post_kill, post_stop, AppState,
};
pub use state::{
    create_dashboard_channels, DashboardCommand, DashboardEvent, DashboardHandles, DashboardState,
    SupervisorStatus,
};
