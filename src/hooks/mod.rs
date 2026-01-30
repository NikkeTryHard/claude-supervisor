//! Hook handlers for Claude Code events.
//!
//! This module provides handlers for Claude Code hook events including:
//! - `PreToolUse`: Evaluate and optionally modify tool calls before execution
//! - `Stop`: Control whether Claude should stop or continue working
//!
//! # Components
//!
//! - [`HookHandler`]: Main handler that processes hook events
//! - [`IterationTracker`]: Tracks iteration counts per session
//! - [`CompletionDetector`]: Detects task completion from Claude's responses

mod completion;
mod handler;
mod input;
mod iteration;
mod pre_tool_use;
mod stop;

pub use completion::*;
pub use handler::*;
pub use input::*;
pub use iteration::*;
pub use pre_tool_use::*;
pub use stop::*;
