//! Watcher module for Claude Code conversation files.
//!
//! Provides JSONL parsing for session history files.

mod error;
mod jsonl;

pub use error::WatcherError;
pub use jsonl::*;
