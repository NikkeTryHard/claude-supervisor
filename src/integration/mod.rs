//! Integration module for connecting watcher and hooks.
//!
//! Provides the bridge between file watching events and hook handling.

mod bridge;

pub use bridge::WatcherHookBridge;
