//! IPC client for hook binaries.
//!
//! This module provides the client-side IPC implementation for hook binaries
//! to communicate with the supervisor.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::ipc::{EscalationRequest, EscalationResponse, IpcError, DEFAULT_SOCKET_PATH};

/// Default timeout for IPC operations (4 seconds).
///
/// This is set below Claude Code's 5-second hook timeout to ensure
/// the hook can respond even if the supervisor times out.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(4);

/// IPC client for hook binaries to communicate with the supervisor.
///
/// The client connects to the supervisor's Unix domain socket and sends
/// escalation requests, receiving decisions in response.
#[derive(Debug, Clone)]
pub struct IpcClient {
    socket_path: PathBuf,
    timeout: Duration,
}

impl IpcClient {
    /// Creates a new IPC client with the default socket path.
    #[must_use]
    pub fn new() -> Self {
        Self {
            socket_path: PathBuf::from(DEFAULT_SOCKET_PATH),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Creates a new IPC client with a custom socket path.
    #[must_use]
    pub fn with_path<P: AsRef<Path>>(path: P) -> Self {
        Self {
            socket_path: path.as_ref().to_path_buf(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Sets the timeout duration for IPC operations.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Returns the socket path.
    #[must_use]
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Returns the timeout duration.
    #[must_use]
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Checks if the supervisor is running by verifying the socket file exists.
    #[must_use]
    pub fn is_supervisor_running(&self) -> bool {
        self.socket_path.exists()
    }

    /// Sends an escalation request to the supervisor and waits for a response.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The supervisor is not running ([`IpcError::SupervisorNotRunning`])
    /// - The connection fails ([`IpcError::ConnectionFailed`])
    /// - The operation times out ([`IpcError::Timeout`])
    /// - Message serialization fails ([`IpcError::SerializationError`])
    /// - The response is invalid ([`IpcError::InvalidResponse`])
    pub async fn escalate(
        &self,
        request: &EscalationRequest,
    ) -> Result<EscalationResponse, IpcError> {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        use tokio::net::UnixStream;

        if !self.is_supervisor_running() {
            return Err(IpcError::SupervisorNotRunning);
        }

        // Safe: timeout values are never going to exceed u64::MAX milliseconds
        #[allow(clippy::cast_possible_truncation)]
        let timeout_ms = self.timeout.as_millis() as u64;

        let result = tokio::time::timeout(self.timeout, async {
            // Connect to the supervisor
            let stream = UnixStream::connect(&self.socket_path).await?;
            let (reader, mut writer) = stream.into_split();

            // Serialize and send the request
            let mut request_json = serde_json::to_string(request)?;
            request_json.push('\n');
            writer.write_all(request_json.as_bytes()).await?;
            writer.flush().await?;

            // Read the response
            let mut reader = BufReader::new(reader);
            let mut response_line = String::new();
            let bytes_read = reader.read_line(&mut response_line).await?;

            if bytes_read == 0 {
                return Err(IpcError::InvalidResponse);
            }

            // Parse the response
            let response: EscalationResponse = serde_json::from_str(response_line.trim())?;
            Ok(response)
        })
        .await;

        match result {
            Ok(inner) => inner,
            Err(_) => Err(IpcError::Timeout(timeout_ms)),
        }
    }

    /// Sends a Stop escalation request to the supervisor and waits for a response.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The supervisor is not running ([`IpcError::SupervisorNotRunning`])
    /// - The connection fails ([`IpcError::ConnectionFailed`])
    /// - The operation times out ([`IpcError::Timeout`])
    /// - Message serialization fails ([`IpcError::SerializationError`])
    /// - The response is invalid ([`IpcError::InvalidResponse`])
    pub async fn escalate_stop(
        &self,
        request: &crate::ipc::StopEscalationRequest,
    ) -> Result<crate::ipc::StopEscalationResponse, IpcError> {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        use tokio::net::UnixStream;

        if !self.is_supervisor_running() {
            return Err(IpcError::SupervisorNotRunning);
        }

        #[allow(clippy::cast_possible_truncation)]
        let timeout_ms = self.timeout.as_millis() as u64;

        let result = tokio::time::timeout(self.timeout, async {
            let stream = UnixStream::connect(&self.socket_path).await?;
            let (reader, mut writer) = stream.into_split();

            // Wrap request with type tag for server routing
            let wrapper = serde_json::json!({
                "type": "stop",
                "payload": request
            });
            let mut request_json = serde_json::to_string(&wrapper)?;
            request_json.push('\n');
            writer.write_all(request_json.as_bytes()).await?;
            writer.flush().await?;

            let mut reader = BufReader::new(reader);
            let mut response_line = String::new();
            let bytes_read = reader.read_line(&mut response_line).await?;

            if bytes_read == 0 {
                return Err(IpcError::InvalidResponse);
            }

            let response: crate::ipc::StopEscalationResponse =
                serde_json::from_str(response_line.trim())?;
            Ok(response)
        })
        .await;

        match result {
            Ok(inner) => inner,
            Err(_) => Err(IpcError::Timeout(timeout_ms)),
        }
    }
}

impl Default for IpcClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_new_uses_default_path() {
        let client = IpcClient::new();
        assert_eq!(client.socket_path(), Path::new(DEFAULT_SOCKET_PATH));
    }

    #[test]
    fn client_with_path_uses_custom_path() {
        let client = IpcClient::with_path("/custom/path.sock");
        assert_eq!(client.socket_path(), Path::new("/custom/path.sock"));
    }

    #[test]
    fn client_with_timeout_sets_timeout() {
        let client = IpcClient::new().with_timeout(Duration::from_secs(10));
        assert_eq!(client.timeout(), Duration::from_secs(10));
    }

    #[test]
    fn client_default_timeout_is_4_seconds() {
        let client = IpcClient::new();
        assert_eq!(client.timeout(), Duration::from_secs(4));
    }

    #[test]
    fn client_is_supervisor_running_returns_false_for_nonexistent_socket() {
        let client = IpcClient::with_path("/nonexistent/socket.sock");
        assert!(!client.is_supervisor_running());
    }

    #[test]
    fn client_default_impl() {
        let client = IpcClient::default();
        assert_eq!(client.socket_path(), Path::new(DEFAULT_SOCKET_PATH));
    }

    #[tokio::test]
    async fn client_escalate_stop_returns_error_when_supervisor_not_running() {
        let client = IpcClient::with_path("/nonexistent/socket.sock");
        let request = crate::ipc::StopEscalationRequest {
            session_id: "test".to_string(),
            final_message: "Done".to_string(),
            transcript_path: Some("/path/to/transcript.jsonl".to_string()),
            task: Some("Test task".to_string()),
            iteration: 1,
        };
        let result = client.escalate_stop(&request).await;
        assert!(matches!(result, Err(IpcError::SupervisorNotRunning)));
    }
}
