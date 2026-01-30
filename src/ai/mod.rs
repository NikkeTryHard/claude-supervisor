//! AI client module for supervisor decisions.

mod client;
mod context;
mod prompts;

pub use client::*;
pub use context::ContextCompressor;
pub use prompts::{
    format_tool_review, format_tool_review_with_context, SupervisorContext,
    SUPERVISOR_SYSTEM_PROMPT,
};
