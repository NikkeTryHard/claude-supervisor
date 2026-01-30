//! Integration tests for CLI module exports and functionality.

use claude_supervisor::cli::{
    ClaudeEvent, ClaudeProcess, ClaudeProcessBuilder, ContentDelta, ResultEvent, SpawnError,
    StreamError, StreamParser, SystemInit, ToolResult, ToolUse, DEFAULT_CHANNEL_BUFFER,
};

#[test]
fn all_event_types_exported() {
    // Verify all event types are accessible by creating instances
    let event: ClaudeEvent = ClaudeEvent::MessageStop;
    assert!(matches!(event, ClaudeEvent::MessageStop));

    let init = SystemInit {
        cwd: "/tmp".to_string(),
        tools: vec![],
        model: "claude-sonnet-4-20250514".to_string(),
        session_id: "test".to_string(),
        mcp_servers: vec![],
        subtype: Some("init".to_string()),
    };
    assert_eq!(init.subtype, Some("init".to_string()));

    let tool_use = ToolUse {
        id: "id".to_string(),
        name: "Read".to_string(),
        input: serde_json::json!({}),
    };
    assert_eq!(tool_use.name, "Read");

    let tool_result = ToolResult {
        tool_use_id: "id".to_string(),
        content: "output".to_string(),
        is_error: false,
    };
    assert_eq!(tool_result.content, "output");

    let delta = ContentDelta::TextDelta {
        text: "hello".to_string(),
    };
    assert!(matches!(delta, ContentDelta::TextDelta { .. }));

    let result = ResultEvent {
        result: "done".to_string(),
        session_id: "test".to_string(),
        is_error: false,
        cost_usd: None,
        duration_ms: None,
    };
    assert_eq!(result.result, "done");
}

#[test]
fn builder_is_clonable() {
    let builder = ClaudeProcessBuilder::new("task")
        .allowed_tools(&["Read", "Write"])
        .max_turns(10);

    let cloned = builder.clone();

    assert_eq!(builder.build_args(), cloned.build_args());
}

#[test]
fn events_are_serializable() {
    let event = ClaudeEvent::ToolUse(ToolUse {
        id: "123".to_string(),
        name: "Bash".to_string(),
        input: serde_json::json!({"command": "ls -la"}),
    });

    // Serialize
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("tool_use"));
    assert!(json.contains("Bash"));

    // Deserialize
    let parsed: ClaudeEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(event, parsed);
}

#[test]
fn helper_methods_work() {
    // is_terminal
    let result_event = ClaudeEvent::Result(ResultEvent {
        result: "done".to_string(),
        session_id: "abc".to_string(),
        is_error: false,
        cost_usd: Some(0.05),
        duration_ms: Some(1000),
    });
    assert!(result_event.is_terminal());
    assert!(ClaudeEvent::MessageStop.is_terminal());

    // tool_name
    let tool_event = ClaudeEvent::ToolUse(ToolUse {
        id: "id".to_string(),
        name: "Edit".to_string(),
        input: serde_json::json!({}),
    });
    assert_eq!(tool_event.tool_name(), Some("Edit"));
    assert_eq!(ClaudeEvent::MessageStop.tool_name(), None);

    // session_id
    let system_event = ClaudeEvent::System(SystemInit {
        cwd: "/home/user".to_string(),
        tools: vec!["Read".to_string()],
        model: "claude-sonnet-4-20250514".to_string(),
        session_id: "session_123".to_string(),
        mcp_servers: vec![],
        subtype: Some("init".to_string()),
    });
    assert_eq!(system_event.session_id(), Some("session_123"));
    assert_eq!(result_event.session_id(), Some("abc"));
    assert_eq!(ClaudeEvent::MessageStop.session_id(), None);
}

#[test]
fn error_types_are_debug() {
    let spawn_err = SpawnError::NotFound;
    let spawn_debug = format!("{spawn_err:?}");
    assert!(spawn_debug.contains("NotFound"));

    let stream_err = StreamError::NoStdout;
    let stream_debug = format!("{stream_err:?}");
    assert!(stream_debug.contains("NoStdout"));
}

#[test]
fn error_types_display() {
    let spawn_err = SpawnError::NotFound;
    let spawn_display = format!("{spawn_err}");
    assert!(spawn_display.contains("not found"));

    let stream_err = StreamError::ChannelClosed;
    let stream_display = format!("{stream_err}");
    assert!(stream_display.contains("closed"));
}

#[test]
fn default_channel_buffer_constant() {
    assert_eq!(DEFAULT_CHANNEL_BUFFER, 64);
}

#[test]
fn stream_parser_parse_line_works() {
    let line = r#"{"type":"message_stop"}"#;
    let result = StreamParser::parse_line(line);
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), ClaudeEvent::MessageStop));
}

#[test]
fn process_builder_builds_correct_args() {
    let builder = ClaudeProcessBuilder::new("Fix the bug")
        .allowed_tools(&["Read", "Write", "Bash"])
        .max_turns(5)
        .resume("session_abc")
        .append_system_prompt("Be careful");

    let args = builder.build_args();

    // Verify required args
    assert!(args.contains(&"-p".to_string()));
    assert!(args.contains(&"Fix the bug".to_string()));
    assert!(args.contains(&"--output-format".to_string()));
    assert!(args.contains(&"stream-json".to_string()));

    // Verify optional args
    assert!(args.contains(&"--allowedTools".to_string()));
    assert!(args.contains(&"Read,Write,Bash".to_string()));
    assert!(args.contains(&"--max-turns".to_string()));
    assert!(args.contains(&"5".to_string()));
    assert!(args.contains(&"--resume".to_string()));
    assert!(args.contains(&"session_abc".to_string()));
    assert!(args.contains(&"--append-system-prompt".to_string()));
    assert!(args.contains(&"Be careful".to_string()));
}

#[test]
fn spawn_with_invalid_binary_fails() {
    let builder = ClaudeProcessBuilder::new("test");
    let result = ClaudeProcess::spawn_with_binary("__nonexistent_binary__", &builder);

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), SpawnError::NotFound));
}
