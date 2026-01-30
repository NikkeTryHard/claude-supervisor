# Phase 4: Conversation Awareness Implementation Plan

> **REQUIRED:** Use `execute-plan` to implement this plan batch by batch.

**Goal:** Extend the watcher module with subagent tracking (#24), pattern detection (#25), and integrate with hooks for auto-continue (#26).

**Architecture:** The watcher already has `SessionReconstructor` tracking tool calls. We add: (1) `SubagentTracker` watching `<session>/subagents/` directories, (2) `PatternDetector` analyzing tool call sequences for stuck patterns, (3) integration layer connecting watcher events to `handle_stop()` decisions.

**Tech Stack:** Rust, notify-debouncer-full, tokio mpsc channels, serde_json

**Dependencies:** All three issues depend on #23 (file watching) which is already implemented.

---

## Batch 1: Subagent Tracking (#24)

**Goal:** Watch subagent directories and track parent-child relationships.

### Task 1.1: Add SubagentRecord and SubagentStatus Types

**Files:**
- Create: `src/watcher/subagent.rs`
- Modify: `src/watcher/mod.rs`

**Step 1: Write failing test**

```rust
// In src/watcher/subagent.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subagent_record_creation() {
        let record = SubagentRecord::new(
            "ac243a8".to_string(),
            "9349fa59-4417-4984-a8aa-a8b4ab25e958".to_string(),
            PathBuf::from("/tmp/agent-ac243a8.jsonl"),
        );

        assert_eq!(record.agent_id, "ac243a8");
        assert_eq!(record.parent_session_id, "9349fa59-4417-4984-a8aa-a8b4ab25e958");
        assert_eq!(record.status, SubagentStatus::Running);
        assert_eq!(record.depth, 1);
    }

    #[test]
    fn test_subagent_status_transitions() {
        let mut record = SubagentRecord::new(
            "abc1234".to_string(),
            "parent-uuid".to_string(),
            PathBuf::from("/tmp/test.jsonl"),
        );

        assert_eq!(record.status, SubagentStatus::Running);
        record.mark_completed();
        assert_eq!(record.status, SubagentStatus::Completed);
    }
}
```

**Step 2: Verify failure**

Run: `cargo t subagent_record -p claude-supervisor --lib`

Expected: FAIL with "cannot find module `subagent`"

**Step 3: Implement**

```rust
// src/watcher/subagent.rs
//! Subagent tracking for Claude Code Task tool spawns.

use std::path::PathBuf;

/// Status of a tracked subagent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubagentStatus {
    /// Subagent is actively running.
    Running,
    /// Subagent completed successfully.
    Completed,
    /// Subagent failed or was terminated.
    Failed,
}

/// Record of a spawned subagent.
#[derive(Debug, Clone)]
pub struct SubagentRecord {
    /// The 7-char hex agent ID (matches filename suffix).
    pub agent_id: String,
    /// Parent session UUID that spawned this subagent.
    pub parent_session_id: String,
    /// Path to the subagent's JSONL file.
    pub jsonl_path: PathBuf,
    /// Current status of the subagent.
    pub status: SubagentStatus,
    /// Nesting depth (1 = direct child, 2 = grandchild, etc.).
    pub depth: u32,
}

impl SubagentRecord {
    /// Create a new subagent record.
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

    /// Create a nested subagent (child of another subagent).
    #[must_use]
    pub fn nested(agent_id: String, parent_session_id: String, jsonl_path: PathBuf, parent_depth: u32) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subagent_record_creation() {
        let record = SubagentRecord::new(
            "ac243a8".to_string(),
            "9349fa59-4417-4984-a8aa-a8b4ab25e958".to_string(),
            PathBuf::from("/tmp/agent-ac243a8.jsonl"),
        );

        assert_eq!(record.agent_id, "ac243a8");
        assert_eq!(record.parent_session_id, "9349fa59-4417-4984-a8aa-a8b4ab25e958");
        assert_eq!(record.status, SubagentStatus::Running);
        assert_eq!(record.depth, 1);
    }

    #[test]
    fn test_subagent_status_transitions() {
        let mut record = SubagentRecord::new(
            "abc1234".to_string(),
            "parent-uuid".to_string(),
            PathBuf::from("/tmp/test.jsonl"),
        );

        assert_eq!(record.status, SubagentStatus::Running);
        record.mark_completed();
        assert_eq!(record.status, SubagentStatus::Completed);
    }

    #[test]
    fn test_nested_subagent_depth() {
        let parent = SubagentRecord::new(
            "parent1".to_string(),
            "session-uuid".to_string(),
            PathBuf::from("/tmp/parent.jsonl"),
        );

        let child = SubagentRecord::nested(
            "child1".to_string(),
            "session-uuid".to_string(),
            PathBuf::from("/tmp/child.jsonl"),
            parent.depth,
        );

        assert_eq!(child.depth, 2);
    }
}
```

**Step 4: Update mod.rs**

```rust
// Add to src/watcher/mod.rs after other mod declarations:
mod subagent;

// Add to pub use section:
pub use subagent::{SubagentRecord, SubagentStatus};
```

**Step 5: Verify pass**

Run: `cargo t subagent -p claude-supervisor --lib`

Expected: 3 tests PASS

**Step 6: Commit**

```bash
git add src/watcher/subagent.rs src/watcher/mod.rs
git commit -m "feat(watcher): add SubagentRecord and SubagentStatus types"
```

---

### Task 1.2: Add SubagentTracker with Registration and Lookup

**Files:**
- Modify: `src/watcher/subagent.rs`

**Step 1: Write failing test**

```rust
// Add to src/watcher/subagent.rs tests module:

#[test]
fn test_subagent_tracker_register_and_get() {
    let mut tracker = SubagentTracker::new(32);

    let record = SubagentRecord::new(
        "abc1234".to_string(),
        "session-uuid".to_string(),
        PathBuf::from("/tmp/agent-abc1234.jsonl"),
    );

    tracker.register(record);

    let found = tracker.get("abc1234");
    assert!(found.is_some());
    assert_eq!(found.unwrap().agent_id, "abc1234");
}

#[test]
fn test_subagent_tracker_max_limit() {
    let mut tracker = SubagentTracker::new(2);

    tracker.register(SubagentRecord::new("a".into(), "s".into(), PathBuf::from("/a")));
    tracker.register(SubagentRecord::new("b".into(), "s".into(), PathBuf::from("/b")));

    // Third registration should fail silently (at limit)
    tracker.register(SubagentRecord::new("c".into(), "s".into(), PathBuf::from("/c")));

    assert_eq!(tracker.count(), 2);
    assert!(tracker.get("c").is_none());
}

#[test]
fn test_subagent_tracker_by_session() {
    let mut tracker = SubagentTracker::new(32);

    tracker.register(SubagentRecord::new("a1".into(), "session-1".into(), PathBuf::from("/a1")));
    tracker.register(SubagentRecord::new("a2".into(), "session-1".into(), PathBuf::from("/a2")));
    tracker.register(SubagentRecord::new("b1".into(), "session-2".into(), PathBuf::from("/b1")));

    let session1_agents = tracker.by_session("session-1");
    assert_eq!(session1_agents.len(), 2);

    let session2_agents = tracker.by_session("session-2");
    assert_eq!(session2_agents.len(), 1);
}
```

**Step 2: Verify failure**

Run: `cargo t subagent_tracker -p claude-supervisor --lib`

Expected: FAIL with "cannot find value `SubagentTracker`"

**Step 3: Implement**

```rust
// Add to src/watcher/subagent.rs after SubagentRecord impl:

use std::collections::HashMap;

/// Default maximum number of tracked subagents.
pub const DEFAULT_MAX_SUBAGENTS: usize = 32;

/// Tracks active and completed subagents for a supervisor session.
#[derive(Debug)]
pub struct SubagentTracker {
    /// Subagents indexed by agent_id.
    agents: HashMap<String, SubagentRecord>,
    /// Maximum number of subagents to track.
    max_agents: usize,
}

impl SubagentTracker {
    /// Create a new tracker with a maximum agent limit.
    #[must_use]
    pub fn new(max_agents: usize) -> Self {
        Self {
            agents: HashMap::new(),
            max_agents,
        }
    }

    /// Register a new subagent. Silently fails if at capacity.
    pub fn register(&mut self, record: SubagentRecord) {
        if self.agents.len() >= self.max_agents {
            tracing::warn!(
                agent_id = %record.agent_id,
                max = self.max_agents,
                "Subagent limit reached, ignoring new agent"
            );
            return;
        }
        self.agents.insert(record.agent_id.clone(), record);
    }

    /// Get a subagent by ID.
    #[must_use]
    pub fn get(&self, agent_id: &str) -> Option<&SubagentRecord> {
        self.agents.get(agent_id)
    }

    /// Get a mutable reference to a subagent by ID.
    pub fn get_mut(&mut self, agent_id: &str) -> Option<&mut SubagentRecord> {
        self.agents.get_mut(agent_id)
    }

    /// Get all subagents for a given parent session.
    #[must_use]
    pub fn by_session(&self, session_id: &str) -> Vec<&SubagentRecord> {
        self.agents
            .values()
            .filter(|r| r.parent_session_id == session_id)
            .collect()
    }

    /// Get count of tracked subagents.
    #[must_use]
    pub fn count(&self) -> usize {
        self.agents.len()
    }

    /// Get all running subagents.
    #[must_use]
    pub fn running(&self) -> Vec<&SubagentRecord> {
        self.agents
            .values()
            .filter(|r| r.status == SubagentStatus::Running)
            .collect()
    }

    /// Remove completed subagents older than the limit, keeping most recent.
    pub fn prune_completed(&mut self, keep: usize) {
        let completed: Vec<_> = self.agents
            .iter()
            .filter(|(_, r)| r.status != SubagentStatus::Running)
            .map(|(id, _)| id.clone())
            .collect();

        if completed.len() > keep {
            let to_remove = completed.len() - keep;
            for id in completed.into_iter().take(to_remove) {
                self.agents.remove(&id);
            }
        }
    }
}

impl Default for SubagentTracker {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_SUBAGENTS)
    }
}
```

**Step 4: Update exports in mod.rs**

```rust
// Update pub use line in src/watcher/mod.rs:
pub use subagent::{SubagentRecord, SubagentStatus, SubagentTracker, DEFAULT_MAX_SUBAGENTS};
```

**Step 5: Verify pass**

Run: `cargo t subagent_tracker -p claude-supervisor --lib`

Expected: 3 tests PASS

**Step 6: Commit**

```bash
git add src/watcher/subagent.rs src/watcher/mod.rs
git commit -m "feat(watcher): add SubagentTracker with registration and lookup"
```

---

### Task 1.3: Add Subagent Discovery from Filesystem

**Files:**
- Modify: `src/watcher/discovery.rs`

**Step 1: Write failing test**

```rust
// Add to src/watcher/discovery.rs tests module:

#[test]
fn test_find_subagents_dir() {
    let temp = tempfile::tempdir().unwrap();
    let session_dir = temp.path().join("abc123-uuid");
    let subagents_dir = session_dir.join("subagents");
    std::fs::create_dir_all(&subagents_dir).unwrap();

    let result = find_subagents_dir(&session_dir);
    assert!(result.is_some());
    assert_eq!(result.unwrap(), subagents_dir);
}

#[test]
fn test_find_subagents_dir_missing() {
    let temp = tempfile::tempdir().unwrap();
    let session_dir = temp.path().join("abc123-uuid");
    std::fs::create_dir_all(&session_dir).unwrap();
    // No subagents dir

    let result = find_subagents_dir(&session_dir);
    assert!(result.is_none());
}

#[test]
fn test_discover_subagent_files() {
    let temp = tempfile::tempdir().unwrap();
    let subagents_dir = temp.path().join("subagents");
    std::fs::create_dir_all(&subagents_dir).unwrap();

    // Create some agent files
    std::fs::write(subagents_dir.join("agent-abc1234.jsonl"), "{}").unwrap();
    std::fs::write(subagents_dir.join("agent-def5678.jsonl"), "{}").unwrap();
    std::fs::write(subagents_dir.join("not-an-agent.txt"), "ignored").unwrap();

    let agents = discover_subagent_files(&subagents_dir).unwrap();
    assert_eq!(agents.len(), 2);

    let ids: Vec<_> = agents.iter().map(|(id, _)| id.as_str()).collect();
    assert!(ids.contains(&"abc1234"));
    assert!(ids.contains(&"def5678"));
}

#[test]
fn test_extract_agent_id_from_filename() {
    assert_eq!(extract_agent_id("agent-abc1234.jsonl"), Some("abc1234".to_string()));
    assert_eq!(extract_agent_id("agent-x.jsonl"), Some("x".to_string()));
    assert_eq!(extract_agent_id("not-agent.jsonl"), None);
    assert_eq!(extract_agent_id("agent-.jsonl"), None);
}
```

**Step 2: Verify failure**

Run: `cargo t find_subagents -p claude-supervisor --lib`

Expected: FAIL with "cannot find function `find_subagents_dir`"

**Step 3: Implement**

```rust
// Add to src/watcher/discovery.rs:

/// Find the subagents directory for a session.
///
/// Given a session directory path, checks if a `subagents/` subdirectory exists.
#[must_use]
pub fn find_subagents_dir(session_dir: &Path) -> Option<PathBuf> {
    let subagents = session_dir.join("subagents");
    if subagents.is_dir() {
        Some(subagents)
    } else {
        None
    }
}

/// Extract agent ID from a filename like "agent-abc1234.jsonl".
#[must_use]
pub fn extract_agent_id(filename: &str) -> Option<String> {
    if filename.starts_with("agent-") && filename.ends_with(".jsonl") {
        let id = filename
            .strip_prefix("agent-")?
            .strip_suffix(".jsonl")?;
        if id.is_empty() {
            None
        } else {
            Some(id.to_string())
        }
    } else {
        None
    }
}

/// Discover all subagent JSONL files in a subagents directory.
///
/// Returns a list of (agent_id, path) tuples.
pub fn discover_subagent_files(subagents_dir: &Path) -> Result<Vec<(String, PathBuf)>, WatcherError> {
    let mut agents = Vec::new();

    let entries = std::fs::read_dir(subagents_dir)
        .map_err(|e| WatcherError::Io {
            path: subagents_dir.to_path_buf(),
            source: e,
        })?;

    for entry in entries {
        let entry = entry.map_err(|e| WatcherError::Io {
            path: subagents_dir.to_path_buf(),
            source: e,
        })?;

        let path = entry.path();
        if path.is_file() {
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                if let Some(agent_id) = extract_agent_id(filename) {
                    agents.push((agent_id, path));
                }
            }
        }
    }

    Ok(agents)
}
```

**Step 4: Update exports in mod.rs**

```rust
// Update pub use discovery line in src/watcher/mod.rs:
pub use discovery::{
    discover_session, discover_subagent_files, extract_agent_id, find_latest_session,
    find_project_sessions_dir, find_session_by_id, find_subagents_dir, project_path_hash,
};
```

**Step 5: Verify pass**

Run: `cargo t subagent -p claude-supervisor --lib -- discovery`

Expected: 4 tests PASS

**Step 6: Commit**

```bash
git add src/watcher/discovery.rs src/watcher/mod.rs
git commit -m "feat(watcher): add subagent file discovery utilities"
```

---

## Batch 2: Pattern Detection (#25)

**Goal:** Detect stuck patterns in tool call sequences using OpenHands-validated thresholds.

### Task 2.1: Add StuckPattern Enum and PatternDetector Struct

**Files:**
- Create: `src/watcher/pattern.rs`
- Modify: `src/watcher/mod.rs`

**Step 1: Write failing test**

```rust
// In src/watcher/pattern.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::watcher::ToolCallRecord;

    fn make_tool_call(name: &str, input: &str, result: &str, is_error: bool) -> ToolCallRecord {
        ToolCallRecord {
            tool_use_id: uuid::Uuid::new_v4().to_string(),
            tool_name: name.to_string(),
            input: serde_json::json!({ "content": input }),
            result: Some(serde_json::json!({ "output": result })),
            is_error,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }

    #[test]
    fn test_no_pattern_with_varied_calls() {
        let detector = PatternDetector::default();
        let calls = vec![
            make_tool_call("Read", "file1.rs", "content1", false),
            make_tool_call("Edit", "file2.rs", "ok", false),
            make_tool_call("Bash", "cargo test", "passed", false),
        ];

        assert!(detector.detect(&calls).is_none());
    }

    #[test]
    fn test_detect_repeating_action_observation() {
        let detector = PatternDetector::default();
        // Same tool + same input + same result 4+ times
        let calls = vec![
            make_tool_call("Read", "file.rs", "content", false),
            make_tool_call("Read", "file.rs", "content", false),
            make_tool_call("Read", "file.rs", "content", false),
            make_tool_call("Read", "file.rs", "content", false),
        ];

        let pattern = detector.detect(&calls);
        assert!(pattern.is_some());
        assert!(matches!(pattern.unwrap(), StuckPattern::RepeatingAction { count: 4, .. }));
    }

    #[test]
    fn test_detect_repeating_errors() {
        let detector = PatternDetector::default();
        // Same error 3+ times
        let calls = vec![
            make_tool_call("Bash", "cargo build", "error: missing", true),
            make_tool_call("Bash", "cargo build", "error: missing", true),
            make_tool_call("Bash", "cargo build", "error: missing", true),
        ];

        let pattern = detector.detect(&calls);
        assert!(pattern.is_some());
        assert!(matches!(pattern.unwrap(), StuckPattern::RepeatingError { count: 3, .. }));
    }
}
```

**Step 2: Verify failure**

Run: `cargo t pattern -p claude-supervisor --lib`

Expected: FAIL with "cannot find module `pattern`"

**Step 3: Implement**

```rust
// src/watcher/pattern.rs
//! Pattern detection for stuck agent behaviors.
//!
//! Implements OpenHands-validated stuck detection patterns.

use crate::watcher::ToolCallRecord;

/// Detected stuck pattern with details.
#[derive(Debug, Clone, PartialEq)]
pub enum StuckPattern {
    /// Same action producing same result repeatedly.
    RepeatingAction {
        tool_name: String,
        count: usize,
    },
    /// Same action producing errors repeatedly.
    RepeatingError {
        tool_name: String,
        count: usize,
    },
    /// Alternating between two actions (ping-pong).
    AlternatingActions {
        tool_a: String,
        tool_b: String,
        cycles: usize,
    },
}

impl std::fmt::Display for StuckPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RepeatingAction { tool_name, count } => {
                write!(f, "Repeating {} {} times with same result", tool_name, count)
            }
            Self::RepeatingError { tool_name, count } => {
                write!(f, "Repeating {} {} times with errors", tool_name, count)
            }
            Self::AlternatingActions { tool_a, tool_b, cycles } => {
                write!(f, "Alternating {}/{} for {} cycles", tool_a, tool_b, cycles)
            }
        }
    }
}

/// Configurable thresholds for pattern detection.
#[derive(Debug, Clone)]
pub struct PatternThresholds {
    /// Minimum repetitions to trigger RepeatingAction (default: 4).
    pub repeating_action: usize,
    /// Minimum error repetitions to trigger RepeatingError (default: 3).
    pub repeating_error: usize,
    /// Minimum cycles to trigger AlternatingActions (default: 3, means 6 events).
    pub alternating_cycles: usize,
    /// Maximum events to scan (default: 20).
    pub window_size: usize,
}

impl Default for PatternThresholds {
    fn default() -> Self {
        Self {
            repeating_action: 4,
            repeating_error: 3,
            alternating_cycles: 3,
            window_size: 20,
        }
    }
}

/// Detects stuck patterns in tool call sequences.
#[derive(Debug, Clone, Default)]
pub struct PatternDetector {
    thresholds: PatternThresholds,
}

impl PatternDetector {
    /// Create a detector with default thresholds.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a detector with custom thresholds.
    #[must_use]
    pub fn with_thresholds(thresholds: PatternThresholds) -> Self {
        Self { thresholds }
    }

    /// Detect stuck patterns in recent tool calls.
    ///
    /// Returns the first detected pattern, or None if no patterns found.
    #[must_use]
    pub fn detect(&self, calls: &[ToolCallRecord]) -> Option<StuckPattern> {
        if calls.is_empty() {
            return None;
        }

        // Take only the most recent calls within window
        let window: Vec<_> = calls
            .iter()
            .rev()
            .take(self.thresholds.window_size)
            .collect();

        // Check for repeating errors first (more urgent)
        if let Some(pattern) = self.detect_repeating_errors(&window) {
            return Some(pattern);
        }

        // Check for repeating actions
        if let Some(pattern) = self.detect_repeating_actions(&window) {
            return Some(pattern);
        }

        // Check for alternating patterns
        if let Some(pattern) = self.detect_alternating(&window) {
            return Some(pattern);
        }

        None
    }

    fn detect_repeating_errors(&self, calls: &[&ToolCallRecord]) -> Option<StuckPattern> {
        if calls.len() < self.thresholds.repeating_error {
            return None;
        }

        let mut count = 1;
        let first = calls.first()?;

        if !first.is_error {
            return None;
        }

        for call in calls.iter().skip(1) {
            if call.is_error && self.calls_match(first, call) {
                count += 1;
            } else {
                break;
            }
        }

        if count >= self.thresholds.repeating_error {
            Some(StuckPattern::RepeatingError {
                tool_name: first.tool_name.clone(),
                count,
            })
        } else {
            None
        }
    }

    fn detect_repeating_actions(&self, calls: &[&ToolCallRecord]) -> Option<StuckPattern> {
        if calls.len() < self.thresholds.repeating_action {
            return None;
        }

        let mut count = 1;
        let first = calls.first()?;

        for call in calls.iter().skip(1) {
            if self.calls_match(first, call) {
                count += 1;
            } else {
                break;
            }
        }

        if count >= self.thresholds.repeating_action {
            Some(StuckPattern::RepeatingAction {
                tool_name: first.tool_name.clone(),
                count,
            })
        } else {
            None
        }
    }

    fn detect_alternating(&self, calls: &[&ToolCallRecord]) -> Option<StuckPattern> {
        let min_events = self.thresholds.alternating_cycles * 2;
        if calls.len() < min_events {
            return None;
        }

        // Check if calls[0] matches calls[2], calls[4], etc.
        // and calls[1] matches calls[3], calls[5], etc.
        let first = calls.first()?;
        let second = calls.get(1)?;

        if self.calls_match(first, second) {
            return None; // Same action, not alternating
        }

        let mut cycles = 1;
        let mut i = 2;

        while i + 1 < calls.len() {
            let a = calls.get(i)?;
            let b = calls.get(i + 1)?;

            if self.calls_match(first, a) && self.calls_match(second, b) {
                cycles += 1;
                i += 2;
            } else {
                break;
            }
        }

        if cycles >= self.thresholds.alternating_cycles {
            Some(StuckPattern::AlternatingActions {
                tool_a: first.tool_name.clone(),
                tool_b: second.tool_name.clone(),
                cycles,
            })
        } else {
            None
        }
    }

    /// Compare two calls semantically (tool name + input content).
    fn calls_match(&self, a: &ToolCallRecord, b: &ToolCallRecord) -> bool {
        a.tool_name == b.tool_name && a.input == b.input
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tool_call(name: &str, input: &str, result: &str, is_error: bool) -> ToolCallRecord {
        ToolCallRecord {
            tool_use_id: format!("id-{}", rand::random::<u32>()),
            tool_name: name.to_string(),
            input: serde_json::json!({ "content": input }),
            result: Some(serde_json::json!({ "output": result })),
            is_error,
            timestamp: "2026-01-30T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_no_pattern_with_varied_calls() {
        let detector = PatternDetector::default();
        let calls = vec![
            make_tool_call("Read", "file1.rs", "content1", false),
            make_tool_call("Edit", "file2.rs", "ok", false),
            make_tool_call("Bash", "cargo test", "passed", false),
        ];

        assert!(detector.detect(&calls).is_none());
    }

    #[test]
    fn test_detect_repeating_action_observation() {
        let detector = PatternDetector::default();
        let calls = vec![
            make_tool_call("Read", "file.rs", "content", false),
            make_tool_call("Read", "file.rs", "content", false),
            make_tool_call("Read", "file.rs", "content", false),
            make_tool_call("Read", "file.rs", "content", false),
        ];

        let pattern = detector.detect(&calls);
        assert!(pattern.is_some());
        assert!(matches!(pattern.unwrap(), StuckPattern::RepeatingAction { count: 4, .. }));
    }

    #[test]
    fn test_detect_repeating_errors() {
        let detector = PatternDetector::default();
        let calls = vec![
            make_tool_call("Bash", "cargo build", "error: missing", true),
            make_tool_call("Bash", "cargo build", "error: missing", true),
            make_tool_call("Bash", "cargo build", "error: missing", true),
        ];

        let pattern = detector.detect(&calls);
        assert!(pattern.is_some());
        assert!(matches!(pattern.unwrap(), StuckPattern::RepeatingError { count: 3, .. }));
    }

    #[test]
    fn test_detect_alternating_pattern() {
        let detector = PatternDetector::default();
        let calls = vec![
            make_tool_call("Read", "a.rs", "content", false),
            make_tool_call("Edit", "b.rs", "ok", false),
            make_tool_call("Read", "a.rs", "content", false),
            make_tool_call("Edit", "b.rs", "ok", false),
            make_tool_call("Read", "a.rs", "content", false),
            make_tool_call("Edit", "b.rs", "ok", false),
        ];

        let pattern = detector.detect(&calls);
        assert!(pattern.is_some());
        assert!(matches!(pattern.unwrap(), StuckPattern::AlternatingActions { cycles: 3, .. }));
    }

    #[test]
    fn test_errors_take_priority() {
        let detector = PatternDetector::default();
        // Both error pattern AND action pattern present, error wins
        let calls = vec![
            make_tool_call("Bash", "fail", "error", true),
            make_tool_call("Bash", "fail", "error", true),
            make_tool_call("Bash", "fail", "error", true),
            make_tool_call("Bash", "fail", "error", true),
        ];

        let pattern = detector.detect(&calls);
        assert!(matches!(pattern.unwrap(), StuckPattern::RepeatingError { .. }));
    }

    #[test]
    fn test_custom_thresholds() {
        let thresholds = PatternThresholds {
            repeating_action: 2, // Lower threshold
            repeating_error: 2,
            alternating_cycles: 2,
            window_size: 10,
        };
        let detector = PatternDetector::with_thresholds(thresholds);

        let calls = vec![
            make_tool_call("Read", "file.rs", "content", false),
            make_tool_call("Read", "file.rs", "content", false),
        ];

        // Would not trigger default (4), but triggers custom (2)
        assert!(detector.detect(&calls).is_some());
    }
}
```

**Step 4: Update mod.rs**

```rust
// Add to src/watcher/mod.rs after other mod declarations:
mod pattern;

// Add to pub use section:
pub use pattern::{PatternDetector, PatternThresholds, StuckPattern};
```

**Step 5: Verify pass**

Run: `cargo t pattern -p claude-supervisor --lib`

Expected: 6 tests PASS

**Step 6: Commit**

```bash
git add src/watcher/pattern.rs src/watcher/mod.rs
git commit -m "feat(watcher): add PatternDetector with stuck detection algorithms"
```

---

### Task 2.2: Integrate PatternDetector with SessionReconstructor

**Files:**
- Modify: `src/watcher/reconstructor.rs`

**Step 1: Write failing test**

```rust
// Add to src/watcher/reconstructor.rs tests:

#[test]
fn test_reconstructor_detects_stuck_pattern() {
    use crate::watcher::pattern::StuckPattern;

    let mut recon = SessionReconstructor::new();

    // Simulate 4 identical tool calls
    for i in 0..4 {
        let entry = create_assistant_entry_with_tool("Read", r#"{"path": "same.rs"}"#);
        recon.process_entry(entry);

        let result = create_user_entry_with_result(&format!("tool-{}", i), "same content");
        recon.process_entry(result);
    }

    let pattern = recon.detect_stuck_pattern();
    assert!(pattern.is_some());
    assert!(matches!(pattern.unwrap(), StuckPattern::RepeatingAction { .. }));
}

#[test]
fn test_reconstructor_no_pattern_when_varied() {
    let mut recon = SessionReconstructor::new();

    // Different tool calls
    let tools = [("Read", "file1.rs"), ("Edit", "file2.rs"), ("Bash", "cargo test")];
    for (i, (tool, input)) in tools.iter().enumerate() {
        let entry = create_assistant_entry_with_tool(tool, &format!(r#"{{"path": "{}"}}"#, input));
        recon.process_entry(entry);

        let result = create_user_entry_with_result(&format!("tool-{}", i), "ok");
        recon.process_entry(result);
    }

    assert!(recon.detect_stuck_pattern().is_none());
}
```

**Step 2: Verify failure**

Run: `cargo t reconstructor_detects -p claude-supervisor --lib`

Expected: FAIL with "no method named `detect_stuck_pattern`"

**Step 3: Implement**

```rust
// Add import at top of src/watcher/reconstructor.rs:
use super::pattern::{PatternDetector, StuckPattern};

// Add field to SessionReconstructor struct:
    /// Pattern detector for stuck detection.
    pattern_detector: PatternDetector,

// Update Default impl:
impl Default for SessionReconstructor {
    fn default() -> Self {
        Self {
            entries_by_uuid: HashMap::new(),
            tool_calls: Vec::new(),
            pending_tools: HashMap::new(),
            pattern_detector: PatternDetector::default(),
        }
    }
}

// Add methods to SessionReconstructor impl:

    /// Detect if the agent is stuck in a pattern.
    #[must_use]
    pub fn detect_stuck_pattern(&self) -> Option<StuckPattern> {
        self.pattern_detector.detect(&self.tool_calls)
    }

    /// Check if stuck with custom detector.
    #[must_use]
    pub fn detect_stuck_with(&self, detector: &PatternDetector) -> Option<StuckPattern> {
        detector.detect(&self.tool_calls)
    }
```

**Step 4: Add test helper functions to tests module**

```rust
// Add to tests module in reconstructor.rs:

fn create_assistant_entry_with_tool(tool_name: &str, input_json: &str) -> JournalEntry {
    let input: serde_json::Value = serde_json::from_str(input_json).unwrap();
    JournalEntry::Assistant(AssistantEntry {
        uuid: format!("assistant-{}", rand::random::<u32>()),
        parent_uuid: None,
        session_id: "test-session".to_string(),
        timestamp: "2026-01-30T00:00:00Z".to_string(),
        message: AssistantMessage {
            role: "assistant".to_string(),
            content: vec![ContentBlock::ToolUse {
                id: format!("tool-{}", rand::random::<u32>()),
                name: tool_name.to_string(),
                input,
            }],
        },
        cwd: "/tmp".to_string(),
        version: "1.0".to_string(),
        git_branch: None,
        is_sidechain: None,
    })
}

fn create_user_entry_with_result(tool_use_id: &str, content: &str) -> JournalEntry {
    JournalEntry::User(UserEntry {
        uuid: format!("user-{}", rand::random::<u32>()),
        parent_uuid: None,
        session_id: "test-session".to_string(),
        timestamp: "2026-01-30T00:00:00Z".to_string(),
        message: Message {
            role: "user".to_string(),
            content: vec![MessageContent::ToolResult {
                tool_use_id: tool_use_id.to_string(),
                content: content.to_string(),
                is_error: Some(false),
            }],
        },
        user_type: "tool_result".to_string(),
        cwd: "/tmp".to_string(),
        git_branch: None,
        version: "1.0".to_string(),
        is_sidechain: None,
        source_tool_use_id: Some(tool_use_id.to_string()),
        tool_use_result: None,
    })
}
```

**Step 5: Verify pass**

Run: `cargo t reconstructor -p claude-supervisor --lib`

Expected: All tests PASS

**Step 6: Commit**

```bash
git add src/watcher/reconstructor.rs
git commit -m "feat(watcher): integrate PatternDetector with SessionReconstructor"
```

---

## Batch 3: Auto-Continue Integration (#26)

**Goal:** Wire CompletionDetector and PatternDetector into handle_stop() decisions.

### Task 3.1: Add is_task_complete Method Using CompletionDetector

**Files:**
- Modify: `src/hooks/handler.rs`

**Step 1: Write failing test**

```rust
// Add to src/hooks/handler.rs tests:

#[test]
fn test_handle_stop_uses_completion_detector() {
    let policy = PolicyEngine::new();
    let config = StopConfig {
        force_continue: false,
        max_iterations: 10,
        completion_phrases: vec!["all done".to_string()],
        incomplete_phrases: vec!["next step".to_string()],
    };
    let handler = HookHandler::with_config(policy, config);

    // Incomplete transcript should block
    let incomplete_input = StopHookInput {
        session_id: "test".to_string(),
        stop_hook_active: Some(false),
        transcript_content: Some("I'll continue with next step...".to_string()),
    };

    let result = handler.should_continue(&incomplete_input);
    assert!(result.is_some()); // Some = should continue

    // Complete transcript should allow stop
    let complete_input = StopHookInput {
        session_id: "test".to_string(),
        stop_hook_active: Some(false),
        transcript_content: Some("all done with the task".to_string()),
    };

    let result = handler.should_continue(&complete_input);
    assert!(result.is_none()); // None = allow stop
}
```

**Step 2: Verify failure**

Run: `cargo t handle_stop_uses_completion -p claude-supervisor --lib`

Expected: FAIL with "no method named `should_continue`"

**Step 3: Implement**

```rust
// Add to HookHandler impl in src/hooks/handler.rs:

    /// Check if the task should continue based on transcript content.
    ///
    /// Returns Some(reason) if should continue, None if should allow stop.
    #[must_use]
    pub fn should_continue(&self, input: &StopHookInput) -> Option<String> {
        // Safety: if stop_hook_active, always allow stop
        if input.stop_hook_active == Some(true) {
            return None;
        }

        // Check transcript content for completion status
        if let Some(ref content) = input.transcript_content {
            match self.completion.analyze(content) {
                CompletionStatus::Incomplete(reason) => {
                    return Some(reason);
                }
                CompletionStatus::Complete => {
                    return None;
                }
                CompletionStatus::Unknown => {
                    // Fall through to other checks
                }
            }
        }

        // If force_continue is set and not at max iterations, continue
        if self.stop_config.force_continue {
            let iteration = self.iterations.get(&input.session_id);
            if iteration < self.stop_config.max_iterations {
                return Some("Force continue enabled".to_string());
            }
        }

        None
    }
```

**Step 4: Add StopHookInput struct if not exists**

```rust
// Add to src/hooks/input.rs or handler.rs:

/// Input for Stop hook decisions.
#[derive(Debug, Clone)]
pub struct StopHookInput {
    pub session_id: String,
    pub stop_hook_active: Option<bool>,
    pub transcript_content: Option<String>,
}
```

**Step 5: Add CompletionStatus enum to completion.rs**

```rust
// Add to src/hooks/completion.rs:

/// Result of analyzing transcript for completion.
#[derive(Debug, Clone, PartialEq)]
pub enum CompletionStatus {
    /// Task appears complete.
    Complete,
    /// Task appears incomplete with reason.
    Incomplete(String),
    /// Cannot determine completion status.
    Unknown,
}

impl CompletionDetector {
    /// Analyze transcript content for completion status.
    #[must_use]
    pub fn analyze(&self, content: &str) -> CompletionStatus {
        let lower = content.to_lowercase();

        // Check incomplete phrases first (they take priority)
        for phrase in &self.incomplete_phrases {
            if lower.contains(&phrase.to_lowercase()) {
                return CompletionStatus::Incomplete(format!("Found incomplete phrase: {}", phrase));
            }
        }

        // Check complete phrases
        for phrase in &self.complete_phrases {
            if lower.contains(&phrase.to_lowercase()) {
                return CompletionStatus::Complete;
            }
        }

        CompletionStatus::Unknown
    }
}
```

**Step 6: Verify pass**

Run: `cargo t completion -p claude-supervisor --lib`

Expected: All tests PASS

**Step 7: Commit**

```bash
git add src/hooks/handler.rs src/hooks/completion.rs src/hooks/input.rs
git commit -m "feat(hooks): add should_continue using CompletionDetector"
```

---

### Task 3.2: Integrate Pattern Detection into Stop Decision

**Files:**
- Modify: `src/hooks/handler.rs`

**Step 1: Write failing test**

```rust
// Add to src/hooks/handler.rs tests:

#[test]
fn test_handle_stop_detects_stuck_pattern() {
    use crate::watcher::{PatternDetector, ToolCallRecord, StuckPattern};

    let policy = PolicyEngine::new();
    let handler = HookHandler::new(policy);

    // Create stuck pattern (4 identical calls)
    let stuck_calls: Vec<ToolCallRecord> = (0..4)
        .map(|_| ToolCallRecord {
            tool_use_id: "id".to_string(),
            tool_name: "Read".to_string(),
            input: serde_json::json!({"path": "same.rs"}),
            result: Some(serde_json::json!("content")),
            is_error: false,
            timestamp: "2026-01-30T00:00:00Z".to_string(),
        })
        .collect();

    let pattern = handler.check_stuck_pattern(&stuck_calls);
    assert!(pattern.is_some());
    assert!(matches!(pattern.unwrap(), StuckPattern::RepeatingAction { .. }));
}

#[test]
fn test_stuck_pattern_allows_stop() {
    let policy = PolicyEngine::new();
    let config = StopConfig {
        force_continue: true, // Would normally continue
        max_iterations: 100,
        ..Default::default()
    };
    let handler = HookHandler::with_config(policy, config);

    let input = StopHookInput {
        session_id: "test".to_string(),
        stop_hook_active: Some(false),
        transcript_content: Some("next step...".to_string()), // Would normally continue
    };

    // But if stuck, should allow stop
    let stuck_calls = create_stuck_calls();
    let should_stop = handler.should_stop_when_stuck(&input, &stuck_calls);
    assert!(should_stop);
}
```

**Step 2: Verify failure**

Run: `cargo t stuck_pattern -p claude-supervisor --lib`

Expected: FAIL with "no method named `check_stuck_pattern`"

**Step 3: Implement**

```rust
// Add to HookHandler struct:
    pattern_detector: PatternDetector,

// Update constructors to initialize pattern_detector:
    pattern_detector: PatternDetector::default(),

// Add methods to HookHandler impl:

    /// Check for stuck patterns in tool call history.
    #[must_use]
    pub fn check_stuck_pattern(&self, calls: &[ToolCallRecord]) -> Option<StuckPattern> {
        self.pattern_detector.detect(calls)
    }

    /// Determine if should stop due to stuck pattern.
    ///
    /// Returns true if stuck and should allow stop regardless of other settings.
    #[must_use]
    pub fn should_stop_when_stuck(&self, input: &StopHookInput, calls: &[ToolCallRecord]) -> bool {
        // Safety always wins
        if input.stop_hook_active == Some(true) {
            return true;
        }

        // Check for stuck pattern
        if let Some(pattern) = self.check_stuck_pattern(calls) {
            tracing::warn!(
                session = %input.session_id,
                pattern = %pattern,
                "Stuck pattern detected, allowing stop"
            );
            return true;
        }

        false
    }

// Add combined decision method:

    /// Make the final stop/continue decision.
    ///
    /// Returns Ok(StopResponse) with the decision.
    pub fn decide_stop(
        &self,
        input: &StopHookInput,
        tool_calls: &[ToolCallRecord],
    ) -> StopResponse {
        // 1. Safety check: stop_hook_active
        if input.stop_hook_active == Some(true) {
            return StopResponse::allow();
        }

        // 2. Increment iteration count
        let iteration = self.iterations.increment(&input.session_id);

        // 3. Check max iterations
        if iteration >= self.stop_config.max_iterations {
            tracing::info!(
                session = %input.session_id,
                iteration,
                max = self.stop_config.max_iterations,
                "Max iterations reached, allowing stop"
            );
            return StopResponse::allow();
        }

        // 4. Check for stuck pattern
        if self.should_stop_when_stuck(input, tool_calls) {
            return StopResponse::allow();
        }

        // 5. Check completion status
        if let Some(reason) = self.should_continue(input) {
            return StopResponse::block(reason);
        }

        // 6. Default: allow stop
        StopResponse::allow()
    }
```

**Step 4: Add import for PatternDetector and ToolCallRecord**

```rust
// Add to imports at top of handler.rs:
use crate::watcher::{PatternDetector, StuckPattern, ToolCallRecord};
```

**Step 5: Verify pass**

Run: `cargo t stuck -p claude-supervisor --lib`

Expected: All tests PASS

**Step 6: Commit**

```bash
git add src/hooks/handler.rs
git commit -m "feat(hooks): integrate PatternDetector into stop decisions"
```

---

### Task 3.3: Add Watcher-Hooks Integration Channel

**Files:**
- Create: `src/integration/mod.rs`
- Create: `src/integration/bridge.rs`
- Modify: `src/lib.rs`

**Step 1: Write failing test**

```rust
// In src/integration/bridge.rs
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_watcher_hook_bridge_receives_events() {
        let (tx, mut rx) = mpsc::channel(10);
        let bridge = WatcherHookBridge::new(tx);

        // Simulate watcher event
        let event = WatcherEvent::NewEntry(Box::new(JournalEntry::Unknown));
        bridge.send(event.clone()).await.unwrap();

        let received = rx.recv().await;
        assert!(received.is_some());
    }

    #[tokio::test]
    async fn test_bridge_extracts_tool_calls() {
        let (tx, mut rx) = mpsc::channel(10);
        let bridge = WatcherHookBridge::new(tx);

        // Create entry with tool use
        let entry = create_test_assistant_entry_with_tool();
        bridge.process_entry(entry).await;

        let calls = bridge.get_recent_calls(10).await;
        assert!(!calls.is_empty());
    }
}
```

**Step 2: Verify failure**

Run: `cargo t watcher_hook_bridge -p claude-supervisor --lib`

Expected: FAIL with "cannot find module `integration`"

**Step 3: Implement**

```rust
// src/integration/mod.rs
//! Integration layer connecting watcher events to hook decisions.

mod bridge;

pub use bridge::WatcherHookBridge;
```

```rust
// src/integration/bridge.rs
//! Bridge between watcher events and hook handler.

use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

use crate::watcher::{JournalEntry, SessionReconstructor, ToolCallRecord, WatcherEvent};

/// Bridge connecting watcher events to hook decision making.
#[derive(Debug)]
pub struct WatcherHookBridge {
    /// Channel to send processed events.
    tx: mpsc::Sender<WatcherEvent>,
    /// Session state reconstructor.
    reconstructor: Arc<RwLock<SessionReconstructor>>,
}

impl WatcherHookBridge {
    /// Create a new bridge with the given event channel.
    #[must_use]
    pub fn new(tx: mpsc::Sender<WatcherEvent>) -> Self {
        Self {
            tx,
            reconstructor: Arc::new(RwLock::new(SessionReconstructor::new())),
        }
    }

    /// Send a watcher event through the bridge.
    pub async fn send(&self, event: WatcherEvent) -> Result<(), mpsc::error::SendError<WatcherEvent>> {
        self.tx.send(event).await
    }

    /// Process a journal entry, updating internal state.
    pub async fn process_entry(&self, entry: JournalEntry) {
        let mut recon = self.reconstructor.write().await;
        recon.process_entry(entry);
    }

    /// Get recent tool calls for pattern detection.
    pub async fn get_recent_calls(&self, n: usize) -> Vec<ToolCallRecord> {
        let recon = self.reconstructor.read().await;
        recon.recent_tool_calls(n).to_vec()
    }

    /// Check for stuck patterns.
    pub async fn detect_stuck(&self) -> Option<crate::watcher::StuckPattern> {
        let recon = self.reconstructor.read().await;
        recon.detect_stuck_pattern()
    }

    /// Get a clone of the reconstructor for external use.
    pub fn reconstructor(&self) -> Arc<RwLock<SessionReconstructor>> {
        Arc::clone(&self.reconstructor)
    }
}

impl Clone for WatcherHookBridge {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            reconstructor: Arc::clone(&self.reconstructor),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_watcher_hook_bridge_receives_events() {
        let (tx, mut rx) = mpsc::channel(10);
        let bridge = WatcherHookBridge::new(tx);

        let event = WatcherEvent::FileCreated(std::path::PathBuf::from("/tmp/test.jsonl"));
        bridge.send(event).await.unwrap();

        let received = rx.recv().await;
        assert!(received.is_some());
    }

    #[tokio::test]
    async fn test_bridge_process_entry() {
        let (tx, _rx) = mpsc::channel(10);
        let bridge = WatcherHookBridge::new(tx);

        // Process unknown entry (no crash)
        bridge.process_entry(JournalEntry::Unknown).await;

        let calls = bridge.get_recent_calls(10).await;
        assert!(calls.is_empty()); // Unknown entry has no tool calls
    }
}
```

**Step 4: Update lib.rs**

```rust
// Add to src/lib.rs:
pub mod integration;
```

**Step 5: Verify pass**

Run: `cargo t bridge -p claude-supervisor --lib`

Expected: All tests PASS

**Step 6: Commit**

```bash
git add src/integration/mod.rs src/integration/bridge.rs src/lib.rs
git commit -m "feat(integration): add WatcherHookBridge connecting watcher to hooks"
```

---

## Final Verification

After all batches complete, run full test suite:

```bash
cargo t -p claude-supervisor --lib
cargo clippy --all-targets --all-features -- -D warnings
```

Then create PR:

```bash
git push -u origin phase4-conversation-awareness
gh pr create --title "feat: Phase 4 conversation awareness (#24, #25, #26)" \
  --body "Implements subagent tracking, pattern detection, and auto-continue integration."
```
