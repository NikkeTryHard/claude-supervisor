# Phase 3: Knowledge Layer Implementation Plan

> **REQUIRED:** Use `execute-plan` to implement this plan batch by batch.

**Goal:** Enable the supervisor boss to answer worker questions from project knowledge (CLAUDE.md, session history, memory).

**Architecture:** Three knowledge sources unified via `KnowledgeSource` trait feeding into `KnowledgeAggregator`. JSONL parser in `watcher/` module provides foundation for session history. Async trait for file I/O operations.

**Tech Stack:** comrak (markdown AST), serde (JSONL parsing), async-trait, tokio::fs

**Issues:** #39 (CLAUDE.md), #22 (JSONL parser), #40 (Session history)

---

## Batch 1: Knowledge Layer Foundation + All Sources

**Goal:** Implement complete knowledge layer with JSONL parser, CLAUDE.md loader, and session history reader in one batch.

### Task 1.1: JSONL Parser Types and Entry Parsing

**Files:**
- Create: `src/watcher/mod.rs`
- Create: `src/watcher/jsonl.rs`
- Modify: `src/lib.rs` (add `pub mod watcher;`)

**Step 1: Write failing test**

```rust
// src/watcher/jsonl.rs
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
            _ => panic!("Expected Text content"),
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
            _ => panic!("Expected Blocks content"),
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
}
```

**Step 2: Verify failure**

Run: `cargo test -p claude-supervisor watcher::jsonl --no-run 2>&1 | head -20`

Expected: Compilation error - module `watcher` not found

**Step 3: Implement**

```rust
// src/watcher/mod.rs
mod jsonl;

pub use jsonl::*;
```

```rust
// src/watcher/jsonl.rs
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
    Text {
        text: String,
    },
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
    Thinking {
        thinking: String,
    },
    /// Unknown block type
    #[serde(other)]
    Unknown,
}

/// Parse JSONL content into journal entries.
///
/// Skips malformed lines with a warning.
pub fn parse_jsonl_content(content: &str) -> Vec<JournalEntry> {
    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| {
            match serde_json::from_str::<JournalEntry>(line) {
                Ok(entry) => Some(entry),
                Err(e) => {
                    tracing::warn!("Failed to parse JSONL line: {}", e);
                    None
                }
            }
        })
        .collect()
}

/// Parse a JSONL file from disk.
pub async fn parse_jsonl_file(path: &std::path::Path) -> std::io::Result<Vec<JournalEntry>> {
    let content = tokio::fs::read_to_string(path).await?;
    Ok(parse_jsonl_content(&content))
}

/// Extract text content from a message.
impl MessageContent {
    /// Get the text content as a string.
    pub fn as_text(&self) -> String {
        match self {
            MessageContent::Text(s) => s.clone(),
            MessageContent::Blocks(blocks) => {
                blocks
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
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
            _ => panic!("Expected Text content"),
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
            _ => panic!("Expected Blocks content"),
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
            ContentBlock::Text { text: "line1".to_string() },
            ContentBlock::Text { text: "line2".to_string() },
        ]);
        assert_eq!(blocks_content.as_text(), "line1\nline2");
    }
}
```

**Step 4: Verify pass**

Run: `cargo test -p claude-supervisor watcher::jsonl -- --nocapture`

Expected: All 7 tests pass

**Step 5: Commit**

```bash
git add src/watcher/ src/lib.rs
git commit -m "feat(watcher): add JSONL parser for Claude Code conversation files

Implements issue #22 with support for all 7 entry types:
- user, assistant, progress, system
- file-history-snapshot, summary, queue-operation
- Handles content as String or Vec<ContentBlock>
- Graceful handling of malformed lines"
```

---

### Task 1.2: Knowledge Source Trait and Aggregator

**Files:**
- Create: `src/knowledge/mod.rs`
- Create: `src/knowledge/source.rs`
- Modify: `src/lib.rs` (add `pub mod knowledge;`)

**Step 1: Write failing test**

```rust
// src/knowledge/source.rs
#[cfg(test)]
mod tests {
    use super::*;

    struct MockSource {
        name: &'static str,
        response: Option<String>,
    }

    impl KnowledgeSource for MockSource {
        fn source_name(&self) -> &str {
            self.name
        }

        fn query(&self, _question: &str) -> Option<KnowledgeFact> {
            self.response.as_ref().map(|r| KnowledgeFact {
                source: self.name.to_string(),
                content: r.clone(),
                relevance: 1.0,
            })
        }

        fn context_summary(&self) -> Option<String> {
            self.response.clone()
        }
    }

    #[test]
    fn test_aggregator_queries_all_sources() {
        let mut agg = KnowledgeAggregator::new();
        agg.add_source(Box::new(MockSource {
            name: "source1",
            response: Some("fact1".to_string()),
        }));
        agg.add_source(Box::new(MockSource {
            name: "source2",
            response: Some("fact2".to_string()),
        }));

        let facts = agg.query("test question");

        assert_eq!(facts.len(), 2);
        assert!(facts.iter().any(|f| f.content == "fact1"));
        assert!(facts.iter().any(|f| f.content == "fact2"));
    }

    #[test]
    fn test_aggregator_skips_empty_sources() {
        let mut agg = KnowledgeAggregator::new();
        agg.add_source(Box::new(MockSource {
            name: "empty",
            response: None,
        }));
        agg.add_source(Box::new(MockSource {
            name: "full",
            response: Some("fact".to_string()),
        }));

        let facts = agg.query("test");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].source, "full");
    }

    #[test]
    fn test_aggregator_builds_context() {
        let mut agg = KnowledgeAggregator::new();
        agg.add_source(Box::new(MockSource {
            name: "rules",
            response: Some("Use Rust".to_string()),
        }));
        agg.add_source(Box::new(MockSource {
            name: "history",
            response: Some("Previously decided X".to_string()),
        }));

        let context = agg.build_context();

        assert!(context.contains("Use Rust"));
        assert!(context.contains("Previously decided X"));
    }
}
```

**Step 2: Verify failure**

Run: `cargo test -p claude-supervisor knowledge::source --no-run 2>&1 | head -20`

Expected: Compilation error - module `knowledge` not found

**Step 3: Implement**

```rust
// src/knowledge/mod.rs
//! Knowledge layer for supervisor decision-making.
//!
//! Provides unified access to project knowledge from multiple sources:
//! - CLAUDE.md (project conventions)
//! - Session history (past Q&A)
//! - Memory file (learned facts)

mod source;
mod claude_md;
mod history;

pub use source::*;
pub use claude_md::*;
pub use history::*;
```

```rust
// src/knowledge/source.rs
//! Knowledge source trait and aggregator.

/// A fact retrieved from a knowledge source.
#[derive(Debug, Clone)]
pub struct KnowledgeFact {
    /// Which source provided this fact
    pub source: String,
    /// The fact content
    pub content: String,
    /// Relevance score (0.0 - 1.0)
    pub relevance: f32,
}

/// A source of knowledge for the supervisor.
pub trait KnowledgeSource: Send + Sync {
    /// Unique name for this source.
    fn source_name(&self) -> &str;

    /// Query for facts relevant to a question.
    fn query(&self, question: &str) -> Option<KnowledgeFact>;

    /// Get a summary of all knowledge for context building.
    fn context_summary(&self) -> Option<String>;
}

/// Aggregates multiple knowledge sources.
pub struct KnowledgeAggregator {
    sources: Vec<Box<dyn KnowledgeSource>>,
}

impl KnowledgeAggregator {
    /// Create a new empty aggregator.
    pub fn new() -> Self {
        Self { sources: Vec::new() }
    }

    /// Add a knowledge source.
    pub fn add_source(&mut self, source: Box<dyn KnowledgeSource>) {
        self.sources.push(source);
    }

    /// Query all sources for facts relevant to a question.
    pub fn query(&self, question: &str) -> Vec<KnowledgeFact> {
        self.sources
            .iter()
            .filter_map(|s| s.query(question))
            .collect()
    }

    /// Build a context string from all sources for the boss prompt.
    pub fn build_context(&self) -> String {
        let mut parts = Vec::new();

        for source in &self.sources {
            if let Some(summary) = source.context_summary() {
                parts.push(format!("## {}\n\n{}", source.source_name(), summary));
            }
        }

        parts.join("\n\n---\n\n")
    }

    /// Check if any knowledge is available.
    pub fn has_knowledge(&self) -> bool {
        self.sources.iter().any(|s| s.context_summary().is_some())
    }
}

impl Default for KnowledgeAggregator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockSource {
        name: &'static str,
        response: Option<String>,
    }

    impl KnowledgeSource for MockSource {
        fn source_name(&self) -> &str {
            self.name
        }

        fn query(&self, _question: &str) -> Option<KnowledgeFact> {
            self.response.as_ref().map(|r| KnowledgeFact {
                source: self.name.to_string(),
                content: r.clone(),
                relevance: 1.0,
            })
        }

        fn context_summary(&self) -> Option<String> {
            self.response.clone()
        }
    }

    #[test]
    fn test_aggregator_queries_all_sources() {
        let mut agg = KnowledgeAggregator::new();
        agg.add_source(Box::new(MockSource {
            name: "source1",
            response: Some("fact1".to_string()),
        }));
        agg.add_source(Box::new(MockSource {
            name: "source2",
            response: Some("fact2".to_string()),
        }));

        let facts = agg.query("test question");

        assert_eq!(facts.len(), 2);
        assert!(facts.iter().any(|f| f.content == "fact1"));
        assert!(facts.iter().any(|f| f.content == "fact2"));
    }

    #[test]
    fn test_aggregator_skips_empty_sources() {
        let mut agg = KnowledgeAggregator::new();
        agg.add_source(Box::new(MockSource {
            name: "empty",
            response: None,
        }));
        agg.add_source(Box::new(MockSource {
            name: "full",
            response: Some("fact".to_string()),
        }));

        let facts = agg.query("test");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].source, "full");
    }

    #[test]
    fn test_aggregator_builds_context() {
        let mut agg = KnowledgeAggregator::new();
        agg.add_source(Box::new(MockSource {
            name: "rules",
            response: Some("Use Rust".to_string()),
        }));
        agg.add_source(Box::new(MockSource {
            name: "history",
            response: Some("Previously decided X".to_string()),
        }));

        let context = agg.build_context();

        assert!(context.contains("Use Rust"));
        assert!(context.contains("Previously decided X"));
    }

    #[test]
    fn test_aggregator_has_knowledge() {
        let mut agg = KnowledgeAggregator::new();
        assert!(!agg.has_knowledge());

        agg.add_source(Box::new(MockSource {
            name: "empty",
            response: None,
        }));
        assert!(!agg.has_knowledge());

        agg.add_source(Box::new(MockSource {
            name: "full",
            response: Some("fact".to_string()),
        }));
        assert!(agg.has_knowledge());
    }
}
```

**Step 4: Verify pass**

Run: `cargo test -p claude-supervisor knowledge::source -- --nocapture`

Expected: All 4 tests pass

**Step 5: Commit**

```bash
git add src/knowledge/mod.rs src/knowledge/source.rs src/lib.rs
git commit -m "feat(knowledge): add KnowledgeSource trait and aggregator

Foundation for Phase 3 knowledge layer:
- KnowledgeSource trait for unified querying
- KnowledgeFact for structured responses
- KnowledgeAggregator for multi-source context building"
```

---

### Task 1.3: CLAUDE.md Loader

**Files:**
- Create: `src/knowledge/claude_md.rs`
- Modify: `Cargo.toml` (add `comrak` dependency)

**Step 1: Write failing test**

```rust
// src/knowledge/claude_md.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sections() {
        let markdown = r#"# Project

## Commands

```bash
cargo test
```

## Conventions

- Use snake_case
- Prefer Result over panic

## Resources

Some links here.
"#;

        let source = ClaudeMdSource::from_content(markdown);

        assert!(source.sections.contains_key("Commands"));
        assert!(source.sections.contains_key("Conventions"));
        assert!(source.sections["Conventions"].contains("snake_case"));
    }

    #[test]
    fn test_empty_file() {
        let source = ClaudeMdSource::from_content("");
        assert!(source.sections.is_empty());
    }

    #[test]
    fn test_no_headers() {
        let markdown = "Just some plain text without headers.";
        let source = ClaudeMdSource::from_content(markdown);

        // Should capture as unnamed section or be empty
        assert!(source.sections.is_empty() || source.sections.contains_key(""));
    }

    #[test]
    fn test_query_finds_relevant_section() {
        let markdown = r#"## Error Handling

Use thiserror for error types.

## Testing

Use cargo nextest.
"#;

        let source = ClaudeMdSource::from_content(markdown);
        let fact = source.query("how to handle errors");

        assert!(fact.is_some());
        let fact = fact.unwrap();
        assert!(fact.content.contains("thiserror"));
    }

    #[test]
    fn test_context_summary() {
        let markdown = r#"## Commands

cargo test

## Style

Use rustfmt
"#;

        let source = ClaudeMdSource::from_content(markdown);
        let summary = source.context_summary();

        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert!(summary.contains("Commands"));
        assert!(summary.contains("cargo test"));
    }
}
```

**Step 2: Verify failure**

Run: `cargo test -p claude-supervisor knowledge::claude_md --no-run 2>&1 | head -20`

Expected: Compilation error - `ClaudeMdSource` not found

**Step 3: Implement**

First, add comrak to Cargo.toml:

```toml
# In [dependencies] section
comrak = { version = "0.28", default-features = false }
```

```rust
// src/knowledge/claude_md.rs
//! CLAUDE.md file parser and knowledge source.
//!
//! Loads project conventions from CLAUDE.md files using comrak for
//! AST-based section extraction.

use std::collections::HashMap;
use std::path::Path;

use comrak::nodes::{AstNode, NodeValue};
use comrak::{parse_document, Arena, Options};

use super::source::{KnowledgeFact, KnowledgeSource};

/// Knowledge source backed by CLAUDE.md file(s).
pub struct ClaudeMdSource {
    /// Parsed sections: header -> content
    pub sections: HashMap<String, String>,
    /// Raw content for fallback
    raw_content: String,
}

impl ClaudeMdSource {
    /// Load CLAUDE.md from a project directory.
    ///
    /// Searches for CLAUDE.md in the directory and loads it.
    /// Returns an empty source if not found.
    pub async fn load(project_dir: &Path) -> Self {
        let claude_md_path = project_dir.join("CLAUDE.md");

        if claude_md_path.exists() {
            match tokio::fs::read_to_string(&claude_md_path).await {
                Ok(content) => Self::from_content(&content),
                Err(e) => {
                    tracing::warn!("Failed to read CLAUDE.md: {}", e);
                    Self::empty()
                }
            }
        } else {
            tracing::debug!("No CLAUDE.md found in {:?}", project_dir);
            Self::empty()
        }
    }

    /// Create from raw markdown content.
    pub fn from_content(content: &str) -> Self {
        let sections = Self::parse_sections(content);
        Self {
            sections,
            raw_content: content.to_string(),
        }
    }

    /// Create an empty source.
    pub fn empty() -> Self {
        Self {
            sections: HashMap::new(),
            raw_content: String::new(),
        }
    }

    /// Parse markdown into sections using comrak AST.
    fn parse_sections(content: &str) -> HashMap<String, String> {
        let arena = Arena::new();
        let options = Options::default();
        let root = parse_document(&arena, content, &options);

        let mut sections = HashMap::new();
        let mut current_header: Option<String> = None;
        let mut current_content = String::new();

        for node in root.children() {
            match &node.data.borrow().value {
                NodeValue::Heading(_) => {
                    // Save previous section
                    if let Some(header) = current_header.take() {
                        let trimmed = current_content.trim().to_string();
                        if !trimmed.is_empty() {
                            sections.insert(header, trimmed);
                        }
                    }
                    current_content.clear();

                    // Extract new header text
                    current_header = Some(Self::extract_text(node));
                }
                _ => {
                    // Accumulate content under current header
                    if current_header.is_some() {
                        let text = Self::node_to_text(node, content);
                        if !text.is_empty() {
                            if !current_content.is_empty() {
                                current_content.push_str("\n\n");
                            }
                            current_content.push_str(&text);
                        }
                    }
                }
            }
        }

        // Save final section
        if let Some(header) = current_header {
            let trimmed = current_content.trim().to_string();
            if !trimmed.is_empty() {
                sections.insert(header, trimmed);
            }
        }

        sections
    }

    /// Extract text content from a heading node.
    fn extract_text<'a>(node: &'a AstNode<'a>) -> String {
        let mut text = String::new();
        Self::collect_text(node, &mut text);
        text.trim().to_string()
    }

    /// Recursively collect text from nodes.
    fn collect_text<'a>(node: &'a AstNode<'a>, out: &mut String) {
        match &node.data.borrow().value {
            NodeValue::Text(t) => out.push_str(t),
            NodeValue::Code(c) => out.push_str(&c.literal),
            _ => {
                for child in node.children() {
                    Self::collect_text(child, out);
                }
            }
        }
    }

    /// Convert a node back to approximate markdown text.
    fn node_to_text<'a>(node: &'a AstNode<'a>, _original: &str) -> String {
        let mut text = String::new();
        Self::collect_text(node, &mut text);
        text
    }

    /// Find sections relevant to a query using simple keyword matching.
    fn find_relevant_sections(&self, query: &str) -> Vec<(&str, &str)> {
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();

        let mut matches: Vec<_> = self
            .sections
            .iter()
            .filter_map(|(header, content)| {
                let header_lower = header.to_lowercase();
                let content_lower = content.to_lowercase();

                // Score based on keyword matches
                let score: usize = query_words
                    .iter()
                    .map(|word| {
                        let mut s = 0;
                        if header_lower.contains(word) {
                            s += 2; // Header match worth more
                        }
                        if content_lower.contains(word) {
                            s += 1;
                        }
                        s
                    })
                    .sum();

                if score > 0 {
                    Some((score, header.as_str(), content.as_str()))
                } else {
                    None
                }
            })
            .collect();

        // Sort by score descending
        matches.sort_by(|a, b| b.0.cmp(&a.0));

        matches.into_iter().map(|(_, h, c)| (h, c)).collect()
    }
}

impl KnowledgeSource for ClaudeMdSource {
    fn source_name(&self) -> &str {
        "CLAUDE.md"
    }

    fn query(&self, question: &str) -> Option<KnowledgeFact> {
        let relevant = self.find_relevant_sections(question);

        if let Some((header, content)) = relevant.first() {
            Some(KnowledgeFact {
                source: format!("CLAUDE.md: {}", header),
                content: (*content).to_string(),
                relevance: 0.8,
            })
        } else {
            None
        }
    }

    fn context_summary(&self) -> Option<String> {
        if self.sections.is_empty() {
            return None;
        }

        let summary: String = self
            .sections
            .iter()
            .map(|(header, content)| {
                // Truncate long sections
                let truncated = if content.len() > 500 {
                    format!("{}...", &content[..500])
                } else {
                    content.clone()
                };
                format!("### {}\n\n{}", header, truncated)
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        Some(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sections() {
        let markdown = r#"# Project

## Commands

```bash
cargo test
```

## Conventions

- Use snake_case
- Prefer Result over panic

## Resources

Some links here.
"#;

        let source = ClaudeMdSource::from_content(markdown);

        assert!(source.sections.contains_key("Commands"));
        assert!(source.sections.contains_key("Conventions"));
        assert!(source.sections["Conventions"].contains("snake_case"));
    }

    #[test]
    fn test_empty_file() {
        let source = ClaudeMdSource::from_content("");
        assert!(source.sections.is_empty());
    }

    #[test]
    fn test_no_headers() {
        let markdown = "Just some plain text without headers.";
        let source = ClaudeMdSource::from_content(markdown);

        // Should be empty since no headers to section by
        assert!(source.sections.is_empty());
    }

    #[test]
    fn test_query_finds_relevant_section() {
        let markdown = r#"## Error Handling

Use thiserror for error types.

## Testing

Use cargo nextest.
"#;

        let source = ClaudeMdSource::from_content(markdown);
        let fact = source.query("how to handle errors");

        assert!(fact.is_some());
        let fact = fact.unwrap();
        assert!(fact.content.contains("thiserror"));
    }

    #[test]
    fn test_context_summary() {
        let markdown = r#"## Commands

cargo test

## Style

Use rustfmt
"#;

        let source = ClaudeMdSource::from_content(markdown);
        let summary = source.context_summary();

        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert!(summary.contains("Commands"));
        assert!(summary.contains("cargo test"));
    }

    #[test]
    fn test_find_relevant_sections_scoring() {
        let markdown = r#"## Error Handling

Use thiserror.

## Testing Errors

Test error cases.

## Logging

Use tracing.
"#;

        let source = ClaudeMdSource::from_content(markdown);

        // "error" appears in header of first two sections
        let relevant = source.find_relevant_sections("error");

        assert!(!relevant.is_empty());
        // Both error-related sections should be found
        let headers: Vec<_> = relevant.iter().map(|(h, _)| *h).collect();
        assert!(headers.contains(&"Error Handling") || headers.contains(&"Testing Errors"));
    }
}
```

**Step 4: Verify pass**

Run: `cargo test -p claude-supervisor knowledge::claude_md -- --nocapture`

Expected: All 6 tests pass

**Step 5: Commit**

```bash
git add src/knowledge/claude_md.rs Cargo.toml
git commit -m "feat(knowledge): add CLAUDE.md loader with section extraction

Implements issue #39:
- Uses comrak for AST-based markdown parsing
- Extracts sections by headers
- Keyword-based query matching with scoring
- Truncates long sections for context building"
```

---

### Task 1.4: Session History Reader

**Files:**
- Create: `src/knowledge/history.rs`

**Step 1: Write failing test**

```rust
// src/knowledge/history.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_qa_pairs() {
        let entries = vec![
            JournalEntry::User(UserEntry {
                uuid: "q1".to_string(),
                parent_uuid: None,
                session_id: "s1".to_string(),
                timestamp: "2026-01-29T10:00:00Z".to_string(),
                message: Message {
                    role: "user".to_string(),
                    content: MessageContent::Text("How do I run tests?".to_string()),
                },
                user_type: "external".to_string(),
                cwd: "/tmp".to_string(),
                git_branch: None,
                version: "2.1.25".to_string(),
                is_sidechain: None,
                source_tool_use_id: None,
                tool_use_result: None,
            }),
            JournalEntry::Assistant(AssistantEntry {
                uuid: "a1".to_string(),
                parent_uuid: Some("q1".to_string()),
                session_id: "s1".to_string(),
                timestamp: "2026-01-29T10:00:01Z".to_string(),
                message: AssistantMessage {
                    role: "assistant".to_string(),
                    content: vec![ContentBlock::Text {
                        text: "Use cargo nextest run".to_string(),
                    }],
                    model: Some("claude-3".to_string()),
                },
                cwd: "/tmp".to_string(),
                version: "2.1.25".to_string(),
                git_branch: None,
                is_sidechain: None,
            }),
        ];

        let pairs = extract_qa_pairs(&entries);

        assert_eq!(pairs.len(), 1);
        assert!(pairs[0].question.contains("run tests"));
        assert!(pairs[0].answer.contains("nextest"));
    }

    #[test]
    fn test_history_source_query() {
        let pairs = vec![
            QAPair {
                question: "What test framework?".to_string(),
                answer: "Use cargo nextest".to_string(),
                timestamp: "2026-01-29T10:00:00Z".to_string(),
            },
            QAPair {
                question: "How to format?".to_string(),
                answer: "Use cargo fmt".to_string(),
                timestamp: "2026-01-29T10:00:01Z".to_string(),
            },
        ];

        let source = SessionHistorySource { pairs };
        let fact = source.query("test framework");

        assert!(fact.is_some());
        assert!(fact.unwrap().content.contains("nextest"));
    }

    #[test]
    fn test_skip_tool_results() {
        let entries = vec![
            JournalEntry::User(UserEntry {
                uuid: "tr1".to_string(),
                parent_uuid: Some("prev".to_string()),
                session_id: "s1".to_string(),
                timestamp: "2026-01-29T10:00:00Z".to_string(),
                message: Message {
                    role: "user".to_string(),
                    content: MessageContent::Text("".to_string()),
                },
                user_type: "external".to_string(),
                cwd: "/tmp".to_string(),
                git_branch: None,
                version: "2.1.25".to_string(),
                is_sidechain: None,
                source_tool_use_id: Some("tool-123".to_string()), // This is a tool result
                tool_use_result: Some(serde_json::json!({"result": "ok"})),
            }),
        ];

        let pairs = extract_qa_pairs(&entries);

        // Tool results should not be extracted as Q&A
        assert!(pairs.is_empty());
    }
}
```

**Step 2: Verify failure**

Run: `cargo test -p claude-supervisor knowledge::history --no-run 2>&1 | head -20`

Expected: Compilation error - `SessionHistorySource` not found

**Step 3: Implement**

```rust
// src/knowledge/history.rs
//! Session history knowledge source.
//!
//! Extracts Q&A pairs from Claude Code JSONL conversation files
//! to maintain consistency across sessions.

use super::source::{KnowledgeFact, KnowledgeSource};
use crate::watcher::{
    AssistantEntry, ContentBlock, JournalEntry, Message, MessageContent, UserEntry,
    AssistantMessage,
};

/// A question-answer pair extracted from session history.
#[derive(Debug, Clone)]
pub struct QAPair {
    pub question: String,
    pub answer: String,
    pub timestamp: String,
}

/// Knowledge source backed by session history.
pub struct SessionHistorySource {
    pub pairs: Vec<QAPair>,
}

impl SessionHistorySource {
    /// Create from a list of journal entries.
    pub fn from_entries(entries: &[JournalEntry]) -> Self {
        let pairs = extract_qa_pairs(entries);
        Self { pairs }
    }

    /// Create an empty source.
    pub fn empty() -> Self {
        Self { pairs: Vec::new() }
    }

    /// Load from JSONL files in a project directory.
    pub async fn load(project_dir: &std::path::Path) -> Self {
        // Find the Claude projects directory for this path
        let home = dirs::home_dir().unwrap_or_default();
        let claude_projects = home.join(".claude").join("projects");

        if !claude_projects.exists() {
            tracing::debug!("No Claude projects directory found");
            return Self::empty();
        }

        // Hash the project path to find the right directory
        let project_hash = project_dir
            .to_string_lossy()
            .replace('/', "-")
            .trim_start_matches('-')
            .to_string();

        let project_sessions = claude_projects.join(&project_hash);

        if !project_sessions.exists() {
            tracing::debug!("No session history for project: {:?}", project_dir);
            return Self::empty();
        }

        // Load all JSONL files
        let mut all_entries = Vec::new();

        if let Ok(mut entries) = tokio::fs::read_dir(&project_sessions).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                    match crate::watcher::parse_jsonl_file(&path).await {
                        Ok(entries) => all_entries.extend(entries),
                        Err(e) => tracing::warn!("Failed to parse {:?}: {}", path, e),
                    }
                }
            }
        }

        Self::from_entries(&all_entries)
    }

    /// Find Q&A pairs matching a query.
    fn find_matching_pairs(&self, query: &str) -> Vec<&QAPair> {
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();

        let mut matches: Vec<_> = self
            .pairs
            .iter()
            .filter_map(|pair| {
                let q_lower = pair.question.to_lowercase();

                let score: usize = query_words
                    .iter()
                    .filter(|word| q_lower.contains(*word))
                    .count();

                if score > 0 {
                    Some((score, pair))
                } else {
                    None
                }
            })
            .collect();

        matches.sort_by(|a, b| b.0.cmp(&a.0));
        matches.into_iter().map(|(_, p)| p).collect()
    }
}

impl KnowledgeSource for SessionHistorySource {
    fn source_name(&self) -> &str {
        "Session History"
    }

    fn query(&self, question: &str) -> Option<KnowledgeFact> {
        let matches = self.find_matching_pairs(question);

        matches.first().map(|pair| KnowledgeFact {
            source: "Session History".to_string(),
            content: format!("Q: {}\nA: {}", pair.question, pair.answer),
            relevance: 0.7,
        })
    }

    fn context_summary(&self) -> Option<String> {
        if self.pairs.is_empty() {
            return None;
        }

        // Return recent Q&A pairs (up to 10)
        let recent: Vec<_> = self.pairs.iter().rev().take(10).collect();

        let summary = recent
            .iter()
            .map(|pair| format!("Q: {}\nA: {}", pair.question, pair.answer))
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");

        Some(summary)
    }
}

/// Extract Q&A pairs from journal entries.
pub fn extract_qa_pairs(entries: &[JournalEntry]) -> Vec<QAPair> {
    let mut pairs = Vec::new();

    // Build a map of uuid -> entry for parent lookup
    let mut user_messages: std::collections::HashMap<String, &UserEntry> =
        std::collections::HashMap::new();
    let mut assistant_messages: Vec<&AssistantEntry> = Vec::new();

    for entry in entries {
        match entry {
            JournalEntry::User(u) => {
                // Skip tool results (they have source_tool_use_id)
                if u.source_tool_use_id.is_none() {
                    user_messages.insert(u.uuid.clone(), u);
                }
            }
            JournalEntry::Assistant(a) => {
                assistant_messages.push(a);
            }
            _ => {}
        }
    }

    // Match assistant messages to their parent user messages
    for assistant in assistant_messages {
        if let Some(parent_uuid) = &assistant.parent_uuid {
            if let Some(user) = user_messages.get(parent_uuid) {
                let question = user.message.content.as_text();
                let answer = extract_assistant_text(&assistant.message);

                // Only include if both have meaningful content
                if !question.trim().is_empty() && !answer.trim().is_empty() {
                    pairs.push(QAPair {
                        question,
                        answer,
                        timestamp: assistant.timestamp.clone(),
                    });
                }
            }
        }
    }

    pairs
}

/// Extract text content from an assistant message.
fn extract_assistant_text(message: &AssistantMessage) -> String {
    message
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_user_entry(uuid: &str, parent: Option<&str>, content: &str) -> JournalEntry {
        JournalEntry::User(UserEntry {
            uuid: uuid.to_string(),
            parent_uuid: parent.map(|s| s.to_string()),
            session_id: "s1".to_string(),
            timestamp: "2026-01-29T10:00:00Z".to_string(),
            message: Message {
                role: "user".to_string(),
                content: MessageContent::Text(content.to_string()),
            },
            user_type: "external".to_string(),
            cwd: "/tmp".to_string(),
            git_branch: None,
            version: "2.1.25".to_string(),
            is_sidechain: None,
            source_tool_use_id: None,
            tool_use_result: None,
        })
    }

    fn make_assistant_entry(uuid: &str, parent: &str, content: &str) -> JournalEntry {
        JournalEntry::Assistant(AssistantEntry {
            uuid: uuid.to_string(),
            parent_uuid: Some(parent.to_string()),
            session_id: "s1".to_string(),
            timestamp: "2026-01-29T10:00:01Z".to_string(),
            message: AssistantMessage {
                role: "assistant".to_string(),
                content: vec![ContentBlock::Text {
                    text: content.to_string(),
                }],
                model: Some("claude-3".to_string()),
            },
            cwd: "/tmp".to_string(),
            version: "2.1.25".to_string(),
            git_branch: None,
            is_sidechain: None,
        })
    }

    #[test]
    fn test_extract_qa_pairs() {
        let entries = vec![
            make_user_entry("q1", None, "How do I run tests?"),
            make_assistant_entry("a1", "q1", "Use cargo nextest run"),
        ];

        let pairs = extract_qa_pairs(&entries);

        assert_eq!(pairs.len(), 1);
        assert!(pairs[0].question.contains("run tests"));
        assert!(pairs[0].answer.contains("nextest"));
    }

    #[test]
    fn test_history_source_query() {
        let pairs = vec![
            QAPair {
                question: "What test framework?".to_string(),
                answer: "Use cargo nextest".to_string(),
                timestamp: "2026-01-29T10:00:00Z".to_string(),
            },
            QAPair {
                question: "How to format?".to_string(),
                answer: "Use cargo fmt".to_string(),
                timestamp: "2026-01-29T10:00:01Z".to_string(),
            },
        ];

        let source = SessionHistorySource { pairs };
        let fact = source.query("test framework");

        assert!(fact.is_some());
        assert!(fact.unwrap().content.contains("nextest"));
    }

    #[test]
    fn test_skip_tool_results() {
        let entries = vec![JournalEntry::User(UserEntry {
            uuid: "tr1".to_string(),
            parent_uuid: Some("prev".to_string()),
            session_id: "s1".to_string(),
            timestamp: "2026-01-29T10:00:00Z".to_string(),
            message: Message {
                role: "user".to_string(),
                content: MessageContent::Text("".to_string()),
            },
            user_type: "external".to_string(),
            cwd: "/tmp".to_string(),
            git_branch: None,
            version: "2.1.25".to_string(),
            is_sidechain: None,
            source_tool_use_id: Some("tool-123".to_string()),
            tool_use_result: Some(serde_json::json!({"result": "ok"})),
        })];

        let pairs = extract_qa_pairs(&entries);

        assert!(pairs.is_empty());
    }

    #[test]
    fn test_context_summary_limits() {
        let pairs: Vec<QAPair> = (0..20)
            .map(|i| QAPair {
                question: format!("Question {}", i),
                answer: format!("Answer {}", i),
                timestamp: format!("2026-01-29T10:00:{:02}Z", i),
            })
            .collect();

        let source = SessionHistorySource { pairs };
        let summary = source.context_summary().unwrap();

        // Should only include recent 10
        assert!(summary.contains("Question 19"));
        assert!(summary.contains("Question 10"));
        assert!(!summary.contains("Question 0"));
    }
}
```

**Step 4: Verify pass**

Run: `cargo test -p claude-supervisor knowledge::history -- --nocapture`

Expected: All 4 tests pass

**Step 5: Commit**

```bash
git add src/knowledge/history.rs
git commit -m "feat(knowledge): add session history reader for Q&A extraction

Implements issue #40:
- Extracts Q&A pairs from JSONL entries via parent_uuid threading
- Keyword-based query matching
- Skips tool results (not actual questions)
- Limits context to recent 10 pairs"
```

---

### Task 1.5: Wire Up lib.rs and Integration Test

**Files:**
- Modify: `src/lib.rs` (add module exports)
- Create: `tests/knowledge_integration.rs`

**Step 1: Write failing test**

```rust
// tests/knowledge_integration.rs
use claude_supervisor::knowledge::{ClaudeMdSource, KnowledgeAggregator, KnowledgeSource};

#[test]
fn test_aggregator_with_claude_md() {
    let markdown = r#"## Commands

Run tests with: cargo nextest run

## Conventions

- Use snake_case for variables
- Prefer Result over unwrap
"#;

    let source = ClaudeMdSource::from_content(markdown);
    let mut agg = KnowledgeAggregator::new();
    agg.add_source(Box::new(source));

    // Should find test command
    let facts = agg.query("how to run tests");
    assert!(!facts.is_empty());
    assert!(facts[0].content.contains("nextest"));

    // Should build context
    let context = agg.build_context();
    assert!(context.contains("Commands"));
    assert!(context.contains("Conventions"));
}
```

**Step 2: Verify failure**

Run: `cargo test --test knowledge_integration --no-run 2>&1 | head -20`

Expected: Compilation error - module not exported

**Step 3: Implement**

```rust
// src/lib.rs - add these lines
pub mod knowledge;
pub mod watcher;
```

**Step 4: Verify pass**

Run: `cargo test --test knowledge_integration -- --nocapture`

Expected: Test passes

**Step 5: Final verification - all tests**

Run: `cargo test -p claude-supervisor`

Expected: All tests pass

**Step 6: Commit**

```bash
git add src/lib.rs tests/knowledge_integration.rs
git commit -m "feat(knowledge): wire up modules and add integration test

Completes Phase 3 knowledge layer:
- Export knowledge and watcher modules from lib.rs
- Integration test verifying aggregator with real sources"
```

---

## Final Verification

Run all tests and clippy:

```bash
cargo test -p claude-supervisor
cargo clippy --all-targets --all-features -- -D warnings
```

Expected: All tests pass, no clippy warnings

## Summary

| Task | Issue | Deliverable |
|------|-------|-------------|
| 1.1 | #22 | JSONL parser with 7 entry types |
| 1.2 | Foundation | KnowledgeSource trait + aggregator |
| 1.3 | #39 | CLAUDE.md loader with comrak |
| 1.4 | #40 | Session history Q&A extraction |
| 1.5 | Integration | Module exports + integration test |
