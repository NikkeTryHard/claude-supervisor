//! Tests for stream parsing and channel integration.

use claude_supervisor::cli::{ClaudeEvent, ContentDelta, StreamError, StreamParser};

#[test]
fn parse_line_valid_json() {
    let line = r#"{"type":"message_stop"}"#;
    let result = StreamParser::parse_line(line);

    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), ClaudeEvent::MessageStop));
}

#[test]
fn parse_line_tool_use() {
    let line = r#"{"type":"tool_use","tool_use_id":"123","name":"Read","input":{}}"#;
    let result = StreamParser::parse_line(line);

    assert!(result.is_ok());
    let event = result.unwrap();
    assert_eq!(event.tool_name(), Some("Read"));
}

#[test]
fn parse_line_content_delta() {
    let line =
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#;
    let result = StreamParser::parse_line(line);

    assert!(result.is_ok());
    match result.unwrap() {
        ClaudeEvent::ContentBlockDelta { index, delta } => {
            assert_eq!(index, 0);
            match delta {
                ContentDelta::TextDelta { text } => assert_eq!(text, "Hello"),
                _ => panic!("Expected TextDelta"),
            }
        }
        _ => panic!("Expected ContentBlockDelta"),
    }
}

#[test]
fn parse_line_invalid_json() {
    let line = "not valid json at all";
    let result = StreamParser::parse_line(line);

    assert!(result.is_err());
    match result.unwrap_err() {
        StreamError::ParseError { input, reason: _ } => {
            assert_eq!(input, "not valid json at all");
        }
        other => panic!("Expected ParseError, got {other:?}"),
    }
}

#[test]
fn parse_line_empty_string() {
    let line = "";
    let result = StreamParser::parse_line(line);

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        StreamError::ParseError { .. }
    ));
}

#[test]
fn parse_line_whitespace_only() {
    let line = "   \t\n  ";
    let result = StreamParser::parse_line(line);

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        StreamError::ParseError { .. }
    ));
}

#[test]
fn parse_line_unknown_event_type() {
    let line = r#"{"type":"future_event_type","data":"something"}"#;
    let result = StreamParser::parse_line(line);

    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), ClaudeEvent::Unknown));
}

#[test]
fn default_channel_buffer_size() {
    assert_eq!(claude_supervisor::cli::DEFAULT_CHANNEL_BUFFER, 64);
}

#[tokio::test]
async fn into_channel_receives_events() {
    use tokio::io::AsyncWriteExt;

    // Create a mock stdout using a pipe
    let (reader, mut writer) = tokio::io::duplex(1024);

    // Spawn task to write test data
    tokio::spawn(async move {
        writer
            .write_all(b"{\"type\":\"message_stop\"}\n")
            .await
            .unwrap();
        writer
            .write_all(b"{\"type\":\"message_start\",\"message\":{}}\n")
            .await
            .unwrap();
        // Close the writer to signal EOF
        drop(writer);
    });

    let mut rx = StreamParser::into_channel(reader, 16);

    // Should receive first event
    let event1 = rx.recv().await;
    assert!(event1.is_some());
    assert!(matches!(event1.unwrap(), ClaudeEvent::MessageStop));

    // Should receive second event
    let event2 = rx.recv().await;
    assert!(event2.is_some());
    assert!(matches!(event2.unwrap(), ClaudeEvent::MessageStart { .. }));

    // Channel should close after EOF
    let event3 = rx.recv().await;
    assert!(event3.is_none());
}

#[tokio::test]
async fn into_channel_skips_invalid_lines() {
    use tokio::io::AsyncWriteExt;

    let (reader, mut writer) = tokio::io::duplex(1024);

    tokio::spawn(async move {
        // Valid event
        writer
            .write_all(b"{\"type\":\"message_stop\"}\n")
            .await
            .unwrap();
        // Invalid JSON (will be skipped with tracing warning)
        writer.write_all(b"invalid json\n").await.unwrap();
        // Another valid event
        writer
            .write_all(b"{\"type\":\"message_stop\"}\n")
            .await
            .unwrap();
        drop(writer);
    });

    let mut rx = StreamParser::into_channel(reader, 16);

    // Should receive first valid event
    let event1 = rx.recv().await;
    assert!(event1.is_some());

    // Should receive second valid event (invalid line skipped)
    let event2 = rx.recv().await;
    assert!(event2.is_some());

    // Channel should close
    let event3 = rx.recv().await;
    assert!(event3.is_none());
}

#[tokio::test]
async fn into_channel_handles_empty_lines() {
    use tokio::io::AsyncWriteExt;

    let (reader, mut writer) = tokio::io::duplex(1024);

    tokio::spawn(async move {
        writer.write_all(b"\n").await.unwrap();
        writer.write_all(b"   \n").await.unwrap();
        writer
            .write_all(b"{\"type\":\"message_stop\"}\n")
            .await
            .unwrap();
        drop(writer);
    });

    let mut rx = StreamParser::into_channel(reader, 16);

    // Should receive the valid event (empty lines skipped)
    let event = rx.recv().await;
    assert!(event.is_some());
    assert!(matches!(event.unwrap(), ClaudeEvent::MessageStop));

    // Channel should close
    assert!(rx.recv().await.is_none());
}

#[tokio::test]
async fn parse_stdout_sends_to_channel() {
    use tokio::io::AsyncWriteExt;

    let (reader, mut writer) = tokio::io::duplex(1024);
    let (tx, mut rx) = tokio::sync::mpsc::channel(16);

    // Write events
    tokio::spawn(async move {
        writer
            .write_all(b"{\"type\":\"message_stop\"}\n")
            .await
            .unwrap();
        drop(writer);
    });

    // Parse stdout
    let result = StreamParser::parse_stdout(reader, tx).await;
    assert!(result.is_ok());

    // Should have received the event
    let event = rx.recv().await;
    assert!(event.is_some());
    assert!(matches!(event.unwrap(), ClaudeEvent::MessageStop));
}
