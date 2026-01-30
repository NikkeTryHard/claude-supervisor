//! AI client module for supervisor decisions.

mod boss;
mod client;
mod context;
mod prompts;

pub use boss::{format_boss_prompt, BossDecision, BOSS_SYSTEM_PROMPT};
pub use client::*;
pub use context::ContextCompressor;
pub use prompts::{
    format_tool_review, format_tool_review_with_context, SupervisorContext,
    SUPERVISOR_SYSTEM_PROMPT,
};
