//! Event types from Claude Code stream-json output.

use serde::{Deserialize, Serialize};

/// Events emitted by Claude Code in stream-json format.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClaudeEvent {
    /// System message or status update.
    System { message: String },
    /// Assistant text output.
    Assistant { text: String },
    /// Tool use request.
    ToolUse {
        tool_name: String,
        tool_input: serde_json::Value,
    },
    /// Tool result.
    ToolResult {
        tool_name: String,
        output: serde_json::Value,
    },
    /// Catch-all for unknown event types.
    #[serde(other)]
    Unknown,
}
