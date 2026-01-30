//! Event types from Claude Code stream-json output.
//!
//! This module defines all event types that Claude Code emits when running
//! in non-interactive mode with `--output-format stream-json`.

use serde::{Deserialize, Serialize};

/// System initialization event data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    pub mcp_servers: Vec<String>,
    /// Event subtype (e.g., "init").
    #[serde(default)]
    pub subtype: Option<String>,
}

/// Tool use request data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolUse {
    /// Unique identifier for this tool use.
    pub id: String,
    /// Name of the tool being invoked.
    pub name: String,
    /// Tool input parameters.
    pub input: serde_json::Value,
}

/// Tool execution result data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    /// Identifier matching the original tool use.
    pub tool_use_id: String,
    /// Result content from tool execution.
    pub content: String,
    /// Whether the tool execution resulted in an error.
    #[serde(default)]
    pub is_error: bool,
}

/// Content delta types for streaming.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentDelta {
    /// Text content delta.
    TextDelta {
        /// The text fragment.
        text: String,
    },
    /// JSON input delta (for tool inputs).
    InputJsonDelta {
        /// Partial JSON string.
        partial_json: String,
    },
    /// Thinking content delta.
    ThinkingDelta {
        /// The thinking fragment.
        thinking: String,
    },
    /// Catch-all for unknown delta types.
    #[serde(other)]
    Unknown,
}

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
}

/// Events emitted by Claude Code in stream-json format.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClaudeEvent {
    /// System initialization event.
    System(SystemInit),
    /// Assistant message event.
    Assistant {
        /// Message content (flexible structure).
        message: serde_json::Value,
    },
    /// Tool use request.
    ToolUse(ToolUse),
    /// Tool execution result.
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
    /// Catch-all for unknown event types.
    #[serde(other)]
    Unknown,
}

impl ClaudeEvent {
    /// Returns true if this is a terminal event (`Result` or `MessageStop`).
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Result(_) | Self::MessageStop)
    }

    /// Returns the tool name if this is a `ToolUse` event.
    #[must_use]
    pub fn tool_name(&self) -> Option<&str> {
        match self {
            Self::ToolUse(tool_use) => Some(&tool_use.name),
            _ => None,
        }
    }

    /// Returns the session ID if available.
    #[must_use]
    pub fn session_id(&self) -> Option<&str> {
        match self {
            Self::System(init) => Some(&init.session_id),
            Self::Result(result) => Some(&result.session_id),
            _ => None,
        }
    }
}
