//! Context compression for AI supervisor escalations.
//!
//! This module provides utilities for compressing event history into
//! a token-aware context string for the AI supervisor.

use crate::cli::ClaudeEvent;

/// Compressor for event history to fit within token limits.
#[derive(Debug, Clone)]
pub struct ContextCompressor {
    /// Maximum number of events to include.
    max_events: usize,
    /// Maximum total characters in output.
    max_chars: usize,
}

impl Default for ContextCompressor {
    fn default() -> Self {
        Self {
            max_events: 20,
            max_chars: 8000,
        }
    }
}

impl ContextCompressor {
    /// Create a new compressor with custom limits.
    #[must_use]
    pub fn new(max_events: usize, max_chars: usize) -> Self {
        Self {
            max_events,
            max_chars,
        }
    }

    /// Compress a list of events into a context string.
    #[must_use]
    pub fn compress(&self, events: &[ClaudeEvent]) -> String {
        if events.is_empty() {
            return String::new();
        }

        let mut result = String::new();
        let events_to_process: Vec<_> = events.iter().rev().take(self.max_events).collect();

        for event in events_to_process.into_iter().rev() {
            let summary = Self::summarize_event(event);
            if !summary.is_empty() {
                if result.len() + summary.len() + 1 > self.max_chars {
                    break;
                }
                if !result.is_empty() {
                    result.push('\n');
                }
                result.push_str(&summary);
            }
        }

        result
    }

    /// Summarize a single event.
    fn summarize_event(event: &ClaudeEvent) -> String {
        match event {
            ClaudeEvent::System(init) => {
                format!("[INIT] cwd={}, model={}", init.cwd, init.model)
            }
            ClaudeEvent::ToolUse(tool_use) => Self::summarize_tool_use(tool_use),
            ClaudeEvent::ToolResult(result) => Self::summarize_tool_result(result),
            ClaudeEvent::Result(result) => {
                if result.is_error {
                    format!("[ERROR] {}", truncate(&result.result, 100))
                } else {
                    format!("[RESULT] {}", truncate(&result.result, 100))
                }
            }
            ClaudeEvent::Assistant { message } => {
                let text = message
                    .get("content")
                    .and_then(|c| c.as_str())
                    .unwrap_or("");
                if text.is_empty() {
                    String::new()
                } else {
                    format!("[ASSISTANT] {}", truncate(text, 100))
                }
            }
            ClaudeEvent::MessageStart { .. }
            | ClaudeEvent::MessageStop
            | ClaudeEvent::ContentBlockStart { .. }
            | ClaudeEvent::ContentBlockStop { .. }
            | ClaudeEvent::ContentBlockDelta { .. }
            | ClaudeEvent::User { .. }
            | ClaudeEvent::Unknown => String::new(),
        }
    }

    /// Summarize a tool use event.
    fn summarize_tool_use(tool_use: &crate::cli::ToolUse) -> String {
        let input_summary = Self::summarize_input(&tool_use.input);
        format!("[TOOL] {} {}", tool_use.name, input_summary)
    }

    /// Summarize a tool result event.
    fn summarize_tool_result(result: &crate::cli::ToolResult) -> String {
        if result.is_error {
            format!("[TOOL_ERROR] {}", truncate(&result.content, 100))
        } else {
            format!("[TOOL_OK] {}", truncate(&result.content, 80))
        }
    }

    /// Summarize tool input JSON.
    fn summarize_input(input: &serde_json::Value) -> String {
        match input {
            serde_json::Value::Object(map) => {
                let parts: Vec<String> = map
                    .iter()
                    .take(3)
                    .map(|(k, v)| {
                        let v_str = match v {
                            serde_json::Value::String(s) => truncate(s, 50),
                            _ => truncate(&v.to_string(), 50),
                        };
                        format!("{k}={v_str}")
                    })
                    .collect();
                parts.join(", ")
            }
            _ => truncate(&input.to_string(), 100),
        }
    }
}

/// Truncate a string to a maximum length, adding ellipsis if needed.
/// Uses char boundaries to ensure UTF-8 safety.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        // Find the last valid char boundary where content + "..." fits in max_len
        let target_len = max_len.saturating_sub(3);
        let truncate_at = s
            .char_indices()
            .take_while(|(i, _)| *i < target_len)
            .last()
            .map_or(0, |(i, c)| i + c.len_utf8());
        format!("{}...", &s[..truncate_at])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{SystemInit, ToolResult, ToolUse};

    #[test]
    fn test_compressor_default() {
        let compressor = ContextCompressor::default();
        assert_eq!(compressor.max_events, 20);
        assert_eq!(compressor.max_chars, 8000);
    }

    #[test]
    fn test_compress_empty() {
        let compressor = ContextCompressor::default();
        let result = compressor.compress(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_compress_tool_use() {
        let compressor = ContextCompressor::default();
        let events = vec![ClaudeEvent::ToolUse(ToolUse {
            id: "tool-1".to_string(),
            name: "Read".to_string(),
            input: serde_json::json!({"file_path": "/test/file.txt"}),
        })];

        let result = compressor.compress(&events);
        assert!(result.contains("[TOOL] Read"));
        assert!(result.contains("file_path"));
    }

    #[test]
    fn test_compress_tool_result() {
        let compressor = ContextCompressor::default();
        let events = vec![ClaudeEvent::ToolResult(ToolResult {
            tool_use_id: "tool-1".to_string(),
            content: "File contents here".to_string(),
            is_error: false,
        })];

        let result = compressor.compress(&events);
        assert!(result.contains("[TOOL_OK]"));
        assert!(result.contains("File contents here"));
    }

    #[test]
    fn test_compress_error_result() {
        let compressor = ContextCompressor::default();
        let events = vec![ClaudeEvent::ToolResult(ToolResult {
            tool_use_id: "tool-1".to_string(),
            content: "Permission denied".to_string(),
            is_error: true,
        })];

        let result = compressor.compress(&events);
        assert!(result.contains("[TOOL_ERROR]"));
        assert!(result.contains("Permission denied"));
    }

    #[test]
    fn test_truncate_long_content() {
        let long_string = "a".repeat(200);
        let truncated = truncate(&long_string, 50);
        // With ASCII chars, truncation at max_len-3 + "..." = 47 + 3 = 50
        assert_eq!(truncated.len(), 50);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_truncate_utf8_safety() {
        // Test with multi-byte characters (emoji is 4 bytes, crab is U+1F980)
        let text = "Hello 🦀 World";
        let result = truncate(text, 10);
        assert!(!result.is_empty());
        // Should not panic and should be valid UTF-8
        assert!(result.is_char_boundary(result.len()));

        // Test truncating right at emoji boundary
        let emoji_text = "🦀🦀🦀🦀🦀"; // 5 crabs, 20 bytes
        let result = truncate(emoji_text, 10);
        assert!(result.ends_with("..."));
        // Verify we can iterate over chars (proves valid UTF-8)
        assert!(result.chars().count() > 0);

        // Test with mixed multi-byte chars
        let mixed = "日本語テスト"; // Japanese text, 3 bytes per char
        let result = truncate(mixed, 10);
        assert!(result.is_char_boundary(result.len()));
    }

    #[test]
    fn test_compress_respects_max_chars() {
        let compressor = ContextCompressor::new(100, 100);
        let events: Vec<ClaudeEvent> = (0..50)
            .map(|i| {
                ClaudeEvent::ToolUse(ToolUse {
                    id: format!("tool-{i}"),
                    name: "Read".to_string(),
                    input: serde_json::json!({"file_path": format!("/test/file{i}.txt")}),
                })
            })
            .collect();

        let result = compressor.compress(&events);
        assert!(result.len() <= 100);
    }

    #[test]
    fn test_compress_system_init() {
        let compressor = ContextCompressor::default();
        let events = vec![ClaudeEvent::System(SystemInit {
            cwd: "/home/user/project".to_string(),
            tools: vec!["Read".to_string(), "Write".to_string()],
            model: "claude-3".to_string(),
            session_id: "test-session".to_string(),
            mcp_servers: vec![],
            subtype: None,
            permission_mode: None,
            claude_code_version: None,
            agents: vec![],
            skills: vec![],
            slash_commands: vec![],
        })];

        let result = compressor.compress(&events);
        assert!(result.contains("[INIT]"));
        assert!(result.contains("/home/user/project"));
        assert!(result.contains("claude-3"));
    }
}
