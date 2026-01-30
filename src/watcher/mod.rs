//! Watcher module for Claude Code conversation files.
//!
//! Provides JSONL parsing for session history files.

mod error;
mod jsonl;
mod session_watcher;
mod tailer;

pub use error::WatcherError;
pub use jsonl::*;
pub use session_watcher::{SessionWatcher, WatcherEvent};
pub use tailer::JsonlTailer;
