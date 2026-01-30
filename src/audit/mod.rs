//! Audit logging module for supervisor decisions.

mod error;
mod schema;
mod types;

pub use error::AuditError;
pub use schema::{SCHEMA, SCHEMA_VERSION};
pub use types::{AuditEvent, AuditSession, Decision, EventType, SessionMetrics};
