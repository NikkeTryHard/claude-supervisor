//! Knowledge layer for supervisor decision-making.
//!
//! Provides unified access to project knowledge from multiple sources:
//! - CLAUDE.md (project conventions)
//! - Session history (past Q&A)
//! - Memory file (learned facts)

mod claude_md;
mod history;
mod memory;
mod source;

pub use claude_md::*;
pub use history::*;
pub use memory::*;
pub use source::*;
