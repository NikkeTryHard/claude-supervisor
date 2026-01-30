//! IPC server for the supervisor.
//!
//! This module provides the server-side IPC implementation for the supervisor
//! to receive escalation requests from hook binaries.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::watch;

use crate::ipc::{EscalationRequest, EscalationResponse, IpcError, DEFAULT_SOCKET_PATH};

/// IPC server for receiving escalation requests from hook binaries.
///
/// The server listens on a Unix domain socket and spawns a handler
/// for each incoming connection.
#[derive(Debug)]
pub struct IpcServer {
    socket_path: PathBuf,
}

impl IpcServer {
    /// Creates a new IPC server with a custom socket path.
    #[must_use]
    pub fn new<P: AsRef<Path>>(socket_path: P) -> Self {
        Self {
            socket_path: socket_path.as_ref().to_path_buf(),
        }
    }

    /// Creates a new IPC server with the default socket path.
    #[must_use]
    pub fn with_default_path() -> Self {
        Self::new(DEFAULT_SOCKET_PATH)
    }

    /// Returns the socket path.
    #[must_use]
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Starts the IPC server with the given request handler.
    ///
    /// The handler is called for each incoming escalation request and should
    /// return the appropriate response.
    ///
    /// # Errors
    ///
    /// Returns an error if the server fails to bind to the socket.
    pub fn start<F, Fut>(&self, handler: F) -> Result<ServerHandle, IpcError>
    where
        F: Fn(EscalationRequest) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = EscalationResponse> + Send,
    {
        // Remove existing socket file if it exists
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path)?;
        }

        // Bind to the socket
        let listener = UnixListener::bind(&self.socket_path)?;
        let socket_path = self.socket_path.clone();

        tracing::info!(path = %socket_path.display(), "IPC server started");

        // Create shutdown channel
        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);

        let handler = Arc::new(handler);

        // Spawn the accept loop
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;

                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            tracing::info!("IPC server shutting down");
                            break;
                        }
                    }

                    accept_result = listener.accept() => {
                        match accept_result {
                            Ok((stream, _addr)) => {
                                let handler = Arc::clone(&handler);
                                tokio::spawn(async move {
                                    if let Err(e) = handle_connection(stream, handler).await {
                                        tracing::warn!(error = %e, "Connection handler error");
                                    }
                                });
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "Failed to accept connection");
                            }
                        }
                    }
                }
            }
        });

        Ok(ServerHandle {
            socket_path: self.socket_path.clone(),
            shutdown_tx,
        })
    }
}

/// Handle for a running IPC server.
///
/// When dropped, the socket file is cleaned up.
#[derive(Debug)]
pub struct ServerHandle {
    socket_path: PathBuf,
    shutdown_tx: watch::Sender<bool>,
}

impl ServerHandle {
    /// Signals the server to shut down.
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
    }

    /// Returns the socket path.
    #[must_use]
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        // Signal shutdown
        let _ = self.shutdown_tx.send(true);

        // Clean up socket file
        if self.socket_path.exists() {
            if let Err(e) = std::fs::remove_file(&self.socket_path) {
                tracing::warn!(
                    path = %self.socket_path.display(),
                    error = %e,
                    "Failed to remove socket file"
                );
            }
        }
    }
}

/// Handles a single connection from a hook binary.
async fn handle_connection<F, Fut>(
    stream: tokio::net::UnixStream,
    handler: Arc<F>,
) -> Result<(), IpcError>
where
    F: Fn(EscalationRequest) -> Fut + Send + Sync,
    Fut: Future<Output = EscalationResponse> + Send,
{
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    // Read the request
    let bytes_read = reader.read_line(&mut line).await?;
    if bytes_read == 0 {
        return Ok(());
    }

    // Parse the request
    let request: EscalationRequest = serde_json::from_str(line.trim())?;

    tracing::debug!(
        session_id = %request.session_id,
        tool_name = %request.tool_name,
        reason = %request.reason,
        "Received escalation request"
    );

    // Call the handler
    let response = handler(request).await;

    // Send the response
    let mut response_json = serde_json::to_string(&response)?;
    response_json.push('\n');
    writer.write_all(response_json.as_bytes()).await?;
    writer.flush().await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;

    #[test]
    fn server_new_uses_custom_path() {
        let server = IpcServer::new("/custom/path.sock");
        assert_eq!(server.socket_path(), Path::new("/custom/path.sock"));
    }

    #[test]
    fn server_with_default_path_uses_default() {
        let server = IpcServer::with_default_path();
        assert_eq!(server.socket_path(), Path::new(DEFAULT_SOCKET_PATH));
    }

    #[tokio::test]
    async fn server_client_integration() {
        use crate::ipc::IpcClient;

        // Create a temporary socket path
        let temp_dir = std::env::temp_dir();
        let socket_path = temp_dir.join(format!("test-{}.sock", std::process::id()));

        // Start the server
        let server = IpcServer::new(&socket_path);
        let handle = server
            .start(|req| async move {
                if req.tool_name == "Bash" {
                    EscalationResponse::Deny {
                        reason: "Bash not allowed".to_string(),
                    }
                } else {
                    EscalationResponse::Allow
                }
            })
            .expect("Failed to start server");

        // Give the server time to start
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Create a client
        let client = IpcClient::with_path(&socket_path);
        assert!(client.is_supervisor_running());

        // Send an escalation request that should be denied
        let request = EscalationRequest {
            session_id: "test-session".to_string(),
            tool_name: "Bash".to_string(),
            tool_input: json!({"command": "ls"}),
            reason: "Test escalation".to_string(),
        };

        let response = client.escalate(&request).await.expect("Escalation failed");
        assert!(matches!(response, EscalationResponse::Deny { .. }));

        // Send an escalation request that should be allowed
        let request = EscalationRequest {
            session_id: "test-session".to_string(),
            tool_name: "Read".to_string(),
            tool_input: json!({"path": "/tmp/test"}),
            reason: "Test escalation".to_string(),
        };

        let response = client.escalate(&request).await.expect("Escalation failed");
        assert!(matches!(response, EscalationResponse::Allow));

        // Shutdown the server
        handle.shutdown();

        // Give the server time to shut down and clean up
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    #[tokio::test]
    async fn server_handle_drop_cleans_up_socket() {
        let temp_dir = std::env::temp_dir();
        let socket_path = temp_dir.join(format!("test-drop-{}.sock", std::process::id()));

        {
            let server = IpcServer::new(&socket_path);
            let _handle = server
                .start(|_| async { EscalationResponse::Allow })
                .expect("Failed to start server");

            // Socket should exist while handle is alive
            assert!(socket_path.exists());
        }

        // Give time for cleanup
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Socket should be cleaned up after handle is dropped
        assert!(!socket_path.exists());
    }
}
