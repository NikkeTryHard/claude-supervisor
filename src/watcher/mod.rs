//! Watcher module for Claude Code conversation files.
//!
//! Provides JSONL parsing for session history files.

mod error;
mod jsonl;
mod tailer;

pub use error::WatcherError;
pub use jsonl::*;
pub use tailer::JsonlTailer;
