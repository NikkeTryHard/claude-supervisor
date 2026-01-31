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
pub fn truncate(s: &str, max_len: usize, raw_mode: bool) -> String {
    if raw_mode {
        return s.to_string();
    }
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
pub fn format_tool_input(input: &serde_json::Value, raw_mode: bool) -> String {
    match input {
        serde_json::Value::Object(map) => {
            let pairs: Vec<String> = map
                .iter()
                .map(|(k, v)| {
                    let value_str = match v {
                        serde_json::Value::String(s) => truncate(s, 50, raw_mode),
                        other => truncate(&other.to_string(), 50, raw_mode),
                    };
                    format!("{k}={value_str}")
                })
                .collect();
            pairs.join(", ")
        }
        other => truncate(&other.to_string(), DEFAULT_MAX_LEN, raw_mode),
    }
}

/// Print session start information.
pub fn print_session_start(model: &str, session_id: &str, raw_mode: bool) {
    println!(
        "{} {} model={}, session={}",
        timestamp().dimmed(),
        "[SESSION]".blue().bold(),
        model.cyan(),
        truncate(session_id, 20, raw_mode).dimmed()
    );
    let _ = io::stdout().flush();
}

/// Print session end information.
pub fn print_session_end(
    cost_usd: Option<f64>,
    is_error: bool,
    session_id: Option<&str>,
    result_msg: Option<&str>,
    raw_mode: bool,
) {
    let ts = timestamp();
    if is_error {
        println!(
            "{} {} Session ended with error {}",
            ts.dimmed(),
            "[SESSION]".red().bold(),
            session_id
                .map_or(String::new(), |id| format!(
                    "session_id={}",
                    truncate(id, 20, raw_mode)
                ))
                .dimmed()
        );
        if let Some(msg) = result_msg {
            if !msg.is_empty() {
                println!(
                    "{} {} {}",
                    ts.dimmed(),
                    "[ERROR]".red().bold(),
                    truncate(msg, 200, raw_mode).red()
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
            session_id
                .map_or(String::new(), |id| format!(
                    "session_id={}",
                    truncate(id, 20, raw_mode)
                ))
                .dimmed()
        );
    } else {
        println!(
            "{} {} Session completed {}",
            ts.dimmed(),
            "[SESSION]".blue().bold(),
            session_id
                .map_or(String::new(), |id| format!(
                    "session_id={}",
                    truncate(id, 20, raw_mode)
                ))
                .dimmed()
        );
    }
    let _ = io::stdout().flush();
}

/// Print a tool request.
pub fn print_tool_request(name: &str, input: &serde_json::Value, raw_mode: bool) {
    println!(
        "{} {} ({})",
        "[TOOL]".cyan().bold(),
        name.bold(),
        format_tool_input(input, raw_mode).dimmed()
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

/// Print tool result output.
pub fn print_tool_result(tool_use_id: &str, content: &str, is_error: bool, raw_mode: bool) {
    let id_short = truncate(tool_use_id, 12, raw_mode);
    let content_short = truncate(content, 150, raw_mode);
    if is_error {
        println!(
            "{} {} {}",
            "[RESULT]".red().bold(),
            id_short.dimmed(),
            content_short
        );
    } else {
        println!(
            "{} {} {}",
            "[RESULT]".green().bold(),
            id_short.dimmed(),
            content_short
        );
    }
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

/// Print raw event output (for verbose/raw mode).
pub fn print_raw_event(event_type: &str, event_json: &str) {
    println!(
        "{} {} {}",
        timestamp().dimmed(),
        format!("[{event_type}]").yellow().bold(),
        event_json
    );
    let _ = io::stdout().flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_short_string() {
        assert_eq!(truncate("hello", 10, false), "hello");
    }

    #[test]
    fn test_truncate_exact_length() {
        assert_eq!(truncate("hello", 5, false), "hello");
    }

    #[test]
    fn test_truncate_long_string() {
        assert_eq!(truncate("hello world", 8, false), "hello...");
    }

    #[test]
    fn test_truncate_very_short_max() {
        assert_eq!(truncate("hello", 3, false), "...");
        assert_eq!(truncate("hello", 2, false), "...");
        assert_eq!(truncate("hello", 0, false), "...");
    }

    #[test]
    fn test_truncate_raw_mode_no_truncation() {
        let long_string = "a".repeat(200);
        assert_eq!(truncate(&long_string, 10, true), long_string);
    }

    #[test]
    fn test_format_tool_input_object() {
        let input = serde_json::json!({
            "file_path": "/home/user/test.txt",
            "content": "hello"
        });
        let formatted = format_tool_input(&input, false);
        assert!(formatted.contains("file_path="));
        assert!(formatted.contains("content="));
    }

    #[test]
    fn test_format_tool_input_long_value() {
        let long_content = "a".repeat(100);
        let input = serde_json::json!({
            "content": long_content
        });
        let formatted = format_tool_input(&input, false);
        assert!(formatted.len() < 100);
        assert!(formatted.contains("..."));
    }

    #[test]
    fn test_format_tool_input_non_object() {
        let input = serde_json::json!("just a string");
        let formatted = format_tool_input(&input, false);
        assert!(formatted.contains("just a string"));
    }

    #[test]
    fn test_format_tool_input_number() {
        let input = serde_json::json!(42);
        let formatted = format_tool_input(&input, false);
        assert_eq!(formatted, "42");
    }

    #[test]
    fn test_print_tool_result_truncates_long_content() {
        let long_content = "a".repeat(200);
        let truncated = truncate(&long_content, 150, false);
        assert!(truncated.len() <= 150);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_format_tool_input_raw_mode_no_truncation() {
        let long_content = "a".repeat(100);
        let input = serde_json::json!({
            "content": long_content.clone()
        });
        let formatted = format_tool_input(&input, true);
        assert!(formatted.contains(&long_content));
        assert!(!formatted.contains("..."));
    }
}
