# Flexible Stream Parsing Implementation Plan

> **REQUIRED:** Use `execute-plan` to implement this plan batch by batch.

**Goal:** Parse Claude Code stream-json output flexibly without losing information, preserving raw JSON alongside typed access.

**Architecture:** Hybrid parsing with `RawClaudeEvent` wrapper storing original JSON string + parsed `ClaudeEvent`. Add `#[serde(flatten)]` extras maps to struct variants. Replace unit `Unknown` with data-carrying `Other(Value)`. Use untagged enums for polymorphic fields.

**Tech Stack:** Rust, serde, serde_json

---

## Batch 1: Raw Event Wrapper

**Goal:** Create `RawClaudeEvent` that preserves original JSON alongside parsed event.

### Task 1.1: Add RawClaudeEvent struct

**Files:**
- Modify: `src/cli/events.rs:1-10`

**Step 1: Write failing test**

Add to `src/cli/events.rs` at the end:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_event_preserves_json() {
        let json = r#"{"type":"system","cwd":"/tmp","tools":[],"model":"test","session_id":"abc","mcp_servers":[]}"#;
        let raw = RawClaudeEvent::parse(json).unwrap();

        assert_eq!(raw.raw(), json);
        assert!(matches!(raw.event(), ClaudeEvent::System(_)));
    }

    #[test]
    fn test_raw_event_preserves_unknown_fields() {
        let json = r#"{"type":"system","cwd":"/tmp","tools":[],"model":"test","session_id":"abc","mcp_servers":[],"new_field":"preserved"}"#;
        let raw = RawClaudeEvent::parse(json).unwrap();

        assert_eq!(raw.raw(), json);
        assert!(raw.raw().contains("new_field"));
    }
}
```

**Step 2: Verify failure**

Run: `source ~/.cargo/env && cd /home/nikketryhard/dev/claude-supervisor/.worktrees/flexible-stream-parsing && cargo nextest run test_raw_event -v`

Expected: FAIL with "cannot find value `RawClaudeEvent`"

**Step 3: Implement**

Add after the imports in `src/cli/events.rs`:

```rust
/// A Claude event with its original raw JSON preserved.
///
/// This wrapper stores the original JSON string alongside the parsed event,
/// ensuring no information is lost during parsing.
#[derive(Debug, Clone)]
pub struct RawClaudeEvent {
    /// The original JSON string.
    raw: String,
    /// The parsed event.
    event: ClaudeEvent,
}

impl RawClaudeEvent {
    /// Parse a JSON string into a RawClaudeEvent.
    ///
    /// # Errors
    ///
    /// Returns an error if the JSON cannot be parsed as a ClaudeEvent.
    pub fn parse(json: &str) -> Result<Self, serde_json::Error> {
        let event: ClaudeEvent = serde_json::from_str(json)?;
        Ok(Self {
            raw: json.to_string(),
            event,
        })
    }

    /// Get the original raw JSON string.
    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// Get the parsed event.
    #[must_use]
    pub fn event(&self) -> &ClaudeEvent {
        &self.event
    }

    /// Consume self and return the parsed event.
    #[must_use]
    pub fn into_event(self) -> ClaudeEvent {
        self.event
    }

    /// Consume self and return both raw JSON and parsed event.
    #[must_use]
    pub fn into_parts(self) -> (String, ClaudeEvent) {
        (self.raw, self.event)
    }
}
```

**Step 4: Verify pass**

Run: `source ~/.cargo/env && cd /home/nikketryhard/dev/claude-supervisor/.worktrees/flexible-stream-parsing && cargo nextest run test_raw_event -v`

Expected: PASS (2 tests)

**Step 5: Commit**

```bash
cd /home/nikketryhard/dev/claude-supervisor/.worktrees/flexible-stream-parsing
git add src/cli/events.rs
git commit -m "feat(events): add RawClaudeEvent wrapper for lossless parsing"
```

---

## Batch 2: Replace Unknown with Other(Value)

**Goal:** Make the `Unknown` variant capture full JSON data instead of discarding it.

### Task 2.1: Replace Unknown variant with Other

**Files:**
- Modify: `src/cli/events.rs:167-169` (Unknown variant)

**Step 1: Write failing test**

Add to the tests module in `src/cli/events.rs`:

```rust
#[test]
fn test_unknown_event_preserves_data() {
    let json = r#"{"type":"future_event_type","data":"important","count":42}"#;
    let event: ClaudeEvent = serde_json::from_str(json).unwrap();

    match event {
        ClaudeEvent::Other(value) => {
            assert_eq!(value.get("type").unwrap(), "future_event_type");
            assert_eq!(value.get("data").unwrap(), "important");
            assert_eq!(value.get("count").unwrap(), 42);
        }
        _ => panic!("Expected Other variant"),
    }
}
```

**Step 2: Verify failure**

Run: `source ~/.cargo/env && cd /home/nikketryhard/dev/claude-supervisor/.worktrees/flexible-stream-parsing && cargo nextest run test_unknown_event -v`

Expected: FAIL with "no variant named `Other`"

**Step 3: Implement**

This requires a custom deserializer because `#[serde(other)]` only works with unit variants. Replace the `ClaudeEvent` enum definition and add a custom deserializer.

First, update the enum (remove `#[serde(other)] Unknown`):

```rust
/// Events emitted by Claude Code in stream-json format.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClaudeEvent {
    /// System initialization event.
    System(SystemInit),
    /// Assistant message event.
    Assistant {
        /// Message content (flexible structure).
        message: serde_json::Value,
    },
    /// User message event (contains tool results).
    User {
        /// Message content (flexible structure).
        message: serde_json::Value,
        /// Tool use result summary (if present, can be string or object).
        #[serde(default)]
        tool_use_result: Option<serde_json::Value>,
    },
    /// Tool use request.
    ToolUse(ToolUse),
    /// Tool execution result (legacy, kept for compatibility).
    ToolResult(ToolResult),
    /// Streaming content delta.
    ContentBlockDelta {
        /// Block index.
        index: usize,
        /// Delta content.
        delta: ContentDelta,
    },
    /// Content block start marker.
    ContentBlockStart {
        /// Block index.
        index: usize,
        /// Block metadata.
        content_block: serde_json::Value,
    },
    /// Content block end marker.
    ContentBlockStop {
        /// Block index.
        index: usize,
    },
    /// Message start marker.
    MessageStart {
        /// Message metadata.
        message: serde_json::Value,
    },
    /// Message end marker.
    MessageStop,
    /// Final result event.
    Result(ResultEvent),
    /// Catch-all for unknown event types - preserves full JSON data.
    Other(serde_json::Value),
}
```

Then add the custom deserializer after the enum:

```rust
impl<'de> Deserialize<'de> for ClaudeEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;

        // Try to get the type field
        let event_type = value
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("");

        match event_type {
            "system" => {
                let init: SystemInit = serde_json::from_value(value.clone())
                    .map_err(serde::de::Error::custom)?;
                Ok(ClaudeEvent::System(init))
            }
            "assistant" => {
                let message = value.get("message").cloned().unwrap_or(serde_json::Value::Null);
                Ok(ClaudeEvent::Assistant { message })
            }
            "user" => {
                let message = value.get("message").cloned().unwrap_or(serde_json::Value::Null);
                let tool_use_result = value.get("tool_use_result").cloned();
                Ok(ClaudeEvent::User { message, tool_use_result })
            }
            "tool_use" => {
                let tool_use: ToolUse = serde_json::from_value(value.clone())
                    .map_err(serde::de::Error::custom)?;
                Ok(ClaudeEvent::ToolUse(tool_use))
            }
            "tool_result" => {
                let tool_result: ToolResult = serde_json::from_value(value.clone())
                    .map_err(serde::de::Error::custom)?;
                Ok(ClaudeEvent::ToolResult(tool_result))
            }
            "content_block_delta" => {
                let index = value.get("index")
                    .and_then(|i| i.as_u64())
                    .unwrap_or(0) as usize;
                let delta: ContentDelta = value.get("delta")
                    .cloned()
                    .map(|d| serde_json::from_value(d).unwrap_or(ContentDelta::Unknown))
                    .unwrap_or(ContentDelta::Unknown);
                Ok(ClaudeEvent::ContentBlockDelta { index, delta })
            }
            "content_block_start" => {
                let index = value.get("index")
                    .and_then(|i| i.as_u64())
                    .unwrap_or(0) as usize;
                let content_block = value.get("content_block").cloned().unwrap_or(serde_json::Value::Null);
                Ok(ClaudeEvent::ContentBlockStart { index, content_block })
            }
            "content_block_stop" => {
                let index = value.get("index")
                    .and_then(|i| i.as_u64())
                    .unwrap_or(0) as usize;
                Ok(ClaudeEvent::ContentBlockStop { index })
            }
            "message_start" => {
                let message = value.get("message").cloned().unwrap_or(serde_json::Value::Null);
                Ok(ClaudeEvent::MessageStart { message })
            }
            "message_stop" => Ok(ClaudeEvent::MessageStop),
            "result" => {
                let result: ResultEvent = serde_json::from_value(value.clone())
                    .map_err(serde::de::Error::custom)?;
                Ok(ClaudeEvent::Result(result))
            }
            _ => {
                // Unknown type - preserve the entire JSON value
                Ok(ClaudeEvent::Other(value))
            }
        }
    }
}
```

**Step 4: Verify pass**

Run: `source ~/.cargo/env && cd /home/nikketryhard/dev/claude-supervisor/.worktrees/flexible-stream-parsing && cargo nextest run test_unknown_event -v`

Expected: PASS

**Step 5: Commit**

```bash
cd /home/nikketryhard/dev/claude-supervisor/.worktrees/flexible-stream-parsing
git add src/cli/events.rs
git commit -m "feat(events): replace Unknown with Other(Value) for data preservation"
```

### Task 2.2: Update is_terminal and existing tests

**Files:**
- Modify: `src/cli/events.rs` (update any references to Unknown)

**Step 1: Write failing test**

Add test to verify Other variant doesn't break existing functionality:

```rust
#[test]
fn test_other_is_not_terminal() {
    let json = r#"{"type":"new_streaming_type","data":"test"}"#;
    let event: ClaudeEvent = serde_json::from_str(json).unwrap();
    assert!(!event.is_terminal());
}
```

**Step 2: Verify failure**

Run: `source ~/.cargo/env && cd /home/nikketryhard/dev/claude-supervisor/.worktrees/flexible-stream-parsing && cargo nextest run test_other_is_not -v`

Expected: PASS (already works) or compile error if Unknown references remain

**Step 3: Implement**

Search and replace any remaining `Unknown` references to use `Other`:

```bash
grep -r "Unknown" src/ --include="*.rs"
```

Update `is_terminal` if needed (it should already work since `Other` is not matched).

**Step 4: Verify pass**

Run: `source ~/.cargo/env && cd /home/nikketryhard/dev/claude-supervisor/.worktrees/flexible-stream-parsing && cargo nextest run -v`

Expected: All 555+ tests pass

**Step 5: Commit**

```bash
cd /home/nikketryhard/dev/claude-supervisor/.worktrees/flexible-stream-parsing
git add -A
git commit -m "fix(events): update all Unknown references to Other"
```

---

## Batch 3: Update StreamParser to use RawClaudeEvent

**Goal:** Update the stream parser to emit `RawClaudeEvent` instead of `ClaudeEvent`.

### Task 3.1: Add RawClaudeEvent channel support

**Files:**
- Modify: `src/cli/stream.rs`

**Step 1: Write failing test**

Add to `src/cli/stream.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::events::RawClaudeEvent;

    #[tokio::test]
    async fn test_parse_raw_line() {
        let json = r#"{"type":"message_stop"}"#;
        let raw = StreamParser::parse_raw_line(json).unwrap();

        assert_eq!(raw.raw(), json);
        assert!(matches!(raw.event(), ClaudeEvent::MessageStop));
    }
}
```

**Step 2: Verify failure**

Run: `source ~/.cargo/env && cd /home/nikketryhard/dev/claude-supervisor/.worktrees/flexible-stream-parsing && cargo nextest run test_parse_raw_line -v`

Expected: FAIL with "no function named `parse_raw_line`"

**Step 3: Implement**

Add to `StreamParser` impl in `src/cli/stream.rs`:

```rust
use crate::cli::events::RawClaudeEvent;

impl StreamParser {
    /// Parse a single line into a RawClaudeEvent, preserving the original JSON.
    ///
    /// # Errors
    ///
    /// Returns `StreamError::ParseError` if the JSON is invalid.
    pub fn parse_raw_line(line: &str) -> Result<RawClaudeEvent, StreamError> {
        RawClaudeEvent::parse(line).map_err(|e| StreamError::ParseError {
            input: line.to_string(),
            reason: e.to_string(),
        })
    }
}
```

**Step 4: Verify pass**

Run: `source ~/.cargo/env && cd /home/nikketryhard/dev/claude-supervisor/.worktrees/flexible-stream-parsing && cargo nextest run test_parse_raw_line -v`

Expected: PASS

**Step 5: Commit**

```bash
cd /home/nikketryhard/dev/claude-supervisor/.worktrees/flexible-stream-parsing
git add src/cli/stream.rs
git commit -m "feat(stream): add parse_raw_line for RawClaudeEvent support"
```

### Task 3.2: Add parse_stdout_raw function

**Files:**
- Modify: `src/cli/stream.rs`

**Step 1: Write failing test**

```rust
#[tokio::test]
async fn test_parse_stdout_raw_preserves_json() {
    use tokio::sync::mpsc;

    let json_lines = r#"{"type":"message_stop"}
{"type":"result","result":"done","session_id":"abc","is_error":false}"#;

    let cursor = std::io::Cursor::new(json_lines);
    let (tx, mut rx) = mpsc::channel::<RawClaudeEvent>(10);

    StreamParser::parse_stdout_raw(cursor, tx).await.unwrap();

    let event1 = rx.recv().await.unwrap();
    assert!(event1.raw().contains("message_stop"));

    let event2 = rx.recv().await.unwrap();
    assert!(event2.raw().contains("session_id"));
}
```

**Step 2: Verify failure**

Run: `source ~/.cargo/env && cd /home/nikketryhard/dev/claude-supervisor/.worktrees/flexible-stream-parsing && cargo nextest run test_parse_stdout_raw -v`

Expected: FAIL with "no function named `parse_stdout_raw`"

**Step 3: Implement**

Add to `StreamParser` impl:

```rust
/// Parse events from an async reader and send RawClaudeEvents to a channel.
///
/// This preserves the original JSON for each event.
pub async fn parse_stdout_raw<R>(stdout: R, tx: Sender<RawClaudeEvent>) -> Result<(), StreamError>
where
    R: AsyncRead + Unpin,
{
    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();

    while let Some(line) = lines.next_line().await.map_err(StreamError::ReadError)? {
        // Skip empty lines
        if line.trim().is_empty() {
            continue;
        }

        // Print raw JSON line for verbose output
        println!("{line}");
        let _ = io::stdout().flush();

        match Self::parse_raw_line(&line) {
            Ok(raw_event) => {
                if tx.send(raw_event).await.is_err() {
                    return Err(StreamError::ChannelClosed);
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, line = %line, "Failed to parse stream line");
            }
        }
    }

    Ok(())
}
```

Update imports at top of file:

```rust
use tokio::sync::mpsc::{self, Receiver, Sender};
```

**Step 4: Verify pass**

Run: `source ~/.cargo/env && cd /home/nikketryhard/dev/claude-supervisor/.worktrees/flexible-stream-parsing && cargo nextest run test_parse_stdout_raw -v`

Expected: PASS

**Step 5: Commit**

```bash
cd /home/nikketryhard/dev/claude-supervisor/.worktrees/flexible-stream-parsing
git add src/cli/stream.rs
git commit -m "feat(stream): add parse_stdout_raw for RawClaudeEvent streaming"
```

---

## Batch 4: Add Extras Map to Key Variants

**Goal:** Add `#[serde(flatten)]` extras maps to capture unknown fields in struct variants.

### Task 4.1: Add extras to SystemInit

**Files:**
- Modify: `src/cli/events.rs` (SystemInit struct)

**Step 1: Write failing test**

```rust
#[test]
fn test_system_init_captures_extras() {
    let json = r#"{
        "type": "system",
        "cwd": "/tmp",
        "tools": [],
        "model": "test",
        "session_id": "abc",
        "mcp_servers": [],
        "future_field": "captured",
        "another_new_field": 123
    }"#;
    let event: ClaudeEvent = serde_json::from_str(json).unwrap();

    match event {
        ClaudeEvent::System(init) => {
            assert_eq!(init.extras.get("future_field").unwrap(), "captured");
            assert_eq!(init.extras.get("another_new_field").unwrap(), 123);
        }
        _ => panic!("Expected System variant"),
    }
}
```

**Step 2: Verify failure**

Run: `source ~/.cargo/env && cd /home/nikketryhard/dev/claude-supervisor/.worktrees/flexible-stream-parsing && cargo nextest run test_system_init_captures -v`

Expected: FAIL with "no field `extras`"

**Step 3: Implement**

Update `SystemInit` struct:

```rust
use std::collections::HashMap;

/// System initialization event data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SystemInit {
    /// Current working directory.
    pub cwd: String,
    /// Available tools for this session.
    pub tools: Vec<String>,
    /// Model being used.
    pub model: String,
    /// Session identifier.
    pub session_id: String,
    /// MCP servers available.
    #[serde(default)]
    pub mcp_servers: Vec<McpServer>,
    /// Event subtype (e.g., "init").
    #[serde(default)]
    pub subtype: Option<String>,
    /// Permission mode.
    #[serde(default)]
    pub permission_mode: Option<String>,
    /// Claude Code version.
    #[serde(default)]
    pub claude_code_version: Option<String>,
    /// Available agents.
    #[serde(default)]
    pub agents: Vec<String>,
    /// Available skills.
    #[serde(default)]
    pub skills: Vec<String>,
    /// Slash commands.
    #[serde(default)]
    pub slash_commands: Vec<String>,
    /// Extra fields not explicitly defined (forward compatibility).
    #[serde(flatten, default)]
    pub extras: HashMap<String, serde_json::Value>,
}
```

**Step 4: Verify pass**

Run: `source ~/.cargo/env && cd /home/nikketryhard/dev/claude-supervisor/.worktrees/flexible-stream-parsing && cargo nextest run test_system_init_captures -v`

Expected: PASS

**Step 5: Commit**

```bash
cd /home/nikketryhard/dev/claude-supervisor/.worktrees/flexible-stream-parsing
git add src/cli/events.rs
git commit -m "feat(events): add extras map to SystemInit for forward compatibility"
```

### Task 4.2: Add extras to ResultEvent

**Files:**
- Modify: `src/cli/events.rs` (ResultEvent struct)

**Step 1: Write failing test**

```rust
#[test]
fn test_result_event_captures_extras() {
    let json = r#"{
        "type": "result",
        "result": "done",
        "session_id": "abc",
        "is_error": false,
        "subtype": "success",
        "total_cost_usd": 0.05,
        "usage": {"input_tokens": 100}
    }"#;
    let event: ClaudeEvent = serde_json::from_str(json).unwrap();

    match event {
        ClaudeEvent::Result(res) => {
            assert!(res.extras.contains_key("subtype"));
            assert!(res.extras.contains_key("total_cost_usd"));
            assert!(res.extras.contains_key("usage"));
        }
        _ => panic!("Expected Result variant"),
    }
}
```

**Step 2: Verify failure**

Run: `source ~/.cargo/env && cd /home/nikketryhard/dev/claude-supervisor/.worktrees/flexible-stream-parsing && cargo nextest run test_result_event_captures -v`

Expected: FAIL with "no field `extras`"

**Step 3: Implement**

Update `ResultEvent` struct:

```rust
/// Final result event data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResultEvent {
    /// The result content.
    pub result: String,
    /// Session identifier.
    pub session_id: String,
    /// Whether an error occurred.
    #[serde(default)]
    pub is_error: bool,
    /// Total cost in USD.
    #[serde(default)]
    pub cost_usd: Option<f64>,
    /// Total duration in milliseconds.
    #[serde(default)]
    pub duration_ms: Option<u64>,
    /// Extra fields not explicitly defined (forward compatibility).
    #[serde(flatten, default)]
    pub extras: HashMap<String, serde_json::Value>,
}
```

**Step 4: Verify pass**

Run: `source ~/.cargo/env && cd /home/nikketryhard/dev/claude-supervisor/.worktrees/flexible-stream-parsing && cargo nextest run test_result_event_captures -v`

Expected: PASS

**Step 5: Commit**

```bash
cd /home/nikketryhard/dev/claude-supervisor/.worktrees/flexible-stream-parsing
git add src/cli/events.rs
git commit -m "feat(events): add extras map to ResultEvent for forward compatibility"
```

---

## Batch 5: Export and Integration

**Goal:** Export new types and ensure all tests pass.

### Task 5.1: Update module exports

**Files:**
- Modify: `src/cli/mod.rs`

**Step 1: Write failing test**

```rust
// In a new test file or integration test
use claude_supervisor::cli::RawClaudeEvent;

#[test]
fn test_raw_event_is_exported() {
    let json = r#"{"type":"message_stop"}"#;
    let _raw = RawClaudeEvent::parse(json).unwrap();
}
```

**Step 2: Verify failure**

Run: `source ~/.cargo/env && cd /home/nikketryhard/dev/claude-supervisor/.worktrees/flexible-stream-parsing && cargo nextest run -v 2>&1 | head -50`

Expected: May fail if `RawClaudeEvent` is not exported

**Step 3: Implement**

Check and update `src/cli/mod.rs` to export `RawClaudeEvent`:

```rust
pub use events::{
    ClaudeEvent, ContentDelta, McpServer, RawClaudeEvent, ResultEvent, SystemInit, ToolResult,
    ToolUse,
};
```

**Step 4: Verify pass**

Run: `source ~/.cargo/env && cd /home/nikketryhard/dev/claude-supervisor/.worktrees/flexible-stream-parsing && cargo nextest run -v`

Expected: All tests pass

**Step 5: Commit**

```bash
cd /home/nikketryhard/dev/claude-supervisor/.worktrees/flexible-stream-parsing
git add src/cli/mod.rs
git commit -m "feat(cli): export RawClaudeEvent from cli module"
```

### Task 5.2: Run full test suite and clippy

**Files:**
- None (verification only)

**Step 1: Run all tests**

```bash
source ~/.cargo/env
cd /home/nikketryhard/dev/claude-supervisor/.worktrees/flexible-stream-parsing
cargo nextest run
```

Expected: All tests pass

**Step 2: Run clippy**

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

Expected: No warnings

**Step 3: Run fmt check**

```bash
cargo fmt --all -- --check
```

Expected: No formatting issues

**Step 4: Commit any fixes**

```bash
git add -A
git commit -m "chore: fix clippy warnings and formatting"
```

---

## Summary

| Batch | Tasks | Purpose |
|-------|-------|---------|
| 1 | 1 | `RawClaudeEvent` wrapper for lossless parsing |
| 2 | 2 | Replace `Unknown` with `Other(Value)` |
| 3 | 2 | Update `StreamParser` for raw events |
| 4 | 2 | Add `extras` maps to key structs |
| 5 | 2 | Export types and verify integration |

**Total:** 9 tasks across 5 batches

After completion, the parser will:
- Preserve original JSON for every event
- Capture unknown event types with full data
- Capture unknown fields in known event types
- Support both typed access and raw access patterns
