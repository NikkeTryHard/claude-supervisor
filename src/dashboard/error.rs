//! Dashboard error types.

/// Errors that can occur during dashboard operations.
#[derive(thiserror::Error, Debug)]
pub enum DashboardError {
    /// Failed to bind to address.
    #[error("Failed to bind to {address}: {source}")]
    BindError {
        address: String,
        #[source]
        source: std::io::Error,
    },

    /// Server error.
    #[error("Server error: {0}")]
    ServerError(String),

    /// Channel closed.
    #[error("Command channel closed")]
    ChannelClosed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bind_error_display() {
        let io_error = std::io::Error::new(std::io::ErrorKind::AddrInUse, "address in use");
        let error = DashboardError::BindError {
            address: "127.0.0.1:8080".to_string(),
            source: io_error,
        };
        assert!(error
            .to_string()
            .contains("Failed to bind to 127.0.0.1:8080"));
        assert!(error.to_string().contains("address in use"));
    }

    #[test]
    fn test_channel_closed_display() {
        let error = DashboardError::ChannelClosed;
        assert_eq!(error.to_string(), "Command channel closed");
    }

    #[test]
    fn test_server_error_display() {
        let error = DashboardError::ServerError("connection reset".to_string());
        assert_eq!(error.to_string(), "Server error: connection reset");
    }
}
