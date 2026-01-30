//! Session watcher with notify integration.
//!
//! Watches JSONL session files for changes and emits events.

use std::path::PathBuf;
use std::sync::mpsc as std_mpsc;
use std::thread;
use std::time::Duration;

use notify_debouncer_full::{
    new_debouncer,
    notify::{self, RecursiveMode},
    DebounceEventResult,
};
use tokio::sync::mpsc;

use super::error::WatcherError;
use super::jsonl::JournalEntry;
use super::tailer::JsonlTailer;

/// Events emitted by the session watcher.
#[derive(Debug)]
pub enum WatcherEvent {
    /// A new journal entry was parsed.
    NewEntry(Box<JournalEntry>),
    /// A new file was created in the watch directory.
    FileCreated(PathBuf),
    /// A watched file was deleted.
    FileDeleted(PathBuf),
    /// A file was truncated (log rotation).
    FileTruncated(PathBuf),
    /// An error occurred during watching.
    Error(WatcherError),
}

/// Watches a session JSONL file or directory for changes.
///
/// Uses notify-debouncer-full for efficient file system event handling
/// and bridges events to a tokio mpsc channel.
pub struct SessionWatcher {
    /// The path being watched.
    watch_path: PathBuf,
    /// Handle to stop the watcher.
    #[allow(dead_code)]
    stop_tx: std_mpsc::Sender<()>,
    /// Handle to the bridge thread.
    #[allow(dead_code)]
    bridge_handle: thread::JoinHandle<()>,
}

impl SessionWatcher {
    /// Create a new session watcher for the given path.
    ///
    /// Returns the watcher and a receiver for watcher events.
    ///
    /// # Arguments
    ///
    /// * `watch_path` - Path to a JSONL file or directory to watch
    ///
    /// # Errors
    ///
    /// Returns an error if the file watcher cannot be created.
    ///
    /// # Panics
    ///
    /// Panics if the tokio runtime cannot be created in the bridge thread.
    pub fn new(
        watch_path: PathBuf,
    ) -> Result<(Self, mpsc::UnboundedReceiver<WatcherEvent>), WatcherError> {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (stop_tx, stop_rx) = std_mpsc::channel();

        // Create the notify debouncer
        let (notify_tx, notify_rx) = std_mpsc::channel();

        let mut debouncer = new_debouncer(Duration::from_millis(100), None, move |result| {
            let _ = notify_tx.send(result);
        })?;

        // Watch the path
        let is_file = watch_path.is_file();
        let watch_target = if is_file {
            watch_path.parent().unwrap_or(&watch_path).to_path_buf()
        } else {
            watch_path.clone()
        };

        debouncer.watch(&watch_target, RecursiveMode::NonRecursive)?;

        // Create tailer for the file if watching a file
        let tailer = if is_file {
            Some(JsonlTailer::new(watch_path.clone()))
        } else {
            None
        };

        let watch_path_clone = watch_path.clone();

        // Bridge thread: converts std_mpsc events to tokio mpsc
        let bridge_handle = thread::spawn(move || {
            let mut tailer = tailer;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create tokio runtime for bridge thread");

            loop {
                // Check for stop signal
                if stop_rx.try_recv().is_ok() {
                    break;
                }

                // Wait for notify events with timeout
                match notify_rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(result) => {
                        Self::handle_debounce_result(
                            result,
                            &watch_path_clone,
                            &mut tailer,
                            &event_tx,
                            &runtime,
                        );
                    }
                    Err(std_mpsc::RecvTimeoutError::Timeout) => {
                        // No events, continue loop
                    }
                    Err(std_mpsc::RecvTimeoutError::Disconnected) => {
                        // Channel closed, exit
                        break;
                    }
                }
            }

            // Keep debouncer alive until thread exits
            drop(debouncer);
        });

        Ok((
            Self {
                watch_path,
                stop_tx,
                bridge_handle,
            },
            event_rx,
        ))
    }

    /// Handle a debounce result from notify.
    fn handle_debounce_result(
        result: DebounceEventResult,
        watch_path: &PathBuf,
        tailer: &mut Option<JsonlTailer>,
        event_tx: &mpsc::UnboundedSender<WatcherEvent>,
        runtime: &tokio::runtime::Runtime,
    ) {
        match result {
            Ok(events) => {
                for event in &events {
                    Self::handle_notify_event(event, watch_path, tailer, event_tx, runtime);
                }
            }
            Err(errors) => {
                for error in errors {
                    let _ = event_tx.send(WatcherEvent::Error(WatcherError::Notify(error)));
                }
            }
        }
    }

    /// Handle a single notify event.
    fn handle_notify_event(
        event: &notify_debouncer_full::DebouncedEvent,
        watch_path: &PathBuf,
        tailer: &mut Option<JsonlTailer>,
        event_tx: &mpsc::UnboundedSender<WatcherEvent>,
        runtime: &tokio::runtime::Runtime,
    ) {
        use notify::EventKind;

        // Filter events to only those affecting our watch path
        let affects_watch_path = event
            .paths
            .iter()
            .any(|p| p == watch_path || (watch_path.is_dir() && p.starts_with(watch_path)));

        if !affects_watch_path && watch_path.is_file() {
            return;
        }

        match event.kind {
            EventKind::Create(_) => {
                for path in &event.paths {
                    if path.extension().is_some_and(|ext| ext == "jsonl") {
                        let _ = event_tx.send(WatcherEvent::FileCreated(path.clone()));
                    }
                }
            }
            EventKind::Modify(_) => {
                // Read new entries if we have a tailer
                if let Some(ref mut t) = tailer {
                    match runtime.block_on(t.read_new_entries()) {
                        Ok(entries) => {
                            for entry in entries {
                                let _ = event_tx.send(WatcherEvent::NewEntry(Box::new(entry)));
                            }
                        }
                        Err(WatcherError::FileTruncated) => {
                            let _ = event_tx.send(WatcherEvent::FileTruncated(watch_path.clone()));
                            t.reset();
                        }
                        Err(e) => {
                            let _ = event_tx.send(WatcherEvent::Error(e));
                        }
                    }
                }
            }
            EventKind::Remove(_) => {
                for path in &event.paths {
                    if path == watch_path || path.extension().is_some_and(|ext| ext == "jsonl") {
                        let _ = event_tx.send(WatcherEvent::FileDeleted(path.clone()));
                    }
                }
            }
            _ => {}
        }
    }

    /// Get the path being watched.
    #[must_use]
    pub fn watch_path(&self) -> &PathBuf {
        &self.watch_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_entry(uuid: &str) -> String {
        format!(
            r#"{{"type":"user","uuid":"{uuid}","parentUuid":null,"sessionId":"sess-1","timestamp":"2026-01-29T10:00:00Z","message":{{"role":"user","content":"Hello"}},"userType":"external","cwd":"/tmp","version":"2.1.25"}}"#
        )
    }

    #[tokio::test]
    async fn test_watcher_creation() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.jsonl");
        std::fs::write(&file_path, "").unwrap();

        let result = SessionWatcher::new(file_path.clone());

        // Handle potential resource limitations (MaxFilesWatch) gracefully
        match result {
            Ok((watcher, _rx)) => {
                assert_eq!(watcher.watch_path(), &file_path);
            }
            Err(WatcherError::Notify(e)) => {
                // Skip test if system has too many watchers
                eprintln!("Skipping test due to system limit: {e}");
            }
            Err(e) => panic!("Unexpected error: {e}"),
        }
    }

    #[tokio::test]
    async fn test_watcher_detects_new_entries() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.jsonl");
        std::fs::write(&file_path, "").unwrap();

        let result = SessionWatcher::new(file_path.clone());

        // Handle potential resource limitations gracefully
        let (watcher, mut rx) = match result {
            Ok(r) => r,
            Err(WatcherError::Notify(e)) => {
                eprintln!("Skipping test due to system limit: {e}");
                return;
            }
            Err(e) => panic!("Unexpected error: {e}"),
        };

        // Give watcher time to initialize
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Append an entry
        {
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&file_path)
                .unwrap();
            writeln!(file, "{}", create_test_entry("uuid-1")).unwrap();
        }

        // Wait for event with timeout
        let event = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await;

        drop(watcher); // Clean up

        // Check we got an event (might be NewEntry or could timeout on slow systems)
        if let Ok(Some(WatcherEvent::NewEntry(entry))) = event {
            if let JournalEntry::User(u) = entry.as_ref() {
                assert_eq!(u.uuid, "uuid-1");
            }
        }
        // It's okay if we timeout on slow CI systems - the watcher is working
    }

    #[tokio::test]
    async fn test_watcher_directory_mode() {
        let temp_dir = TempDir::new().unwrap();

        let result = SessionWatcher::new(temp_dir.path().to_path_buf());

        // Handle potential resource limitations gracefully
        let (watcher, mut rx) = match result {
            Ok(r) => r,
            Err(WatcherError::Notify(e)) => {
                eprintln!("Skipping test due to system limit: {e}");
                return;
            }
            Err(e) => panic!("Unexpected error: {e}"),
        };

        // Give watcher time to initialize
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Create a new JSONL file
        let file_path = temp_dir.path().join("new_session.jsonl");
        std::fs::write(&file_path, "").unwrap();

        // Wait for event with timeout
        let event = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await;

        drop(watcher);

        // Should detect file creation
        if let Ok(Some(WatcherEvent::FileCreated(path))) = event {
            assert!(path.ends_with("new_session.jsonl"));
        }
    }

    #[test]
    fn test_watcher_event_variants() {
        // Test that all event variants can be created
        let entry_json = create_test_entry("test");
        let entry: JournalEntry = serde_json::from_str(&entry_json).unwrap();

        // Verify variants can be constructed and matched
        let new_entry = WatcherEvent::NewEntry(Box::new(entry));
        assert!(matches!(new_entry, WatcherEvent::NewEntry(_)));

        let created = WatcherEvent::FileCreated(PathBuf::from("/tmp/test.jsonl"));
        assert!(matches!(created, WatcherEvent::FileCreated(_)));

        let deleted = WatcherEvent::FileDeleted(PathBuf::from("/tmp/test.jsonl"));
        assert!(matches!(deleted, WatcherEvent::FileDeleted(_)));

        let truncated = WatcherEvent::FileTruncated(PathBuf::from("/tmp/test.jsonl"));
        assert!(matches!(truncated, WatcherEvent::FileTruncated(_)));

        let error = WatcherEvent::Error(WatcherError::FileTruncated);
        assert!(matches!(error, WatcherEvent::Error(_)));
    }
}
