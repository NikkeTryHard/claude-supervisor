//! Audit error types.

use std::path::PathBuf;

/// Errors that can occur during audit operations.
#[derive(thiserror::Error, Debug)]
pub enum AuditError {
    /// Failed to open or create database.
    #[error("Failed to open database at {path}: {source}")]
    DatabaseOpen {
        path: PathBuf,
        #[source]
        source: rusqlite::Error,
    },

    /// Failed to execute SQL.
    #[error("Database query failed: {0}")]
    Query(#[from] rusqlite::Error),

    /// Failed to serialize data to JSON.
    #[error("JSON serialization failed: {0}")]
    Serialize(#[from] serde_json::Error),

    /// Blocking task was cancelled.
    #[error("Blocking task cancelled")]
    TaskCancelled,

    /// Failed to create parent directory.
    #[error("Failed to create directory {path}: {source}")]
    CreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_open_display() {
        let err = AuditError::DatabaseOpen {
            path: PathBuf::from("/tmp/audit.db"),
            source: rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(1),
                Some("test".to_string()),
            ),
        };
        assert!(err.to_string().contains("Failed to open database"));
        assert!(err.to_string().contains("/tmp/audit.db"));
    }

    #[test]
    fn test_task_cancelled_display() {
        let err = AuditError::TaskCancelled;
        assert_eq!(err.to_string(), "Blocking task cancelled");
    }

    #[test]
    fn test_create_dir_display() {
        let err = AuditError::CreateDir {
            path: PathBuf::from("/root/audit"),
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
        };
        assert!(err.to_string().contains("Failed to create directory"));
        assert!(err.to_string().contains("/root/audit"));
    }
}
