//! Session state reconstructor.
//!
//! Reconstructs session state from journal entries, tracking tool calls
//! and their results.

use std::collections::HashMap;

use super::jsonl::{AssistantEntry, ContentBlock, JournalEntry, UserEntry};
use super::pattern::{PatternDetector, StuckPattern};

/// Record of a tool call with its result.
#[derive(Debug, Clone)]
pub struct ToolCallRecord {
    /// The tool use ID from the API.
    pub tool_use_id: String,
    /// Name of the tool that was called.
    pub tool_name: String,
    /// Input parameters to the tool.
    pub input: serde_json::Value,
    /// Result of the tool call, if available.
    pub result: Option<serde_json::Value>,
    /// Whether the tool call resulted in an error.
    pub is_error: bool,
    /// Timestamp when the tool was called.
    pub timestamp: String,
}

/// Reconstructs session state from journal entries.
///
/// Tracks all entries by UUID, correlates tool calls with their results,
/// and maintains a timeline of tool executions.
#[derive(Debug, Default)]
pub struct SessionReconstructor {
    /// All entries indexed by UUID.
    entries_by_uuid: HashMap<String, JournalEntry>,
    /// Completed tool calls with results.
    tool_calls: Vec<ToolCallRecord>,
    /// Pending tool calls awaiting results.
    pending_tools: HashMap<String, ToolCallRecord>,
}

impl SessionReconstructor {
    /// Create a new empty reconstructor.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a single journal entry.
    ///
    /// Updates internal state based on the entry type:
    /// - Assistant entries: extracts tool use requests
    /// - User entries with tool results: matches with pending tool calls
    pub fn process_entry(&mut self, entry: &JournalEntry) {
        // Store entry by UUID
        let uuid = Self::extract_uuid(entry);
        if let Some(uuid) = uuid {
            self.entries_by_uuid.insert(uuid.clone(), entry.clone());
        }

        match entry {
            JournalEntry::Assistant(assistant) => {
                self.process_assistant_entry(assistant);
            }
            JournalEntry::User(user) => {
                self.process_user_entry(user);
            }
            _ => {}
        }
    }

    /// Process multiple entries in order.
    pub fn process_entries<'a>(&mut self, entries: impl IntoIterator<Item = &'a JournalEntry>) {
        for entry in entries {
            self.process_entry(entry);
        }
    }

    /// Get all completed tool calls.
    #[must_use]
    pub fn tool_calls(&self) -> &[ToolCallRecord] {
        &self.tool_calls
    }

    /// Get pending tool calls that haven't received results yet.
    #[must_use]
    pub fn pending_tool_calls(&self) -> Vec<&ToolCallRecord> {
        self.pending_tools.values().collect()
    }

    /// Get an entry by its UUID.
    #[must_use]
    pub fn get_entry(&self, uuid: &str) -> Option<&JournalEntry> {
        self.entries_by_uuid.get(uuid)
    }

    /// Get the total number of entries processed.
    #[must_use]
    pub fn entry_count(&self) -> usize {
        self.entries_by_uuid.len()
    }

    /// Get the most recent N tool calls.
    #[must_use]
    pub fn recent_tool_calls(&self, n: usize) -> Vec<&ToolCallRecord> {
        let len = self.tool_calls.len();
        let start = len.saturating_sub(n);
        self.tool_calls[start..].iter().collect()
    }

    /// Clear all state, resetting the reconstructor.
    pub fn clear(&mut self) {
        self.entries_by_uuid.clear();
        self.tool_calls.clear();
        self.pending_tools.clear();
    }

    /// Detect stuck patterns in the tool call history.
    ///
    /// Uses the provided `PatternDetector` to analyze recent tool calls
    /// for signs of repetitive or stuck behavior.
    #[must_use]
    pub fn detect_stuck_pattern(&self, detector: &PatternDetector) -> Option<StuckPattern> {
        detector.detect(&self.tool_calls)
    }

    /// Extract UUID from any journal entry type.
    fn extract_uuid(entry: &JournalEntry) -> Option<String> {
        match entry {
            JournalEntry::User(u) => Some(u.uuid.clone()),
            JournalEntry::Assistant(a) => Some(a.uuid.clone()),
            JournalEntry::Progress(p) => Some(p.uuid.clone()),
            JournalEntry::System(s) => Some(s.uuid.clone()),
            JournalEntry::Summary(s) => Some(s.leaf_uuid.clone()),
            JournalEntry::FileHistorySnapshot(f) => Some(f.message_id.clone()),
            JournalEntry::QueueOperation(_) | JournalEntry::Unknown => None,
        }
    }

    /// Process an assistant entry to extract tool use requests.
    fn process_assistant_entry(&mut self, assistant: &AssistantEntry) {
        for block in &assistant.message.content {
            if let ContentBlock::ToolUse { id, name, input } = block {
                let record = ToolCallRecord {
                    tool_use_id: id.clone(),
                    tool_name: name.clone(),
                    input: input.clone(),
                    result: None,
                    is_error: false,
                    timestamp: assistant.timestamp.clone(),
                };
                self.pending_tools.insert(id.clone(), record);
            }
        }
    }

    /// Process a user entry to match tool results with pending calls.
    fn process_user_entry(&mut self, user: &UserEntry) {
        // Check if this is a tool result
        if let Some(ref tool_use_id) = user.source_tool_use_id {
            if let Some(mut record) = self.pending_tools.remove(tool_use_id) {
                // Update record with result
                record.result.clone_from(&user.tool_use_result);
                // Check if the result indicates an error
                if let Some(ref result) = record.result {
                    record.is_error = result
                        .get("is_error")
                        .is_some_and(|v| v.as_bool() == Some(true));
                }
                self.tool_calls.push(record);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_assistant_with_tool_use(uuid: &str, tool_id: &str, tool_name: &str) -> String {
        format!(
            r#"{{"type":"assistant","uuid":"{uuid}","parentUuid":"parent-1","sessionId":"sess-1","timestamp":"2026-01-29T10:00:00Z","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"{tool_id}","name":"{tool_name}","input":{{"path":"/tmp/test.txt"}}}}]}},"cwd":"/tmp","version":"2.1.25"}}"#
        )
    }

    fn create_user_with_tool_result(uuid: &str, tool_id: &str, result: &str) -> String {
        format!(
            r#"{{"type":"user","uuid":"{uuid}","parentUuid":"parent-1","sessionId":"sess-1","timestamp":"2026-01-29T10:00:01Z","message":{{"role":"user","content":"Tool result"}},"userType":"tool_result","cwd":"/tmp","version":"2.1.25","sourceToolUseId":"{tool_id}","toolUseResult":{result}}}"#
        )
    }

    fn create_simple_user(uuid: &str) -> String {
        format!(
            r#"{{"type":"user","uuid":"{uuid}","parentUuid":null,"sessionId":"sess-1","timestamp":"2026-01-29T10:00:00Z","message":{{"role":"user","content":"Hello"}},"userType":"external","cwd":"/tmp","version":"2.1.25"}}"#
        )
    }

    #[test]
    fn test_new_reconstructor_is_empty() {
        let reconstructor = SessionReconstructor::new();
        assert_eq!(reconstructor.entry_count(), 0);
        assert!(reconstructor.tool_calls().is_empty());
        assert!(reconstructor.pending_tool_calls().is_empty());
    }

    #[test]
    fn test_process_user_entry() {
        let mut reconstructor = SessionReconstructor::new();
        let json = create_simple_user("uuid-1");
        let entry: JournalEntry = serde_json::from_str(&json).unwrap();

        reconstructor.process_entry(&entry);

        assert_eq!(reconstructor.entry_count(), 1);
        assert!(reconstructor.get_entry("uuid-1").is_some());
    }

    #[test]
    fn test_process_assistant_with_tool_use() {
        let mut reconstructor = SessionReconstructor::new();
        let json = create_assistant_with_tool_use("uuid-1", "tool-1", "Read");
        let entry: JournalEntry = serde_json::from_str(&json).unwrap();

        reconstructor.process_entry(&entry);

        assert_eq!(reconstructor.entry_count(), 1);
        assert_eq!(reconstructor.pending_tool_calls().len(), 1);
        assert!(reconstructor.tool_calls().is_empty());

        let pending = &reconstructor.pending_tool_calls()[0];
        assert_eq!(pending.tool_use_id, "tool-1");
        assert_eq!(pending.tool_name, "Read");
    }

    #[test]
    fn test_tool_call_matched_with_result() {
        let mut reconstructor = SessionReconstructor::new();

        // Process assistant entry with tool use
        let assistant_json = create_assistant_with_tool_use("uuid-1", "tool-1", "Read");
        let assistant_entry: JournalEntry = serde_json::from_str(&assistant_json).unwrap();
        reconstructor.process_entry(&assistant_entry);

        assert_eq!(reconstructor.pending_tool_calls().len(), 1);

        // Process user entry with tool result
        let user_json =
            create_user_with_tool_result("uuid-2", "tool-1", r#"{"content":"file contents"}"#);
        let user_entry: JournalEntry = serde_json::from_str(&user_json).unwrap();
        reconstructor.process_entry(&user_entry);

        // Tool call should now be completed
        assert!(reconstructor.pending_tool_calls().is_empty());
        assert_eq!(reconstructor.tool_calls().len(), 1);

        let completed = &reconstructor.tool_calls()[0];
        assert_eq!(completed.tool_use_id, "tool-1");
        assert_eq!(completed.tool_name, "Read");
        assert!(completed.result.is_some());
        assert!(!completed.is_error);
    }

    #[test]
    fn test_tool_call_with_error_result() {
        let mut reconstructor = SessionReconstructor::new();

        let assistant_json = create_assistant_with_tool_use("uuid-1", "tool-1", "Bash");
        let assistant_entry: JournalEntry = serde_json::from_str(&assistant_json).unwrap();
        reconstructor.process_entry(&assistant_entry);

        let user_json = create_user_with_tool_result(
            "uuid-2",
            "tool-1",
            r#"{"is_error":true,"error":"command failed"}"#,
        );
        let user_entry: JournalEntry = serde_json::from_str(&user_json).unwrap();
        reconstructor.process_entry(&user_entry);

        assert_eq!(reconstructor.tool_calls().len(), 1);
        let completed = &reconstructor.tool_calls()[0];
        assert!(completed.is_error);
    }

    #[test]
    fn test_process_entries_batch() {
        let mut reconstructor = SessionReconstructor::new();

        let entries: Vec<JournalEntry> = vec![
            serde_json::from_str(&create_simple_user("uuid-1")).unwrap(),
            serde_json::from_str(&create_assistant_with_tool_use("uuid-2", "tool-1", "Edit"))
                .unwrap(),
            serde_json::from_str(&create_user_with_tool_result(
                "uuid-3",
                "tool-1",
                r#"{"success":true}"#,
            ))
            .unwrap(),
        ];

        reconstructor.process_entries(&entries);

        assert_eq!(reconstructor.entry_count(), 3);
        assert_eq!(reconstructor.tool_calls().len(), 1);
        assert!(reconstructor.pending_tool_calls().is_empty());
    }

    #[test]
    fn test_multiple_pending_tools() {
        let mut reconstructor = SessionReconstructor::new();

        // Two tool uses in sequence
        let assistant1: JournalEntry =
            serde_json::from_str(&create_assistant_with_tool_use("uuid-1", "tool-1", "Read"))
                .unwrap();
        let assistant2: JournalEntry =
            serde_json::from_str(&create_assistant_with_tool_use("uuid-2", "tool-2", "Glob"))
                .unwrap();

        reconstructor.process_entry(&assistant1);
        reconstructor.process_entry(&assistant2);

        assert_eq!(reconstructor.pending_tool_calls().len(), 2);

        // Complete only one
        let result: JournalEntry = serde_json::from_str(&create_user_with_tool_result(
            "uuid-3",
            "tool-1",
            r#"{"data":"test"}"#,
        ))
        .unwrap();
        reconstructor.process_entry(&result);

        assert_eq!(reconstructor.pending_tool_calls().len(), 1);
        assert_eq!(reconstructor.tool_calls().len(), 1);
    }
}
