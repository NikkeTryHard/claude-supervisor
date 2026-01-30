//! Incremental JSONL file tailer.
//!
//! Reads new entries from a JSONL file as they are appended.

use std::path::PathBuf;

use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncSeekExt, BufReader};

use super::error::WatcherError;
use super::jsonl::JournalEntry;

/// Incremental JSONL file reader that tracks read position.
///
/// Reads only new lines appended since the last read, making it suitable
/// for watching growing log files.
#[derive(Debug)]
pub struct JsonlTailer {
    /// Path to the JSONL file.
    path: PathBuf,
    /// Current byte offset in the file.
    offset: u64,
}

impl JsonlTailer {
    /// Create a new tailer for the given path.
    ///
    /// Starts at offset 0 (beginning of file).
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path, offset: 0 }
    }

    /// Create a new tailer starting at a specific offset.
    #[must_use]
    pub fn with_offset(path: PathBuf, offset: u64) -> Self {
        Self { path, offset }
    }

    /// Get the current byte offset.
    #[must_use]
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// Get the path being tailed.
    #[must_use]
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Read new entries since the last read.
    ///
    /// Returns entries parsed from new lines. Malformed lines are skipped
    /// with a warning logged.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file cannot be opened (file deleted, permission denied)
    /// - I/O errors occur during reading
    ///
    /// If the file is truncated (smaller than our offset), the offset is
    /// reset to 0 and reading starts from the beginning.
    pub async fn read_new_entries(&mut self) -> Result<Vec<JournalEntry>, WatcherError> {
        // Try to open the file
        let file = match File::open(&self.path).await {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(WatcherError::FileDeleted(self.path.clone()));
            }
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                return Err(WatcherError::PermissionDenied(self.path.clone()));
            }
            Err(e) => return Err(WatcherError::Io(e)),
        };

        // Get file metadata to check for truncation
        let metadata = file.metadata().await?;
        let file_len = metadata.len();

        // Detect truncation (file is now smaller than our offset)
        if file_len < self.offset {
            tracing::warn!(
                path = %self.path.display(),
                old_offset = self.offset,
                new_len = file_len,
                "File truncated, resetting offset to 0"
            );
            self.offset = 0;
        }

        // If file hasn't grown, no new entries
        if file_len == self.offset {
            return Ok(Vec::new());
        }

        // Seek to current offset and read new lines
        let mut file = file;
        file.seek(std::io::SeekFrom::Start(self.offset)).await?;

        let mut reader = BufReader::new(file);
        let mut entries = Vec::new();
        let mut line = String::new();

        loop {
            line.clear();
            let bytes_read = reader.read_line(&mut line).await?;

            if bytes_read == 0 {
                // EOF reached
                break;
            }

            self.offset += bytes_read as u64;

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            match serde_json::from_str::<JournalEntry>(trimmed) {
                Ok(entry) => entries.push(entry),
                Err(e) => {
                    tracing::warn!(
                        path = %self.path.display(),
                        line = %trimmed,
                        error = %e,
                        "Skipping malformed JSONL line"
                    );
                }
            }
        }

        Ok(entries)
    }

    /// Reset the offset to the beginning of the file.
    pub fn reset(&mut self) {
        self.offset = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_entry(uuid: &str) -> String {
        format!(
            r#"{{"type":"user","uuid":"{uuid}","parentUuid":null,"sessionId":"sess-1","timestamp":"2026-01-29T10:00:00Z","message":{{"role":"user","content":"Hello"}},"userType":"external","cwd":"/tmp","version":"2.1.25"}}"#
        )
    }

    #[tokio::test]
    async fn test_tailer_reads_initial_content() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "{}", create_test_entry("uuid-1")).unwrap();
        writeln!(file, "{}", create_test_entry("uuid-2")).unwrap();
        file.flush().unwrap();

        let mut tailer = JsonlTailer::new(file.path().to_path_buf());
        let entries = tailer.read_new_entries().await.unwrap();

        assert_eq!(entries.len(), 2);
        assert!(tailer.offset() > 0);
    }

    #[tokio::test]
    async fn test_tailer_reads_only_new_lines() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "{}", create_test_entry("uuid-1")).unwrap();
        file.flush().unwrap();

        let mut tailer = JsonlTailer::new(file.path().to_path_buf());

        // First read
        let entries1 = tailer.read_new_entries().await.unwrap();
        assert_eq!(entries1.len(), 1);
        let offset_after_first = tailer.offset();

        // No new content - should return empty
        let entries2 = tailer.read_new_entries().await.unwrap();
        assert_eq!(entries2.len(), 0);
        assert_eq!(tailer.offset(), offset_after_first);

        // Append new content
        writeln!(file, "{}", create_test_entry("uuid-2")).unwrap();
        writeln!(file, "{}", create_test_entry("uuid-3")).unwrap();
        file.flush().unwrap();

        // Should only get the new entries
        let entries3 = tailer.read_new_entries().await.unwrap();
        assert_eq!(entries3.len(), 2);
        assert!(tailer.offset() > offset_after_first);
    }

    #[tokio::test]
    async fn test_tailer_handles_truncation() {
        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();

        // Write initial content
        {
            let mut f = std::fs::File::create(&path).unwrap();
            writeln!(f, "{}", create_test_entry("uuid-1")).unwrap();
            writeln!(f, "{}", create_test_entry("uuid-2")).unwrap();
        }

        let mut tailer = JsonlTailer::new(path.clone());
        let entries1 = tailer.read_new_entries().await.unwrap();
        assert_eq!(entries1.len(), 2);
        let old_offset = tailer.offset();
        assert!(old_offset > 0);

        // Truncate file (simulate log rotation)
        {
            let mut f = std::fs::File::create(&path).unwrap();
            writeln!(f, "{}", create_test_entry("uuid-new")).unwrap();
        }

        // Tailer should detect truncation and reset
        let entries2 = tailer.read_new_entries().await.unwrap();
        assert_eq!(entries2.len(), 1);
        assert!(tailer.offset() < old_offset);
    }

    #[tokio::test]
    async fn test_tailer_handles_missing_file() {
        let path = PathBuf::from("/tmp/nonexistent-file-12345.jsonl");
        let mut tailer = JsonlTailer::new(path);

        let result = tailer.read_new_entries().await;
        assert!(matches!(result, Err(WatcherError::FileDeleted(_))));
    }

    #[tokio::test]
    async fn test_tailer_skips_malformed_lines() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "{}", create_test_entry("uuid-1")).unwrap();
        writeln!(file, "not valid json").unwrap();
        writeln!(file, "{}", create_test_entry("uuid-2")).unwrap();
        writeln!(file, "{{\"incomplete\": true").unwrap();
        writeln!(file, "{}", create_test_entry("uuid-3")).unwrap();
        file.flush().unwrap();

        let mut tailer = JsonlTailer::new(file.path().to_path_buf());
        let entries = tailer.read_new_entries().await.unwrap();

        // Should have 3 valid entries, skipping the 2 malformed lines
        assert_eq!(entries.len(), 3);
    }

    #[test]
    fn test_tailer_with_offset() {
        let tailer = JsonlTailer::with_offset(PathBuf::from("/tmp/test.jsonl"), 1024);
        assert_eq!(tailer.offset(), 1024);
    }

    #[test]
    fn test_tailer_reset() {
        let mut tailer = JsonlTailer::with_offset(PathBuf::from("/tmp/test.jsonl"), 1024);
        assert_eq!(tailer.offset(), 1024);
        tailer.reset();
        assert_eq!(tailer.offset(), 0);
    }
}
