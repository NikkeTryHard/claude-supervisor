//! Watcher error types.

use std::path::PathBuf;

/// Errors that can occur during file watching.
#[derive(thiserror::Error, Debug)]
pub enum WatcherError {
    /// Watched file was deleted.
    #[error("Watched file deleted: {0}")]
    FileDeleted(PathBuf),

    /// Permission denied accessing file.
    #[error("Permission denied: {0}")]
    PermissionDenied(PathBuf),

    /// File was truncated (position beyond EOF).
    #[error("File truncated, state reset required")]
    FileTruncated,

    /// Notify watcher error.
    #[error("File watcher error: {0}")]
    Notify(#[from] notify::Error),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Channel send error.
    #[error("Channel closed")]
    ChannelClosed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_deleted_display() {
        let err = WatcherError::FileDeleted(PathBuf::from("/tmp/test.jsonl"));
        assert_eq!(err.to_string(), "Watched file deleted: /tmp/test.jsonl");
    }

    #[test]
    fn test_permission_denied_display() {
        let err = WatcherError::PermissionDenied(PathBuf::from("/root/secret.jsonl"));
        assert_eq!(err.to_string(), "Permission denied: /root/secret.jsonl");
    }

    #[test]
    fn test_file_truncated_display() {
        let err = WatcherError::FileTruncated;
        assert_eq!(err.to_string(), "File truncated, state reset required");
    }

    #[test]
    fn test_channel_closed_display() {
        let err = WatcherError::ChannelClosed;
        assert_eq!(err.to_string(), "Channel closed");
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let watcher_err: WatcherError = io_err.into();
        assert!(matches!(watcher_err, WatcherError::Io(_)));
        assert!(watcher_err.to_string().contains("I/O error"));
    }

    #[test]
    fn test_from_notify_error() {
        let notify_err = notify::Error::generic("test error");
        let watcher_err: WatcherError = notify_err.into();
        assert!(matches!(watcher_err, WatcherError::Notify(_)));
        assert!(watcher_err.to_string().contains("File watcher error"));
    }
}
