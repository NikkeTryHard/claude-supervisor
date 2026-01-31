//! Tests for Claude event type parsing.

use claude_supervisor::cli::{ClaudeEvent, ContentDelta, ResultEvent, SystemInit, ToolUse};

#[test]
fn parse_system_init_event() {
    let json = r#"{"type":"system","subtype":"init","session_id":"abc123","cwd":"/home/user/project","tools":["Read","Write","Bash"],"model":"claude-sonnet-4-20250514","mcp_servers":[]}"#;
    let event: ClaudeEvent = serde_json::from_str(json).unwrap();

    match event {
        ClaudeEvent::System(init) => {
            assert_eq!(init.subtype, Some("init".to_string()));
            assert_eq!(init.session_id, "abc123");
            assert_eq!(init.cwd, "/home/user/project");
            assert_eq!(init.tools, vec!["Read", "Write", "Bash"]);
            assert_eq!(init.model, "claude-sonnet-4-20250514");
            assert!(init.mcp_servers.is_empty());
        }
        _ => panic!("Expected System event, got {event:?}"),
    }
}

#[test]
fn parse_assistant_event() {
    let json = r#"{"type":"assistant","message":{"role":"assistant","content":"Hello"}}"#;
    let event: ClaudeEvent = serde_json::from_str(json).unwrap();

    match event {
        ClaudeEvent::Assistant { message } => {
            assert_eq!(message["role"], "assistant");
            assert_eq!(message["content"], "Hello");
        }
        _ => panic!("Expected Assistant event, got {event:?}"),
    }
}

#[test]
fn parse_tool_use_event() {
    let json = r#"{"type":"tool_use","id":"tool_123","name":"Read","input":{"file_path":"/tmp/test.txt"}}"#;
    let event: ClaudeEvent = serde_json::from_str(json).unwrap();

    match event {
        ClaudeEvent::ToolUse(tool_use) => {
            assert_eq!(tool_use.id, "tool_123");
            assert_eq!(tool_use.name, "Read");
            assert_eq!(tool_use.input["file_path"], "/tmp/test.txt");
        }
        _ => panic!("Expected ToolUse event, got {event:?}"),
    }
}

#[test]
fn parse_tool_result_event() {
    let json = r#"{"type":"tool_result","tool_use_id":"tool_123","content":"file contents here","is_error":false}"#;
    let event: ClaudeEvent = serde_json::from_str(json).unwrap();

    match event {
        ClaudeEvent::ToolResult(result) => {
            assert_eq!(result.tool_use_id, "tool_123");
            assert_eq!(result.content, "file contents here");
            assert!(!result.is_error);
        }
        _ => panic!("Expected ToolResult event, got {event:?}"),
    }
}

#[test]
fn parse_content_block_delta_text() {
    let json = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello world"}}"#;
    let event: ClaudeEvent = serde_json::from_str(json).unwrap();

    match event {
        ClaudeEvent::ContentBlockDelta { index, delta } => {
            assert_eq!(index, 0);
            match delta {
                ContentDelta::TextDelta { text } => {
                    assert_eq!(text, "Hello world");
                }
                _ => panic!("Expected TextDelta, got {delta:?}"),
            }
        }
        _ => panic!("Expected ContentBlockDelta event, got {event:?}"),
    }
}

#[test]
fn parse_content_block_delta_input_json() {
    let json = r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"key\":"}}"#;
    let event: ClaudeEvent = serde_json::from_str(json).unwrap();

    match event {
        ClaudeEvent::ContentBlockDelta { index, delta } => {
            assert_eq!(index, 1);
            match delta {
                ContentDelta::InputJsonDelta { partial_json } => {
                    assert_eq!(partial_json, r#"{"key":"#);
                }
                _ => panic!("Expected InputJsonDelta, got {delta:?}"),
            }
        }
        _ => panic!("Expected ContentBlockDelta event, got {event:?}"),
    }
}

#[test]
fn parse_content_block_start() {
    let json = r#"{"type":"content_block_start","index":0,"content_block":{"type":"text"}}"#;
    let event: ClaudeEvent = serde_json::from_str(json).unwrap();

    match event {
        ClaudeEvent::ContentBlockStart {
            index,
            content_block,
        } => {
            assert_eq!(index, 0);
            assert_eq!(content_block["type"], "text");
        }
        _ => panic!("Expected ContentBlockStart event, got {event:?}"),
    }
}

#[test]
fn parse_content_block_stop() {
    let json = r#"{"type":"content_block_stop","index":0}"#;
    let event: ClaudeEvent = serde_json::from_str(json).unwrap();

    match event {
        ClaudeEvent::ContentBlockStop { index } => {
            assert_eq!(index, 0);
        }
        _ => panic!("Expected ContentBlockStop event, got {event:?}"),
    }
}

#[test]
fn parse_message_start() {
    let json = r#"{"type":"message_start","message":{"id":"msg_123","type":"message"}}"#;
    let event: ClaudeEvent = serde_json::from_str(json).unwrap();

    match event {
        ClaudeEvent::MessageStart { message } => {
            assert_eq!(message["id"], "msg_123");
        }
        _ => panic!("Expected MessageStart event, got {event:?}"),
    }
}

#[test]
fn parse_message_stop() {
    let json = r#"{"type":"message_stop"}"#;
    let event: ClaudeEvent = serde_json::from_str(json).unwrap();

    assert!(matches!(event, ClaudeEvent::MessageStop));
}

#[test]
fn parse_result_event() {
    let json = r#"{"type":"result","result":"Task completed successfully","session_id":"abc123","cost_usd":0.05,"is_error":false,"duration_ms":1500}"#;
    let event: ClaudeEvent = serde_json::from_str(json).unwrap();

    match event {
        ClaudeEvent::Result(result) => {
            assert_eq!(result.result, "Task completed successfully");
            assert_eq!(result.session_id, "abc123");
            assert!((result.cost_usd.unwrap() - 0.05).abs() < f64::EPSILON);
            assert!(!result.is_error);
            assert_eq!(result.duration_ms, Some(1500));
        }
        _ => panic!("Expected Result event, got {event:?}"),
    }
}

#[test]
fn parse_unknown_event_as_other() {
    let json = r#"{"type":"some_future_event","data":"whatever"}"#;
    let event: ClaudeEvent = serde_json::from_str(json).unwrap();

    match event {
        ClaudeEvent::Other(value) => {
            assert_eq!(value.get("type").unwrap(), "some_future_event");
            assert_eq!(value.get("data").unwrap(), "whatever");
        }
        _ => panic!("Expected Other variant"),
    }
}

#[test]
fn parse_error_invalid_json() {
    let json = r"not valid json";
    let result: Result<ClaudeEvent, _> = serde_json::from_str(json);
    assert!(result.is_err());
}

#[test]
fn parse_missing_type_becomes_other() {
    let json = r#"{"message":"no type field"}"#;
    let event: ClaudeEvent = serde_json::from_str(json).unwrap();

    // Missing type field defaults to empty string, which becomes Other
    match event {
        ClaudeEvent::Other(value) => {
            assert_eq!(value.get("message").unwrap(), "no type field");
        }
        _ => panic!("Expected Other variant for missing type"),
    }
}

// Helper method tests

#[test]
fn is_terminal_for_result() {
    let result = ClaudeEvent::Result(ResultEvent {
        result: "done".to_string(),
        session_id: "abc".to_string(),
        cost_usd: None,
        is_error: false,
        duration_ms: None,
        extras: std::collections::HashMap::new(),
    });
    assert!(result.is_terminal());
}

#[test]
fn is_terminal_for_message_stop() {
    let event = ClaudeEvent::MessageStop;
    assert!(event.is_terminal());
}

#[test]
fn is_terminal_for_non_terminal() {
    let event = ClaudeEvent::Assistant {
        message: serde_json::json!({}),
    };
    assert!(!event.is_terminal());
}

#[test]
fn tool_name_for_tool_use() {
    let event = ClaudeEvent::ToolUse(ToolUse {
        id: "id".to_string(),
        name: "Read".to_string(),
        input: serde_json::json!({}),
    });
    assert_eq!(event.tool_name(), Some("Read"));
}

#[test]
fn tool_name_for_non_tool_event() {
    let event = ClaudeEvent::MessageStop;
    assert_eq!(event.tool_name(), None);
}

#[test]
fn session_id_for_system_init() {
    let event = ClaudeEvent::System(SystemInit {
        cwd: "/tmp".to_string(),
        tools: vec![],
        model: "claude-sonnet-4-20250514".to_string(),
        session_id: "session_abc".to_string(),
        mcp_servers: vec![],
        subtype: Some("init".to_string()),
        permission_mode: None,
        claude_code_version: None,
        agents: vec![],
        skills: vec![],
        slash_commands: vec![],
        extras: std::collections::HashMap::new(),
    });
    assert_eq!(event.session_id(), Some("session_abc"));
}

#[test]
fn session_id_for_result() {
    let event = ClaudeEvent::Result(ResultEvent {
        result: "done".to_string(),
        session_id: "session_xyz".to_string(),
        is_error: false,
        cost_usd: None,
        duration_ms: None,
        extras: std::collections::HashMap::new(),
    });
    assert_eq!(event.session_id(), Some("session_xyz"));
}

#[test]
fn session_id_for_other_events() {
    let event = ClaudeEvent::MessageStop;
    assert_eq!(event.session_id(), None);
}

// Serialization round-trip tests

#[test]
fn serialize_and_deserialize_tool_use() {
    let original = ClaudeEvent::ToolUse(ToolUse {
        id: "tool_456".to_string(),
        name: "Write".to_string(),
        input: serde_json::json!({"file_path": "/tmp/out.txt", "content": "hello"}),
    });

    let serialized = serde_json::to_string(&original).unwrap();
    let deserialized: ClaudeEvent = serde_json::from_str(&serialized).unwrap();

    assert_eq!(original, deserialized);
}

#[test]
fn serialize_and_deserialize_content_delta() {
    let original = ClaudeEvent::ContentBlockDelta {
        index: 5,
        delta: ContentDelta::TextDelta {
            text: "streaming text".to_string(),
        },
    };

    let serialized = serde_json::to_string(&original).unwrap();
    let deserialized: ClaudeEvent = serde_json::from_str(&serialized).unwrap();

    assert_eq!(original, deserialized);
}
