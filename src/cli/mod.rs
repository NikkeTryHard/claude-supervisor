//! CLI module for Claude Code process spawning and stream parsing.
//!
//! This module provides the core functionality for running Claude Code
//! in non-interactive mode and parsing its stream-json output.
//!
//! ## Architecture
//!
//! ```text
//! ClaudeProcessBuilder --> ClaudeProcess --> stdout
//!                                              |
//!                                              v
//!                                        StreamParser
//!                                              |
//!                                              v
//!                                     mpsc::Receiver<ClaudeEvent>
//! ```
//!
//! ## Usage
//!
//! ```no_run
//! use claude_supervisor::cli::{
//!     ClaudeProcess, ClaudeProcessBuilder, StreamParser, DEFAULT_CHANNEL_BUFFER
//! };
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Build the process configuration
//! let builder = ClaudeProcessBuilder::new("Fix the authentication bug")
//!     .allowed_tools(&["Read", "Write", "Bash"])
//!     .max_turns(10);
//!
//! // Spawn the process
//! let mut process = ClaudeProcess::spawn(&builder)?;
//!
//! // Get the event stream
//! let stdout = process.take_stdout().expect("stdout available");
//! let mut rx = StreamParser::into_channel(stdout, DEFAULT_CHANNEL_BUFFER);
//!
//! // Process events
//! while let Some(event) = rx.recv().await {
//!     if event.is_terminal() {
//!         println!("Session complete");
//!         break;
//!     }
//!     if let Some(tool) = event.tool_name() {
//!         println!("Tool called: {}", tool);
//!     }
//! }
//!
//! // Wait for process to finish
//! process.wait().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Event Types
//!
//! The [`ClaudeEvent`] enum represents all possible events from Claude Code:
//!
//! - [`ClaudeEvent::System`] - Session initialization with available tools
//! - [`ClaudeEvent::Assistant`] - Assistant message content
//! - [`ClaudeEvent::ToolUse`] - Tool invocation request
//! - [`ClaudeEvent::ToolResult`] - Tool execution result
//! - [`ClaudeEvent::ContentBlockDelta`] - Streaming content updates
//! - [`ClaudeEvent::Result`] - Final session result with cost/duration
//!
//! ## Error Handling
//!
//! Two error types are provided:
//!
//! - [`SpawnError`] - Errors when spawning the Claude process
//! - [`StreamError`] - Errors when parsing the output stream

mod events;
mod process;
mod stream;

pub use events::*;
pub use process::*;
pub use stream::*;
