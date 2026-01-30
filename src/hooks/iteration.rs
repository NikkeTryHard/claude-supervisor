//! Iteration tracking for stop hook handling.

use std::collections::HashMap;
use std::sync::RwLock;

/// Tracks iteration counts per session.
#[derive(Debug, Default)]
pub struct IterationTracker {
    counts: RwLock<HashMap<String, u32>>,
}

impl IterationTracker {
    /// Create a new iteration tracker.
    #[must_use]
    pub fn new() -> Self {
        Self {
            counts: RwLock::new(HashMap::new()),
        }
    }

    /// Get the current iteration count for a session.
    ///
    /// # Panics
    ///
    /// Panics if the internal `RwLock` is poisoned.
    #[must_use]
    pub fn get(&self, session_id: &str) -> u32 {
        self.counts
            .read()
            .expect("RwLock poisoned")
            .get(session_id)
            .copied()
            .unwrap_or(0)
    }

    /// Increment the iteration count for a session and return the new count.
    ///
    /// # Panics
    ///
    /// Panics if the internal `RwLock` is poisoned.
    pub fn increment(&self, session_id: &str) -> u32 {
        let mut counts = self.counts.write().expect("RwLock poisoned");
        let count = counts.entry(session_id.to_string()).or_insert(0);
        *count += 1;
        *count
    }

    /// Reset the iteration count for a session.
    ///
    /// # Panics
    ///
    /// Panics if the internal `RwLock` is poisoned.
    pub fn reset(&self, session_id: &str) {
        let mut counts = self.counts.write().expect("RwLock poisoned");
        counts.remove(session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iteration_tracker_new() {
        let tracker = IterationTracker::new();
        assert_eq!(tracker.get("session1"), 0);
    }

    #[test]
    fn test_iteration_tracker_increment() {
        let tracker = IterationTracker::new();
        assert_eq!(tracker.increment("session1"), 1);
        assert_eq!(tracker.increment("session1"), 2);
        assert_eq!(tracker.increment("session1"), 3);
        assert_eq!(tracker.get("session1"), 3);
    }

    #[test]
    fn test_iteration_tracker_multiple_sessions() {
        let tracker = IterationTracker::new();
        assert_eq!(tracker.increment("session1"), 1);
        assert_eq!(tracker.increment("session2"), 1);
        assert_eq!(tracker.increment("session1"), 2);
        assert_eq!(tracker.get("session1"), 2);
        assert_eq!(tracker.get("session2"), 1);
    }

    #[test]
    fn test_iteration_tracker_reset() {
        let tracker = IterationTracker::new();
        tracker.increment("session1");
        tracker.increment("session1");
        assert_eq!(tracker.get("session1"), 2);
        tracker.reset("session1");
        assert_eq!(tracker.get("session1"), 0);
    }
}
