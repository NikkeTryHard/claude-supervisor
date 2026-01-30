//! Audit logging module for supervisor decisions.

mod error;
mod logger;
mod schema;
mod types;

pub use error::AuditError;
pub use logger::{default_audit_path, AuditLog};
pub use schema::{SCHEMA, SCHEMA_VERSION};
pub use types::{AuditEvent, AuditSession, Decision, EventType, SessionMetrics};
