//! Bridge between watcher events and hook handling.
//!
//! Provides a unified interface for processing journal entries and
//! detecting patterns across watcher and hook components.

use std::sync::Arc;

use tokio::sync::{mpsc, RwLock};

use crate::watcher::{
    JournalEntry, PatternDetector, SessionReconstructor, StuckPattern, ToolCallRecord, WatcherEvent,
};

/// Bridge connecting watcher events to hook handling.
///
/// Maintains a session reconstructor and provides methods for
/// processing journal entries and detecting stuck patterns.
#[derive(Debug)]
pub struct WatcherHookBridge {
    tx: mpsc::Sender<WatcherEvent>,
    reconstructor: Arc<RwLock<SessionReconstructor>>,
    pattern_detector: PatternDetector,
}

impl WatcherHookBridge {
    /// Create a new bridge with the given event sender.
    #[must_use]
    pub fn new(tx: mpsc::Sender<WatcherEvent>) -> Self {
        Self {
            tx,
            reconstructor: Arc::new(RwLock::new(SessionReconstructor::new())),
            pattern_detector: PatternDetector::new(),
        }
    }

    /// Create a bridge with a custom pattern detector.
    #[must_use]
    pub fn with_pattern_detector(
        tx: mpsc::Sender<WatcherEvent>,
        pattern_detector: PatternDetector,
    ) -> Self {
        Self {
            tx,
            reconstructor: Arc::new(RwLock::new(SessionReconstructor::new())),
            pattern_detector,
        }
    }

    /// Send a watcher event.
    ///
    /// # Errors
    ///
    /// Returns an error if the channel is closed.
    pub async fn send(
        &self,
        event: WatcherEvent,
    ) -> Result<(), mpsc::error::SendError<WatcherEvent>> {
        self.tx.send(event).await
    }

    /// Process a journal entry, updating the session reconstructor.
    pub async fn process_entry(&self, entry: JournalEntry) {
        let mut reconstructor = self.reconstructor.write().await;
        reconstructor.process_entry(&entry);
    }

    /// Process multiple journal entries in order.
    pub async fn process_entries(&self, entries: Vec<JournalEntry>) {
        let mut reconstructor = self.reconstructor.write().await;
        for entry in &entries {
            reconstructor.process_entry(entry);
        }
    }

    /// Get the most recent N tool calls.
    pub async fn get_recent_calls(&self, n: usize) -> Vec<ToolCallRecord> {
        let reconstructor = self.reconstructor.read().await;
        reconstructor
            .recent_tool_calls(n)
            .into_iter()
            .cloned()
            .collect()
    }

    /// Get all completed tool calls.
    pub async fn get_all_calls(&self) -> Vec<ToolCallRecord> {
        let reconstructor = self.reconstructor.read().await;
        reconstructor.tool_calls().to_vec()
    }

    /// Detect stuck patterns in the current tool call history.
    pub async fn detect_stuck(&self) -> Option<StuckPattern> {
        let reconstructor = self.reconstructor.read().await;
        reconstructor.detect_stuck_pattern(&self.pattern_detector)
    }

    /// Get the total number of entries processed.
    pub async fn entry_count(&self) -> usize {
        let reconstructor = self.reconstructor.read().await;
        reconstructor.entry_count()
    }

    /// Get the total number of completed tool calls.
    pub async fn tool_call_count(&self) -> usize {
        let reconstructor = self.reconstructor.read().await;
        reconstructor.tool_calls().len()
    }

    /// Clear all state in the reconstructor.
    pub async fn clear(&self) {
        let mut reconstructor = self.reconstructor.write().await;
        reconstructor.clear();
    }

    /// Get a clone of the reconstructor for external use.
    #[must_use]
    pub fn reconstructor(&self) -> Arc<RwLock<SessionReconstructor>> {
        Arc::clone(&self.reconstructor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watcher::PatternThresholds;

    fn create_bridge() -> (WatcherHookBridge, mpsc::Receiver<WatcherEvent>) {
        let (tx, rx) = mpsc::channel(16);
        let bridge = WatcherHookBridge::new(tx);
        (bridge, rx)
    }

    #[tokio::test]
    async fn test_bridge_creation() {
        let (bridge, _rx) = create_bridge();
        assert_eq!(bridge.entry_count().await, 0);
        assert_eq!(bridge.tool_call_count().await, 0);
    }

    #[tokio::test]
    async fn test_bridge_with_pattern_detector() {
        let (tx, _rx) = mpsc::channel(16);
        let thresholds = PatternThresholds {
            repeating_action: 2,
            repeating_error: 2,
            alternating_cycles: 2,
            window_size: 10,
        };
        let detector = PatternDetector::with_thresholds(thresholds);
        let bridge = WatcherHookBridge::with_pattern_detector(tx, detector);
        assert_eq!(bridge.entry_count().await, 0);
    }

    #[tokio::test]
    async fn test_send_event() {
        use std::path::PathBuf;
        let (bridge, mut rx) = create_bridge();

        let event = WatcherEvent::FileCreated(PathBuf::from("/tmp/test.jsonl"));

        bridge.send(event).await.unwrap();

        let received = rx.recv().await.unwrap();
        assert!(matches!(received, WatcherEvent::FileCreated(_)));
    }

    #[tokio::test]
    async fn test_get_recent_calls_empty() {
        let (bridge, _rx) = create_bridge();
        let calls = bridge.get_recent_calls(10).await;
        assert!(calls.is_empty());
    }

    #[tokio::test]
    async fn test_detect_stuck_empty() {
        let (bridge, _rx) = create_bridge();
        let pattern = bridge.detect_stuck().await;
        assert!(pattern.is_none());
    }

    #[tokio::test]
    async fn test_clear() {
        let (bridge, _rx) = create_bridge();
        // Nothing to clear, but should not panic
        bridge.clear().await;
        assert_eq!(bridge.entry_count().await, 0);
    }

    #[tokio::test]
    async fn test_reconstructor_access() {
        let (bridge, _rx) = create_bridge();
        let reconstructor = bridge.reconstructor();
        let guard = reconstructor.read().await;
        assert_eq!(guard.entry_count(), 0);
    }
}
