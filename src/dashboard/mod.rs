//! Web dashboard module for monitoring and controlling the supervisor.

mod error;
mod state;

pub use error::DashboardError;
pub use state::{
    create_dashboard_channels, DashboardCommand, DashboardEvent, DashboardHandles, DashboardState,
    SupervisorStatus,
};
