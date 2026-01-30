//! JSONL parser for Claude Code conversation files.
//!
//! Parses `~/.claude/projects/<hash>/*.jsonl` session files.

use serde::Deserialize;

/// A single entry in a Claude Code JSONL conversation file.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum JournalEntry {
    /// User message or tool result
    User(UserEntry),
    /// Assistant response
    Assistant(AssistantEntry),
    /// Progress update (MCP/hook)
    Progress(ProgressEntry),
    /// System message
    System(SystemEntry),
    /// File backup snapshot
    FileHistorySnapshot(FileSnapshotEntry),
    /// Session summary
    Summary(SummaryEntry),
    /// Queue operation (headless mode)
    QueueOperation(QueueOperationEntry),
    /// Unknown entry type (forward compatibility)
    #[serde(other)]
    Unknown,
}

/// User message entry.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserEntry {
    pub uuid: String,
    pub parent_uuid: Option<String>,
    pub session_id: String,
    pub timestamp: String,
    pub message: Message,
    pub user_type: String,
    pub cwd: String,
    #[serde(default)]
    pub git_branch: Option<String>,
    pub version: String,
    #[serde(default)]
    pub is_sidechain: Option<bool>,
    #[serde(default)]
    pub source_tool_use_id: Option<String>,
    #[serde(default)]
    pub tool_use_result: Option<serde_json::Value>,
}

/// Assistant message entry.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantEntry {
    pub uuid: String,
    pub parent_uuid: Option<String>,
    pub session_id: String,
    pub timestamp: String,
    pub message: AssistantMessage,
    pub cwd: String,
    pub version: String,
    #[serde(default)]
    pub git_branch: Option<String>,
    #[serde(default)]
    pub is_sidechain: Option<bool>,
}

/// Progress entry for MCP/hook updates.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEntry {
    pub uuid: String,
    #[serde(default)]
    pub tool_use_id: Option<String>,
    pub data: serde_json::Value,
}

/// System message entry.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemEntry {
    pub uuid: String,
    pub subtype: String,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

/// File history snapshot entry.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSnapshotEntry {
    pub message_id: String,
    pub snapshot: serde_json::Value,
}

/// Session summary entry.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryEntry {
    pub summary: String,
    pub leaf_uuid: String,
}

/// Queue operation entry (headless mode).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueOperationEntry {
    pub operation: String,
    pub timestamp: String,
    pub session_id: String,
}

/// A message with role and content.
#[derive(Debug, Clone, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: MessageContent,
}

/// Assistant message with model info.
#[derive(Debug, Clone, Deserialize)]
pub struct AssistantMessage {
    pub role: String,
    pub content: Vec<ContentBlock>,
    #[serde(default)]
    pub model: Option<String>,
}

/// Message content - can be plain text or structured blocks.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// Plain text content
    Text(String),
    /// Structured content blocks
    Blocks(Vec<ContentBlock>),
}

/// A content block within a message.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// Text content
    Text { text: String },
    /// Tool use request
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// Tool result
    ToolResult {
        tool_use_id: String,
        content: serde_json::Value,
    },
    /// Thinking block
    Thinking { thinking: String },
    /// Unknown block type
    #[serde(other)]
    Unknown,
}

/// Parse JSONL content into journal entries.
///
/// Skips malformed lines with a warning.
#[must_use]
pub fn parse_jsonl_content(content: &str) -> Vec<JournalEntry> {
    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| match serde_json::from_str::<JournalEntry>(line) {
            Ok(entry) => Some(entry),
            Err(e) => {
                tracing::warn!("Failed to parse JSONL line: {}", e);
                None
            }
        })
        .collect()
}

/// Parse a JSONL file from disk.
///
/// # Errors
///
/// Returns an error if the file cannot be read.
pub async fn parse_jsonl_file(path: &std::path::Path) -> std::io::Result<Vec<JournalEntry>> {
    let content = tokio::fs::read_to_string(path).await?;
    Ok(parse_jsonl_content(&content))
}

/// Extract text content from a message.
impl MessageContent {
    /// Get the text content as a string.
    #[must_use]
    pub fn as_text(&self) -> String {
        match self {
            MessageContent::Text(s) => s.clone(),
            MessageContent::Blocks(blocks) => blocks
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_user_entry() {
        let json = r#"{"type":"user","uuid":"abc-123","parentUuid":null,"sessionId":"sess-1","timestamp":"2026-01-29T10:00:00Z","message":{"role":"user","content":"Hello world"},"userType":"external","cwd":"/tmp","version":"2.1.25"}"#;

        let entry: JournalEntry = serde_json::from_str(json).unwrap();

        match entry {
            JournalEntry::User(u) => {
                assert_eq!(u.uuid, "abc-123");
                assert_eq!(u.session_id, "sess-1");
            }
            _ => panic!("Expected User entry"),
        }
    }

    #[test]
    fn test_parse_assistant_entry() {
        let json = r#"{"type":"assistant","uuid":"def-456","parentUuid":"abc-123","sessionId":"sess-1","timestamp":"2026-01-29T10:00:01Z","message":{"role":"assistant","content":[{"type":"text","text":"Hi there!"}]},"cwd":"/tmp","version":"2.1.25"}"#;

        let entry: JournalEntry = serde_json::from_str(json).unwrap();

        match entry {
            JournalEntry::Assistant(a) => {
                assert_eq!(a.uuid, "def-456");
                assert_eq!(a.parent_uuid, Some("abc-123".to_string()));
            }
            _ => panic!("Expected Assistant entry"),
        }
    }

    #[test]
    fn test_parse_content_as_string() {
        let json = r#"{"role":"user","content":"plain text"}"#;
        let msg: Message = serde_json::from_str(json).unwrap();

        match msg.content {
            MessageContent::Text(s) => assert_eq!(s, "plain text"),
            MessageContent::Blocks(_) => panic!("Expected Text content"),
        }
    }

    #[test]
    fn test_parse_content_as_blocks() {
        let json = r#"{"role":"assistant","content":[{"type":"text","text":"hello"}]}"#;
        let msg: Message = serde_json::from_str(json).unwrap();

        match msg.content {
            MessageContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ContentBlock::Text { text } => assert_eq!(text, "hello"),
                    _ => panic!("Expected Text block"),
                }
            }
            MessageContent::Text(_) => panic!("Expected Blocks content"),
        }
    }

    #[test]
    fn test_parse_unknown_entry_type() {
        let json = r#"{"type":"future-type","data":"something"}"#;
        let entry: JournalEntry = serde_json::from_str(json).unwrap();

        match entry {
            JournalEntry::Unknown => {}
            _ => panic!("Expected Unknown entry"),
        }
    }

    #[test]
    fn test_parse_jsonl_file() {
        let jsonl = r#"{"type":"user","uuid":"1","parentUuid":null,"sessionId":"s","timestamp":"2026-01-29T10:00:00Z","message":{"role":"user","content":"Q1"},"userType":"external","cwd":"/tmp","version":"2.1.25"}
{"type":"assistant","uuid":"2","parentUuid":"1","sessionId":"s","timestamp":"2026-01-29T10:00:01Z","message":{"role":"assistant","content":[{"type":"text","text":"A1"}]},"cwd":"/tmp","version":"2.1.25"}
invalid json line
{"type":"summary","summary":"Test session","leafUuid":"2"}"#;

        let entries = parse_jsonl_content(jsonl);

        assert_eq!(entries.len(), 3); // Skips invalid line
    }

    #[test]
    fn test_message_content_as_text() {
        let text_content = MessageContent::Text("hello".to_string());
        assert_eq!(text_content.as_text(), "hello");

        let blocks_content = MessageContent::Blocks(vec![
            ContentBlock::Text {
                text: "line1".to_string(),
            },
            ContentBlock::Text {
                text: "line2".to_string(),
            },
        ]);
        assert_eq!(blocks_content.as_text(), "line1\nline2");
    }
}
