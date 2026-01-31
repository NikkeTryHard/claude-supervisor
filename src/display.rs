//! Colored CLI display utilities for supervisor output.
//!
//! This module provides functions for printing colored, formatted output
//! to the terminal during Claude Code supervision.

use std::io::{self, Write};

use chrono::Utc;
use owo_colors::OwoColorize;

/// Get current timestamp in the same format as tracing.
fn timestamp() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()
}

/// Maximum length for truncated display strings.
const DEFAULT_MAX_LEN: usize = 80;

/// Truncate a string to a maximum length, adding ellipsis if truncated.
#[must_use]
pub fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else if max_len <= 3 {
        "...".to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

/// Format tool input for display, truncating long values.
#[must_use]
pub fn format_tool_input(input: &serde_json::Value) -> String {
    match input {
        serde_json::Value::Object(map) => {
            let pairs: Vec<String> = map
                .iter()
                .map(|(k, v)| {
                    let value_str = match v {
                        serde_json::Value::String(s) => truncate(s, 50),
                        other => truncate(&other.to_string(), 50),
                    };
                    format!("{k}={value_str}")
                })
                .collect();
            pairs.join(", ")
        }
        other => truncate(&other.to_string(), DEFAULT_MAX_LEN),
    }
}

/// Print session start information.
pub fn print_session_start(model: &str, session_id: &str) {
    println!(
        "{} {} model={}, session={}",
        timestamp().dimmed(),
        "[SESSION]".blue().bold(),
        model.cyan(),
        truncate(session_id, 20).dimmed()
    );
    let _ = io::stdout().flush();
}

/// Print session end information.
pub fn print_session_end(
    cost_usd: Option<f64>,
    is_error: bool,
    session_id: Option<&str>,
    result_msg: Option<&str>,
) {
    let ts = timestamp();
    let sid_str = session_id.map_or(String::new(), |id| {
        format!("session_id={}", truncate(id, 20))
    });
    let sid = sid_str.dimmed();
    if is_error {
        println!(
            "{} {} Session ended with error {}",
            ts.dimmed(),
            "[SESSION]".red().bold(),
            sid
        );
        if let Some(msg) = result_msg {
            if !msg.is_empty() {
                println!(
                    "{} {} {}",
                    ts.dimmed(),
                    "[ERROR]".red().bold(),
                    truncate(msg, 200).red()
                );
            }
        }
        tracing::debug!("Session ended with is_error=true. Check Claude Code logs for details.");
    } else if let Some(cost) = cost_usd {
        println!(
            "{} {} Session completed (cost: ${:.4}) {}",
            ts.dimmed(),
            "[SESSION]".blue().bold(),
            cost,
            sid
        );
    } else {
        println!(
            "{} {} Session completed {}",
            ts.dimmed(),
            "[SESSION]".blue().bold(),
            sid
        );
    }
    let _ = io::stdout().flush();
}

/// Print a tool request.
pub fn print_tool_request(name: &str, input: &serde_json::Value) {
    println!(
        "{} {} ({})",
        "[TOOL]".cyan().bold(),
        name.bold(),
        format_tool_input(input).dimmed()
    );
    let _ = io::stdout().flush();
}

/// Print tool allow decision.
pub fn print_allow(tool_name: &str) {
    println!("{} {}", "[ALLOW]".green().bold(), tool_name);
    let _ = io::stdout().flush();
}

/// Print tool deny decision.
pub fn print_deny(tool_name: &str, reason: &str) {
    println!(
        "{} {} - {}",
        "[DENY]".red().bold(),
        tool_name,
        reason.dimmed()
    );
    let _ = io::stdout().flush();
}

/// Print escalation to AI supervisor.
pub fn print_escalate(tool_name: &str, reason: &str) {
    println!(
        "{} {} - {}",
        "[ESCALATE]".yellow().bold(),
        tool_name,
        reason.dimmed()
    );
    let _ = io::stdout().flush();
}

/// Print AI supervisor decision.
pub fn print_supervisor_decision(decision: &str, tool_name: &str) {
    println!(
        "{} {} -> {}",
        "[SUPERVISOR]".magenta().bold(),
        tool_name,
        decision
    );
    let _ = io::stdout().flush();
}

/// Print thinking content (dimmed).
pub fn print_thinking(text: &str) {
    print!("{}", text.dimmed());
    let _ = io::stdout().flush();
}

/// Print text content.
pub fn print_text(text: &str) {
    print!("{text}");
    let _ = io::stdout().flush();
}

/// Print an error message.
pub fn print_error(message: &str) {
    println!("{} {}", "[ERROR]".red().bold(), message);
    let _ = io::stdout().flush();
}

/// Print AI provider connection test result.
pub fn print_connection_test(provider: &str, model: &str, success: bool) {
    let ts = timestamp();
    if success {
        println!(
            "{} {} {} ({}) - {}",
            ts.dimmed(),
            "[AI]".magenta().bold(),
            provider.cyan(),
            model.dimmed(),
            "connected".green()
        );
    } else {
        println!(
            "{} {} {} ({}) - {}",
            ts.dimmed(),
            "[AI]".magenta().bold(),
            provider.cyan(),
            model.dimmed(),
            "failed".red()
        );
    }
    let _ = io::stdout().flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_exact_length() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_long_string() {
        assert_eq!(truncate("hello world", 8), "hello...");
    }

    #[test]
    fn test_truncate_very_short_max() {
        assert_eq!(truncate("hello", 3), "...");
        assert_eq!(truncate("hello", 2), "...");
        assert_eq!(truncate("hello", 0), "...");
    }

    #[test]
    fn test_format_tool_input_object() {
        let input = serde_json::json!({
            "file_path": "/home/user/test.txt",
            "content": "hello"
        });
        let formatted = format_tool_input(&input);
        assert!(formatted.contains("file_path="));
        assert!(formatted.contains("content="));
    }

    #[test]
    fn test_format_tool_input_long_value() {
        let long_content = "a".repeat(100);
        let input = serde_json::json!({
            "content": long_content
        });
        let formatted = format_tool_input(&input);
        assert!(formatted.len() < 100);
        assert!(formatted.contains("..."));
    }

    #[test]
    fn test_format_tool_input_non_object() {
        let input = serde_json::json!("just a string");
        let formatted = format_tool_input(&input);
        assert!(formatted.contains("just a string"));
    }

    #[test]
    fn test_format_tool_input_number() {
        let input = serde_json::json!(42);
        let formatted = format_tool_input(&input);
        assert_eq!(formatted, "42");
    }
}
