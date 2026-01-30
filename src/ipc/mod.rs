//! IPC between supervisor and hook binaries.
//!
//! This module provides inter-process communication for the supervisor to receive
//! escalation requests from hook binaries and respond with decisions.
//!
//! # Architecture
//!
//! ```text
//! Hook Binary                    Supervisor
//!     |                              |
//!     |-- EscalationRequest -------->|
//!     |                              | (evaluate policy)
//!     |<-- EscalationResponse -------|
//!     |                              |
//! ```
//!
//! # Protocol
//!
//! Communication uses JSON-line format over Unix domain sockets:
//! - Client sends JSON + newline
//! - Server responds with JSON + newline
//!
//! # Example
//!
//! ```no_run
//! use claude_supervisor::ipc::{IpcClient, EscalationRequest};
//! use serde_json::json;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let client = IpcClient::new();
//!
//! if client.is_supervisor_running() {
//!     let request = EscalationRequest {
//!         session_id: "abc123".to_string(),
//!         tool_name: "Bash".to_string(),
//!         tool_input: json!({"command": "rm -rf /"}),
//!         reason: "Destructive command detected".to_string(),
//!     };
//!
//!     let response = client.escalate(&request).await?;
//!     println!("Supervisor decision: {:?}", response);
//! }
//! # Ok(())
//! # }
//! ```

pub mod client;
pub mod server;
pub mod types;

pub use client::IpcClient;
pub use server::{IpcServer, ServerHandle};
pub use types::{EscalationRequest, EscalationResponse, IpcError};

/// Default socket path for supervisor IPC.
pub const DEFAULT_SOCKET_PATH: &str = "/tmp/claude-supervisor.sock";
