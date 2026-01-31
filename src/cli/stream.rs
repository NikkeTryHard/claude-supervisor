//! Stream parser for Claude Code stdout.
//!
//! This module provides utilities for parsing the stream-json output
//! from Claude Code and routing events through channels.

use std::io::{self, Write};

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::sync::mpsc::{self, Receiver, Sender};

use crate::cli::events::RawClaudeEvent;
use crate::cli::ClaudeEvent;

/// Default buffer size for event channels.
pub const DEFAULT_CHANNEL_BUFFER: usize = 64;

/// Error type for stream operations.
#[derive(thiserror::Error, Debug)]
pub enum StreamError {
    /// Failed to spawn the Claude process.
    #[error("Failed to spawn Claude process: {0}")]
    SpawnError(#[from] std::io::Error),
    /// Failed to parse a JSON line.
    #[error("Failed to parse JSON: {input}")]
    ParseError {
        /// The input that failed to parse.
        input: String,
        /// The reason for the parse failure.
        reason: String,
    },
    /// Process stdout not available.
    #[error("Process stdout not available")]
    NoStdout,
    /// Failed to read from the stream.
    #[error("Failed to read from stream: {0}")]
    ReadError(std::io::Error),
    /// Channel was closed.
    #[error("Event channel closed")]
    ChannelClosed,
}

impl StreamError {
    /// Create a parse error with input and reason.
    pub fn parse_error(input: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::ParseError {
            input: input.into(),
            reason: reason.into(),
        }
    }
}

/// Parser for Claude Code stream-json output.
pub struct StreamParser;

impl StreamParser {
    /// Parse a single line of stream-json output.
    ///
    /// # Errors
    ///
    /// Returns `StreamError::ParseError` if the JSON is invalid.
    pub fn parse_line(line: &str) -> Result<ClaudeEvent, StreamError> {
        serde_json::from_str(line).map_err(|e| StreamError::ParseError {
            input: line.to_string(),
            reason: e.to_string(),
        })
    }

    /// Parse a single line into a `RawClaudeEvent`, preserving the original JSON.
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

    /// Parse events from an async reader and send them to a channel.
    ///
    /// This function reads lines from the provided reader, parses them
    /// as `ClaudeEvent`s, and sends valid events to the provided sender.
    /// Invalid lines are logged and skipped.
    ///
    /// # Errors
    ///
    /// Returns `StreamError::ReadError` if reading fails.
    /// Returns `StreamError::ChannelClosed` if the receiver is dropped.
    pub async fn parse_stdout<R>(stdout: R, tx: Sender<ClaudeEvent>) -> Result<(), StreamError>
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

            match Self::parse_line(&line) {
                Ok(event) => {
                    if tx.send(event).await.is_err() {
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

    /// Parse events from an async reader and send `RawClaudeEvent`s to a channel.
    ///
    /// This preserves the original JSON for each event.
    ///
    /// # Errors
    ///
    /// Returns `StreamError::ReadError` if reading fails.
    /// Returns `StreamError::ChannelClosed` if the receiver is dropped.
    pub async fn parse_stdout_raw<R>(
        stdout: R,
        tx: Sender<RawClaudeEvent>,
    ) -> Result<(), StreamError>
    where
        R: AsyncRead + Unpin,
    {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        while let Some(line) = lines.next_line().await.map_err(StreamError::ReadError)? {
            if line.trim().is_empty() {
                continue;
            }

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

    /// Create a channel that receives parsed events from a reader.
    ///
    /// This spawns a background task that reads from the provided reader
    /// and sends parsed events to the returned receiver.
    ///
    /// # Arguments
    ///
    /// * `stdout` - The async reader to parse events from
    /// * `buffer_size` - The channel buffer size
    ///
    /// # Returns
    ///
    /// A receiver that yields `ClaudeEvent`s as they are parsed.
    pub fn into_channel<R>(stdout: R, buffer_size: usize) -> Receiver<ClaudeEvent>
    where
        R: AsyncRead + Unpin + Send + 'static,
    {
        let (tx, rx) = mpsc::channel(buffer_size);

        tokio::spawn(async move {
            if let Err(e) = Self::parse_stdout(stdout, tx).await {
                tracing::error!(error = %e, "Stream parsing failed");
            }
        });

        rx
    }
}

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
}
