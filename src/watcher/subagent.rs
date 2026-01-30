//! Subagent tracking for Claude Code Task tool spawns.

use std::collections::HashMap;
use std::path::PathBuf;

/// Status of a tracked subagent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubagentStatus {
    Running,
    Completed,
    Failed,
}

/// Record of a spawned subagent.
#[derive(Debug, Clone)]
pub struct SubagentRecord {
    /// Unique identifier for this subagent.
    pub agent_id: String,
    /// Session ID of the parent that spawned this subagent.
    pub parent_session_id: String,
    /// Path to the subagent's JSONL conversation file.
    pub jsonl_path: PathBuf,
    /// Current status of the subagent.
    pub status: SubagentStatus,
    /// Nesting depth (1 = direct child of main session).
    pub depth: u32,
}

impl SubagentRecord {
    /// Create a new subagent record (depth 1, direct child).
    #[must_use]
    pub fn new(agent_id: String, parent_session_id: String, jsonl_path: PathBuf) -> Self {
        Self {
            agent_id,
            parent_session_id,
            jsonl_path,
            status: SubagentStatus::Running,
            depth: 1,
        }
    }

    /// Create a nested subagent record with inherited depth.
    #[must_use]
    pub fn nested(
        agent_id: String,
        parent_session_id: String,
        jsonl_path: PathBuf,
        parent_depth: u32,
    ) -> Self {
        Self {
            agent_id,
            parent_session_id,
            jsonl_path,
            status: SubagentStatus::Running,
            depth: parent_depth + 1,
        }
    }

    /// Mark the subagent as completed.
    pub fn mark_completed(&mut self) {
        self.status = SubagentStatus::Completed;
    }

    /// Mark the subagent as failed.
    pub fn mark_failed(&mut self) {
        self.status = SubagentStatus::Failed;
    }
}

/// Default maximum number of subagents to track.
pub const DEFAULT_MAX_SUBAGENTS: usize = 32;

/// Tracks spawned subagents for a supervision session.
#[derive(Debug)]
pub struct SubagentTracker {
    agents: HashMap<String, SubagentRecord>,
    max_agents: usize,
}

impl SubagentTracker {
    /// Create a new tracker with specified maximum capacity.
    #[must_use]
    pub fn new(max_agents: usize) -> Self {
        Self {
            agents: HashMap::new(),
            max_agents,
        }
    }

    /// Register a new subagent.
    ///
    /// If the maximum number of agents is reached, the registration is ignored
    /// and a warning is logged.
    pub fn register(&mut self, record: SubagentRecord) {
        if self.agents.len() >= self.max_agents {
            tracing::warn!(
                agent_id = %record.agent_id,
                max = self.max_agents,
                "Subagent limit reached, ignoring registration"
            );
            return;
        }
        tracing::debug!(
            agent_id = %record.agent_id,
            parent = %record.parent_session_id,
            depth = record.depth,
            "Registered subagent"
        );
        self.agents.insert(record.agent_id.clone(), record);
    }

    /// Get a subagent by its ID.
    #[must_use]
    pub fn get(&self, agent_id: &str) -> Option<&SubagentRecord> {
        self.agents.get(agent_id)
    }

    /// Get a mutable reference to a subagent by its ID.
    #[must_use]
    pub fn get_mut(&mut self, agent_id: &str) -> Option<&mut SubagentRecord> {
        self.agents.get_mut(agent_id)
    }

    /// Get all subagents spawned by a specific session.
    #[must_use]
    pub fn by_session(&self, session_id: &str) -> Vec<&SubagentRecord> {
        self.agents
            .values()
            .filter(|r| r.parent_session_id == session_id)
            .collect()
    }

    /// Get the total number of tracked subagents.
    #[must_use]
    pub fn count(&self) -> usize {
        self.agents.len()
    }

    /// Get all currently running subagents.
    #[must_use]
    pub fn running(&self) -> Vec<&SubagentRecord> {
        self.agents
            .values()
            .filter(|r| r.status == SubagentStatus::Running)
            .collect()
    }

    /// Get all completed subagents.
    #[must_use]
    pub fn completed(&self) -> Vec<&SubagentRecord> {
        self.agents
            .values()
            .filter(|r| r.status == SubagentStatus::Completed)
            .collect()
    }

    /// Get all failed subagents.
    #[must_use]
    pub fn failed(&self) -> Vec<&SubagentRecord> {
        self.agents
            .values()
            .filter(|r| r.status == SubagentStatus::Failed)
            .collect()
    }
}

impl Default for SubagentTracker {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_SUBAGENTS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_record(agent_id: &str, parent: &str) -> SubagentRecord {
        SubagentRecord::new(
            agent_id.to_string(),
            parent.to_string(),
            PathBuf::from(format!("/tmp/subagents/{agent_id}.jsonl")),
        )
    }

    // SubagentStatus tests
    #[test]
    fn test_subagent_status_equality() {
        assert_eq!(SubagentStatus::Running, SubagentStatus::Running);
        assert_eq!(SubagentStatus::Completed, SubagentStatus::Completed);
        assert_eq!(SubagentStatus::Failed, SubagentStatus::Failed);
        assert_ne!(SubagentStatus::Running, SubagentStatus::Completed);
    }

    #[test]
    fn test_subagent_status_clone() {
        let status = SubagentStatus::Running;
        let cloned = status;
        assert_eq!(status, cloned);
    }

    // SubagentRecord tests
    #[test]
    fn test_subagent_record_new() {
        let record = SubagentRecord::new(
            "agent-abc123".to_string(),
            "session-xyz".to_string(),
            PathBuf::from("/tmp/agent.jsonl"),
        );

        assert_eq!(record.agent_id, "agent-abc123");
        assert_eq!(record.parent_session_id, "session-xyz");
        assert_eq!(record.jsonl_path, PathBuf::from("/tmp/agent.jsonl"));
        assert_eq!(record.status, SubagentStatus::Running);
        assert_eq!(record.depth, 1);
    }

    #[test]
    fn test_subagent_record_nested() {
        let record = SubagentRecord::nested(
            "agent-nested".to_string(),
            "parent-session".to_string(),
            PathBuf::from("/tmp/nested.jsonl"),
            2,
        );

        assert_eq!(record.agent_id, "agent-nested");
        assert_eq!(record.depth, 3); // parent_depth + 1
        assert_eq!(record.status, SubagentStatus::Running);
    }

    #[test]
    fn test_subagent_record_mark_completed() {
        let mut record = create_test_record("agent-1", "session-1");
        assert_eq!(record.status, SubagentStatus::Running);

        record.mark_completed();
        assert_eq!(record.status, SubagentStatus::Completed);
    }

    #[test]
    fn test_subagent_record_mark_failed() {
        let mut record = create_test_record("agent-1", "session-1");
        assert_eq!(record.status, SubagentStatus::Running);

        record.mark_failed();
        assert_eq!(record.status, SubagentStatus::Failed);
    }

    #[test]
    fn test_subagent_record_clone() {
        let original = create_test_record("agent-1", "session-1");
        let cloned = original.clone();

        assert_eq!(original.agent_id, cloned.agent_id);
        assert_eq!(original.parent_session_id, cloned.parent_session_id);
        assert_eq!(original.jsonl_path, cloned.jsonl_path);
        assert_eq!(original.status, cloned.status);
        assert_eq!(original.depth, cloned.depth);
    }

    // SubagentTracker tests
    #[test]
    fn test_subagent_tracker_new() {
        let tracker = SubagentTracker::new(10);
        assert_eq!(tracker.count(), 0);
    }

    #[test]
    fn test_subagent_tracker_default() {
        let tracker = SubagentTracker::default();
        assert_eq!(tracker.count(), 0);
        assert_eq!(tracker.max_agents, DEFAULT_MAX_SUBAGENTS);
    }

    #[test]
    fn test_subagent_tracker_register() {
        let mut tracker = SubagentTracker::new(10);
        let record = create_test_record("agent-1", "session-1");

        tracker.register(record);
        assert_eq!(tracker.count(), 1);
    }

    #[test]
    fn test_subagent_tracker_register_limit() {
        let mut tracker = SubagentTracker::new(2);

        tracker.register(create_test_record("agent-1", "session-1"));
        tracker.register(create_test_record("agent-2", "session-1"));
        tracker.register(create_test_record("agent-3", "session-1")); // Should be ignored

        assert_eq!(tracker.count(), 2);
        assert!(tracker.get("agent-1").is_some());
        assert!(tracker.get("agent-2").is_some());
        assert!(tracker.get("agent-3").is_none());
    }

    #[test]
    fn test_subagent_tracker_get() {
        let mut tracker = SubagentTracker::new(10);
        tracker.register(create_test_record("agent-1", "session-1"));

        let record = tracker.get("agent-1");
        assert!(record.is_some());
        assert_eq!(record.unwrap().agent_id, "agent-1");

        assert!(tracker.get("nonexistent").is_none());
    }

    #[test]
    fn test_subagent_tracker_get_mut() {
        let mut tracker = SubagentTracker::new(10);
        tracker.register(create_test_record("agent-1", "session-1"));

        if let Some(record) = tracker.get_mut("agent-1") {
            record.mark_completed();
        }

        let record = tracker.get("agent-1").unwrap();
        assert_eq!(record.status, SubagentStatus::Completed);
    }

    #[test]
    fn test_subagent_tracker_by_session() {
        let mut tracker = SubagentTracker::new(10);
        tracker.register(create_test_record("agent-1", "session-a"));
        tracker.register(create_test_record("agent-2", "session-a"));
        tracker.register(create_test_record("agent-3", "session-b"));

        let agents_in_a = tracker.by_session("session-a");
        assert_eq!(agents_in_a.len(), 2);

        let agents_in_b = tracker.by_session("session-b");
        assert_eq!(agents_in_b.len(), 1);

        let agents_in_c = tracker.by_session("session-c");
        assert!(agents_in_c.is_empty());
    }

    #[test]
    fn test_subagent_tracker_running() {
        let mut tracker = SubagentTracker::new(10);
        tracker.register(create_test_record("agent-1", "session-1"));
        tracker.register(create_test_record("agent-2", "session-1"));

        if let Some(record) = tracker.get_mut("agent-1") {
            record.mark_completed();
        }

        let running = tracker.running();
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].agent_id, "agent-2");
    }

    #[test]
    fn test_subagent_tracker_completed() {
        let mut tracker = SubagentTracker::new(10);
        tracker.register(create_test_record("agent-1", "session-1"));
        tracker.register(create_test_record("agent-2", "session-1"));

        if let Some(record) = tracker.get_mut("agent-1") {
            record.mark_completed();
        }

        let completed = tracker.completed();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].agent_id, "agent-1");
    }

    #[test]
    fn test_subagent_tracker_failed() {
        let mut tracker = SubagentTracker::new(10);
        tracker.register(create_test_record("agent-1", "session-1"));
        tracker.register(create_test_record("agent-2", "session-1"));

        if let Some(record) = tracker.get_mut("agent-2") {
            record.mark_failed();
        }

        let failed = tracker.failed();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].agent_id, "agent-2");
    }

    #[test]
    fn test_subagent_tracker_mixed_statuses() {
        let mut tracker = SubagentTracker::new(10);
        tracker.register(create_test_record("running-1", "session-1"));
        tracker.register(create_test_record("completed-1", "session-1"));
        tracker.register(create_test_record("failed-1", "session-1"));

        if let Some(record) = tracker.get_mut("completed-1") {
            record.mark_completed();
        }
        if let Some(record) = tracker.get_mut("failed-1") {
            record.mark_failed();
        }

        assert_eq!(tracker.running().len(), 1);
        assert_eq!(tracker.completed().len(), 1);
        assert_eq!(tracker.failed().len(), 1);
        assert_eq!(tracker.count(), 3);
    }
}
