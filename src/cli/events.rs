//! Event types from Claude Code stream-json output.
//!
//! This module defines all event types that Claude Code emits when running
//! in non-interactive mode with `--output-format stream-json`.

use serde::{Deserialize, Serialize};

/// A Claude event with its original raw JSON preserved.
///
/// This wrapper stores the original JSON string alongside the parsed event,
/// ensuring no information is lost during parsing.
#[derive(Debug, Clone, PartialEq)]
pub struct RawClaudeEvent {
    /// The original JSON string.
    raw: String,
    /// The parsed event.
    event: ClaudeEvent,
}

impl RawClaudeEvent {
    /// Parse a JSON string into a `RawClaudeEvent`.
    ///
    /// # Errors
    ///
    /// Returns an error if the JSON cannot be parsed as a `ClaudeEvent`.
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

/// MCP server status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpServer {
    /// Server name.
    pub name: String,
    /// Connection status.
    pub status: String,
}

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
#[derive(Debug, Clone, PartialEq)]
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

impl Serialize for ClaudeEvent {
    #[allow(clippy::too_many_lines)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;

        match self {
            ClaudeEvent::System(init) => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "system")?;
                // Serialize all SystemInit fields
                map.serialize_entry("cwd", &init.cwd)?;
                map.serialize_entry("tools", &init.tools)?;
                map.serialize_entry("model", &init.model)?;
                map.serialize_entry("session_id", &init.session_id)?;
                map.serialize_entry("mcp_servers", &init.mcp_servers)?;
                if let Some(ref subtype) = init.subtype {
                    map.serialize_entry("subtype", subtype)?;
                }
                if let Some(ref permission_mode) = init.permission_mode {
                    map.serialize_entry("permission_mode", permission_mode)?;
                }
                if let Some(ref version) = init.claude_code_version {
                    map.serialize_entry("claude_code_version", version)?;
                }
                if !init.agents.is_empty() {
                    map.serialize_entry("agents", &init.agents)?;
                }
                if !init.skills.is_empty() {
                    map.serialize_entry("skills", &init.skills)?;
                }
                if !init.slash_commands.is_empty() {
                    map.serialize_entry("slash_commands", &init.slash_commands)?;
                }
                map.end()
            }
            ClaudeEvent::Assistant { message } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "assistant")?;
                map.serialize_entry("message", message)?;
                map.end()
            }
            ClaudeEvent::User {
                message,
                tool_use_result,
            } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "user")?;
                map.serialize_entry("message", message)?;
                if let Some(ref result) = tool_use_result {
                    map.serialize_entry("tool_use_result", result)?;
                }
                map.end()
            }
            ClaudeEvent::ToolUse(tool_use) => {
                let mut map = serializer.serialize_map(Some(4))?;
                map.serialize_entry("type", "tool_use")?;
                map.serialize_entry("id", &tool_use.id)?;
                map.serialize_entry("name", &tool_use.name)?;
                map.serialize_entry("input", &tool_use.input)?;
                map.end()
            }
            ClaudeEvent::ToolResult(result) => {
                let mut map = serializer.serialize_map(Some(4))?;
                map.serialize_entry("type", "tool_result")?;
                map.serialize_entry("tool_use_id", &result.tool_use_id)?;
                map.serialize_entry("content", &result.content)?;
                map.serialize_entry("is_error", &result.is_error)?;
                map.end()
            }
            ClaudeEvent::ContentBlockDelta { index, delta } => {
                let mut map = serializer.serialize_map(Some(3))?;
                map.serialize_entry("type", "content_block_delta")?;
                map.serialize_entry("index", index)?;
                map.serialize_entry("delta", delta)?;
                map.end()
            }
            ClaudeEvent::ContentBlockStart {
                index,
                content_block,
            } => {
                let mut map = serializer.serialize_map(Some(3))?;
                map.serialize_entry("type", "content_block_start")?;
                map.serialize_entry("index", index)?;
                map.serialize_entry("content_block", content_block)?;
                map.end()
            }
            ClaudeEvent::ContentBlockStop { index } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "content_block_stop")?;
                map.serialize_entry("index", index)?;
                map.end()
            }
            ClaudeEvent::MessageStart { message } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "message_start")?;
                map.serialize_entry("message", message)?;
                map.end()
            }
            ClaudeEvent::MessageStop => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("type", "message_stop")?;
                map.end()
            }
            ClaudeEvent::Result(result) => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "result")?;
                map.serialize_entry("result", &result.result)?;
                map.serialize_entry("session_id", &result.session_id)?;
                map.serialize_entry("is_error", &result.is_error)?;
                if let Some(cost) = result.cost_usd {
                    map.serialize_entry("cost_usd", &cost)?;
                }
                if let Some(duration) = result.duration_ms {
                    map.serialize_entry("duration_ms", &duration)?;
                }
                map.end()
            }
            ClaudeEvent::Other(value) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ClaudeEvent {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;

        let event_type = value.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match event_type {
            "system" => {
                let init: SystemInit =
                    serde_json::from_value(value.clone()).map_err(serde::de::Error::custom)?;
                Ok(ClaudeEvent::System(init))
            }
            "assistant" => {
                let message = value
                    .get("message")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                Ok(ClaudeEvent::Assistant { message })
            }
            "user" => {
                let message = value
                    .get("message")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                let tool_use_result = value.get("tool_use_result").cloned();
                Ok(ClaudeEvent::User {
                    message,
                    tool_use_result,
                })
            }
            "tool_use" => {
                let tool_use: ToolUse =
                    serde_json::from_value(value.clone()).map_err(serde::de::Error::custom)?;
                Ok(ClaudeEvent::ToolUse(tool_use))
            }
            "tool_result" => {
                let tool_result: ToolResult =
                    serde_json::from_value(value.clone()).map_err(serde::de::Error::custom)?;
                Ok(ClaudeEvent::ToolResult(tool_result))
            }
            "content_block_delta" => {
                #[allow(clippy::cast_possible_truncation)]
                let index = value
                    .get("index")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0) as usize;
                let delta: ContentDelta = value
                    .get("delta")
                    .cloned()
                    .map_or(ContentDelta::Unknown, |d| {
                        serde_json::from_value(d).unwrap_or(ContentDelta::Unknown)
                    });
                Ok(ClaudeEvent::ContentBlockDelta { index, delta })
            }
            "content_block_start" => {
                #[allow(clippy::cast_possible_truncation)]
                let index = value
                    .get("index")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0) as usize;
                let content_block = value
                    .get("content_block")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                Ok(ClaudeEvent::ContentBlockStart {
                    index,
                    content_block,
                })
            }
            "content_block_stop" => {
                #[allow(clippy::cast_possible_truncation)]
                let index = value
                    .get("index")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0) as usize;
                Ok(ClaudeEvent::ContentBlockStop { index })
            }
            "message_start" => {
                let message = value
                    .get("message")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                Ok(ClaudeEvent::MessageStart { message })
            }
            "message_stop" => Ok(ClaudeEvent::MessageStop),
            "result" => {
                let result: ResultEvent =
                    serde_json::from_value(value.clone()).map_err(serde::de::Error::custom)?;
                Ok(ClaudeEvent::Result(result))
            }
            _ => {
                // Unknown type - preserve the entire JSON value
                Ok(ClaudeEvent::Other(value))
            }
        }
    }
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

    #[test]
    fn test_into_event_consumes_wrapper() {
        let json = r#"{"type":"message_stop"}"#;
        let raw = RawClaudeEvent::parse(json).unwrap();
        let event = raw.into_event();
        assert!(matches!(event, ClaudeEvent::MessageStop));
    }

    #[test]
    fn test_into_parts_returns_both() {
        let json = r#"{"type":"message_stop"}"#;
        let raw = RawClaudeEvent::parse(json).unwrap();
        let (raw_json, event) = raw.into_parts();
        assert_eq!(raw_json, json);
        assert!(matches!(event, ClaudeEvent::MessageStop));
    }

    #[test]
    fn test_parse_invalid_json_returns_error() {
        let result = RawClaudeEvent::parse("not json");
        assert!(result.is_err());
    }

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

    #[test]
    fn test_other_is_not_terminal() {
        let json = r#"{"type":"new_streaming_type","data":"test"}"#;
        let event: ClaudeEvent = serde_json::from_str(json).unwrap();
        assert!(!event.is_terminal());
    }
}
