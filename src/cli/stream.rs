//! Stream parser for Claude Code stdout.

use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

use crate::cli::ClaudeEvent;

/// Error type for stream operations.
#[derive(thiserror::Error, Debug)]
pub enum StreamError {
    #[error("Failed to spawn Claude process: {0}")]
    SpawnError(#[from] std::io::Error),
    #[error("Failed to parse JSON: {0}")]
    ParseError(#[from] serde_json::Error),
    #[error("Process stdout not available")]
    NoStdout,
}

/// Spawn Claude Code in non-interactive mode.
///
/// # Errors
///
/// Returns `StreamError::SpawnError` if the process fails to spawn.
pub fn spawn_claude(task: &str) -> Result<Child, StreamError> {
    let child = Command::new("claude")
        .args(["-p", task, "--output-format", "stream-json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    Ok(child)
}

/// Parse a single line of stream-json output.
///
/// # Errors
///
/// Returns `StreamError::ParseError` if the JSON is invalid.
pub fn parse_event(line: &str) -> Result<ClaudeEvent, StreamError> {
    let event: ClaudeEvent = serde_json::from_str(line)?;
    Ok(event)
}

/// Read events from Claude process stdout.
///
/// # Errors
///
/// Returns `StreamError::NoStdout` if stdout is not available.
pub fn read_events(
    child: &mut Child,
) -> Result<impl futures_core::Stream<Item = Result<ClaudeEvent, StreamError>> + '_, StreamError> {
    let stdout = child.stdout.take().ok_or(StreamError::NoStdout)?;
    let reader = BufReader::new(stdout).lines();

    Ok(futures_util::stream::unfold(reader, |mut reader| async {
        match reader.next_line().await {
            Ok(Some(line)) => {
                let event = parse_event(&line);
                Some((event, reader))
            }
            Ok(None) => None,
            Err(e) => Some((Err(StreamError::SpawnError(e)), reader)),
        }
    }))
}
