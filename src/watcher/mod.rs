//! Watcher module for Claude Code conversation files.
//!
//! Provides JSONL parsing for session history files.

mod discovery;
mod error;
mod jsonl;
mod reconstructor;
mod session_watcher;
mod tailer;

pub use discovery::{
    discover_session, find_latest_session, find_project_sessions_dir, find_session_by_id,
    project_path_hash,
};
pub use error::WatcherError;
pub use jsonl::*;
pub use reconstructor::{SessionReconstructor, ToolCallRecord};
pub use session_watcher::{SessionWatcher, WatcherEvent};
pub use tailer::JsonlTailer;
