//! Pattern detection for stuck agent behaviors.
//!
//! Detects repetitive patterns in tool calls that indicate an agent
//! may be stuck in a loop or unable to make progress.

use super::ToolCallRecord;

/// Detected pattern indicating a stuck agent.
#[derive(Debug, Clone, PartialEq)]
pub enum StuckPattern {
    /// Agent is repeating the same action multiple times.
    RepeatingAction {
        /// Name of the repeated tool.
        tool_name: String,
        /// Number of consecutive repetitions.
        count: usize,
    },
    /// Agent is repeatedly encountering errors from the same tool.
    RepeatingError {
        /// Name of the tool producing errors.
        tool_name: String,
        /// Number of consecutive errors.
        count: usize,
    },
    /// Agent is alternating between two actions without progress.
    AlternatingActions {
        /// First tool in the alternation.
        tool_a: String,
        /// Second tool in the alternation.
        tool_b: String,
        /// Number of complete A-B cycles.
        cycles: usize,
    },
}

impl std::fmt::Display for StuckPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RepeatingAction { tool_name, count } => {
                write!(f, "Repeating {tool_name} {count} times")
            }
            Self::RepeatingError { tool_name, count } => {
                write!(f, "Repeating {tool_name} errors {count} times")
            }
            Self::AlternatingActions {
                tool_a,
                tool_b,
                cycles,
            } => {
                write!(f, "Alternating {tool_a}/{tool_b} {cycles} cycles")
            }
        }
    }
}

/// Thresholds for pattern detection.
#[derive(Debug, Clone)]
pub struct PatternThresholds {
    /// Minimum consecutive repetitions to trigger `RepeatingAction`.
    pub repeating_action: usize,
    /// Minimum consecutive errors to trigger `RepeatingError`.
    pub repeating_error: usize,
    /// Minimum A-B cycles to trigger `AlternatingActions`.
    pub alternating_cycles: usize,
    /// Maximum number of recent calls to analyze.
    pub window_size: usize,
}

impl Default for PatternThresholds {
    fn default() -> Self {
        Self {
            repeating_action: 4,
            repeating_error: 3,
            alternating_cycles: 3,
            window_size: 20,
        }
    }
}

/// Detects stuck patterns in tool call sequences.
#[derive(Debug, Clone, Default)]
pub struct PatternDetector {
    thresholds: PatternThresholds,
}

impl PatternDetector {
    /// Create a new pattern detector with default thresholds.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a pattern detector with custom thresholds.
    #[must_use]
    pub fn with_thresholds(thresholds: PatternThresholds) -> Self {
        Self { thresholds }
    }

    /// Get the current thresholds.
    #[must_use]
    pub fn thresholds(&self) -> &PatternThresholds {
        &self.thresholds
    }

    /// Detect stuck patterns in a sequence of tool calls.
    ///
    /// Checks for patterns in priority order:
    /// 1. Repeating errors (highest priority - indicates failure)
    /// 2. Repeating actions (same tool with similar input)
    /// 3. Alternating actions (A-B-A-B pattern)
    ///
    /// Returns the first detected pattern, or `None` if no pattern found.
    #[must_use]
    pub fn detect(&self, calls: &[ToolCallRecord]) -> Option<StuckPattern> {
        if calls.is_empty() {
            return None;
        }

        // Take only the most recent calls within window
        let window_start = calls.len().saturating_sub(self.thresholds.window_size);
        let window = &calls[window_start..];

        // Check for repeating errors first (highest priority)
        if let Some(pattern) = self.detect_repeating_errors(window) {
            return Some(pattern);
        }

        // Check for repeating actions
        if let Some(pattern) = self.detect_repeating_actions(window) {
            return Some(pattern);
        }

        // Check for alternating actions
        if let Some(pattern) = self.detect_alternating_actions(window) {
            return Some(pattern);
        }

        None
    }

    /// Detect consecutive errors from the same tool.
    fn detect_repeating_errors(&self, calls: &[ToolCallRecord]) -> Option<StuckPattern> {
        if calls.len() < self.thresholds.repeating_error {
            return None;
        }

        let mut count = 0;
        let mut current_tool: Option<&str> = None;

        // Scan from the end to find consecutive errors
        for call in calls.iter().rev() {
            if call.is_error {
                match current_tool {
                    Some(tool) if tool == call.tool_name => {
                        count += 1;
                    }
                    Some(_) => {
                        // Different tool, reset
                        current_tool = Some(&call.tool_name);
                        count = 1;
                    }
                    None => {
                        current_tool = Some(&call.tool_name);
                        count = 1;
                    }
                }
            } else {
                // Non-error breaks the chain
                break;
            }
        }

        if count >= self.thresholds.repeating_error {
            current_tool.map(|tool| StuckPattern::RepeatingError {
                tool_name: tool.to_string(),
                count,
            })
        } else {
            None
        }
    }

    /// Detect consecutive calls to the same tool with similar input.
    fn detect_repeating_actions(&self, calls: &[ToolCallRecord]) -> Option<StuckPattern> {
        if calls.len() < self.thresholds.repeating_action {
            return None;
        }

        let mut count = 1;
        let mut current_tool: Option<&str> = None;
        let mut current_input: Option<&serde_json::Value> = None;

        // Scan from the end to find consecutive same-tool calls
        for call in calls.iter().rev() {
            if let (Some(tool), Some(input)) = (current_tool, current_input) {
                if tool == call.tool_name && self.inputs_similar(input, &call.input) {
                    count += 1;
                } else {
                    // Different tool or input, check if we have enough
                    if count >= self.thresholds.repeating_action {
                        return Some(StuckPattern::RepeatingAction {
                            tool_name: tool.to_string(),
                            count,
                        });
                    }
                    // Reset
                    current_tool = Some(&call.tool_name);
                    current_input = Some(&call.input);
                    count = 1;
                }
            } else {
                current_tool = Some(&call.tool_name);
                current_input = Some(&call.input);
            }
        }

        if count >= self.thresholds.repeating_action {
            current_tool.map(|tool| StuckPattern::RepeatingAction {
                tool_name: tool.to_string(),
                count,
            })
        } else {
            None
        }
    }

    /// Detect alternating A-B-A-B patterns.
    fn detect_alternating_actions(&self, calls: &[ToolCallRecord]) -> Option<StuckPattern> {
        let min_calls = self.thresholds.alternating_cycles * 2;
        if calls.len() < min_calls {
            return None;
        }

        // Start from the most recent calls
        let recent = &calls[calls.len().saturating_sub(min_calls + 2)..];
        if recent.len() < 4 {
            return None;
        }

        // Check if we have an A-B-A-B pattern
        let tool_a = &recent[recent.len() - 1].tool_name;
        let tool_b = &recent[recent.len() - 2].tool_name;

        if tool_a == tool_b {
            return None; // Not alternating
        }

        let mut cycles = 0;
        let mut expecting_a = true;

        for call in recent.iter().rev() {
            let expected = if expecting_a { tool_a } else { tool_b };
            if &call.tool_name == expected {
                if expecting_a {
                    cycles += 1;
                }
                expecting_a = !expecting_a;
            } else {
                break;
            }
        }

        if cycles >= self.thresholds.alternating_cycles {
            Some(StuckPattern::AlternatingActions {
                tool_a: tool_a.clone(),
                tool_b: tool_b.clone(),
                cycles,
            })
        } else {
            None
        }
    }

    /// Check if two inputs are semantically similar.
    ///
    /// Currently uses exact JSON equality. Future versions may use
    /// fuzzy matching for better detection.
    #[allow(clippy::unused_self)]
    fn inputs_similar(&self, a: &serde_json::Value, b: &serde_json::Value) -> bool {
        a == b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_call(tool_name: &str, is_error: bool) -> ToolCallRecord {
        ToolCallRecord {
            tool_use_id: format!("tool-{}", rand_id()),
            tool_name: tool_name.to_string(),
            input: serde_json::json!({"test": "input"}),
            result: None,
            is_error,
            timestamp: "2026-01-30T10:00:00Z".to_string(),
        }
    }

    fn create_call_with_input(tool_name: &str, input: serde_json::Value) -> ToolCallRecord {
        ToolCallRecord {
            tool_use_id: format!("tool-{}", rand_id()),
            tool_name: tool_name.to_string(),
            input,
            result: None,
            is_error: false,
            timestamp: "2026-01-30T10:00:00Z".to_string(),
        }
    }

    fn rand_id() -> u32 {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        COUNTER.fetch_add(1, Ordering::Relaxed)
    }

    // StuckPattern Display tests
    #[test]
    fn test_stuck_pattern_display_repeating_action() {
        let pattern = StuckPattern::RepeatingAction {
            tool_name: "Bash".to_string(),
            count: 5,
        };
        assert_eq!(pattern.to_string(), "Repeating Bash 5 times");
    }

    #[test]
    fn test_stuck_pattern_display_repeating_error() {
        let pattern = StuckPattern::RepeatingError {
            tool_name: "Read".to_string(),
            count: 3,
        };
        assert_eq!(pattern.to_string(), "Repeating Read errors 3 times");
    }

    #[test]
    fn test_stuck_pattern_display_alternating() {
        let pattern = StuckPattern::AlternatingActions {
            tool_a: "Grep".to_string(),
            tool_b: "Read".to_string(),
            cycles: 4,
        };
        assert_eq!(pattern.to_string(), "Alternating Grep/Read 4 cycles");
    }

    // PatternThresholds tests
    #[test]
    fn test_pattern_thresholds_default() {
        let thresholds = PatternThresholds::default();
        assert_eq!(thresholds.repeating_action, 4);
        assert_eq!(thresholds.repeating_error, 3);
        assert_eq!(thresholds.alternating_cycles, 3);
        assert_eq!(thresholds.window_size, 20);
    }

    // PatternDetector tests
    #[test]
    fn test_pattern_detector_new() {
        let detector = PatternDetector::new();
        assert_eq!(detector.thresholds().repeating_action, 4);
    }

    #[test]
    fn test_pattern_detector_with_thresholds() {
        let thresholds = PatternThresholds {
            repeating_action: 2,
            repeating_error: 2,
            alternating_cycles: 2,
            window_size: 10,
        };
        let detector = PatternDetector::with_thresholds(thresholds);
        assert_eq!(detector.thresholds().repeating_action, 2);
    }

    #[test]
    fn test_detect_empty_calls() {
        let detector = PatternDetector::new();
        assert!(detector.detect(&[]).is_none());
    }

    #[test]
    fn test_detect_no_pattern() {
        let detector = PatternDetector::new();
        let calls = vec![
            create_call("Read", false),
            create_call("Grep", false),
            create_call("Edit", false),
        ];
        assert!(detector.detect(&calls).is_none());
    }

    #[test]
    fn test_detect_repeating_errors() {
        let detector = PatternDetector::new();
        let calls = vec![
            create_call("Read", false),
            create_call("Bash", true),
            create_call("Bash", true),
            create_call("Bash", true),
        ];

        let pattern = detector.detect(&calls);
        assert!(pattern.is_some());
        assert!(matches!(
            pattern.unwrap(),
            StuckPattern::RepeatingError { tool_name, count } if tool_name == "Bash" && count == 3
        ));
    }

    #[test]
    fn test_detect_repeating_errors_priority() {
        // Errors should be detected even if there are also repeating actions
        let detector = PatternDetector::new();
        let calls = vec![
            create_call("Read", false),
            create_call("Read", false),
            create_call("Read", false),
            create_call("Read", false),
            create_call("Bash", true),
            create_call("Bash", true),
            create_call("Bash", true),
        ];

        let pattern = detector.detect(&calls);
        assert!(pattern.is_some());
        // Errors have priority
        assert!(matches!(
            pattern.unwrap(),
            StuckPattern::RepeatingError { tool_name, .. } if tool_name == "Bash"
        ));
    }

    #[test]
    fn test_detect_repeating_actions() {
        let detector = PatternDetector::new();
        let input = serde_json::json!({"path": "/tmp/test.txt"});
        let calls = vec![
            create_call_with_input("Read", input.clone()),
            create_call_with_input("Read", input.clone()),
            create_call_with_input("Read", input.clone()),
            create_call_with_input("Read", input.clone()),
        ];

        let pattern = detector.detect(&calls);
        assert!(pattern.is_some());
        assert!(matches!(
            pattern.unwrap(),
            StuckPattern::RepeatingAction { tool_name, count } if tool_name == "Read" && count == 4
        ));
    }

    #[test]
    fn test_detect_repeating_actions_different_input_no_match() {
        let detector = PatternDetector::new();
        let calls = vec![
            create_call_with_input("Read", serde_json::json!({"path": "/a.txt"})),
            create_call_with_input("Read", serde_json::json!({"path": "/b.txt"})),
            create_call_with_input("Read", serde_json::json!({"path": "/c.txt"})),
            create_call_with_input("Read", serde_json::json!({"path": "/d.txt"})),
        ];

        // Different inputs, so no repeating action pattern
        assert!(detector.detect(&calls).is_none());
    }

    #[test]
    fn test_detect_alternating_actions() {
        let detector = PatternDetector::new();
        let calls = vec![
            create_call("Grep", false),
            create_call("Read", false),
            create_call("Grep", false),
            create_call("Read", false),
            create_call("Grep", false),
            create_call("Read", false),
        ];

        let pattern = detector.detect(&calls);
        assert!(pattern.is_some());
        assert!(matches!(
            pattern.unwrap(),
            StuckPattern::AlternatingActions { tool_a, tool_b, cycles }
                if (tool_a == "Grep" && tool_b == "Read" && cycles == 3)
                || (tool_a == "Read" && tool_b == "Grep" && cycles == 3)
        ));
    }

    #[test]
    fn test_detect_alternating_not_enough_cycles() {
        let detector = PatternDetector::new();
        let calls = vec![
            create_call("Grep", false),
            create_call("Read", false),
            create_call("Grep", false),
            create_call("Read", false),
        ];

        // Only 2 cycles, threshold is 3
        assert!(detector.detect(&calls).is_none());
    }

    #[test]
    fn test_detect_window_size() {
        let thresholds = PatternThresholds {
            repeating_action: 3,
            repeating_error: 3,
            alternating_cycles: 3,
            window_size: 5,
        };
        let detector = PatternDetector::with_thresholds(thresholds);

        // Old repeating actions outside window
        let input = serde_json::json!({"test": "value"});
        let mut calls = vec![
            create_call_with_input("Bash", input.clone()),
            create_call_with_input("Bash", input.clone()),
            create_call_with_input("Bash", input.clone()),
            create_call_with_input("Bash", input.clone()),
        ];
        // Add different calls to push old ones out of window
        calls.push(create_call("Read", false));
        calls.push(create_call("Grep", false));
        calls.push(create_call("Edit", false));
        calls.push(create_call("Write", false));
        calls.push(create_call("Glob", false));

        // The Bash calls are now outside the window of 5
        assert!(detector.detect(&calls).is_none());
    }

    #[test]
    fn test_stuck_pattern_equality() {
        let p1 = StuckPattern::RepeatingAction {
            tool_name: "Read".to_string(),
            count: 3,
        };
        let p2 = StuckPattern::RepeatingAction {
            tool_name: "Read".to_string(),
            count: 3,
        };
        let p3 = StuckPattern::RepeatingAction {
            tool_name: "Write".to_string(),
            count: 3,
        };

        assert_eq!(p1, p2);
        assert_ne!(p1, p3);
    }

    #[test]
    fn test_stuck_pattern_clone() {
        let original = StuckPattern::AlternatingActions {
            tool_a: "A".to_string(),
            tool_b: "B".to_string(),
            cycles: 5,
        };
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }
}
