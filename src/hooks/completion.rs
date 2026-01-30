//! Completion detection for stop hook handling.

/// Detects whether Claude's response indicates task completion.
#[derive(Debug, Clone)]
pub struct CompletionDetector {
    complete_phrases: Vec<String>,
    incomplete_phrases: Vec<String>,
}

impl CompletionDetector {
    /// Create a new completion detector with custom phrases.
    #[must_use]
    pub fn new(complete_phrases: Vec<String>, incomplete_phrases: Vec<String>) -> Self {
        Self {
            complete_phrases,
            incomplete_phrases,
        }
    }

    /// Check if the text indicates task completion.
    /// Incomplete phrases take priority over complete phrases.
    #[must_use]
    pub fn is_complete(&self, text: &str) -> bool {
        let text_lower = text.to_lowercase();

        // Incomplete phrases take priority
        for phrase in &self.incomplete_phrases {
            if text_lower.contains(&phrase.to_lowercase()) {
                return false;
            }
        }

        // Check for completion phrases
        for phrase in &self.complete_phrases {
            if text_lower.contains(&phrase.to_lowercase()) {
                return true;
            }
        }

        // Default: not complete (be conservative)
        false
    }
}

impl Default for CompletionDetector {
    fn default() -> Self {
        Self {
            complete_phrases: vec![
                "task is complete".to_string(),
                "successfully completed".to_string(),
                "all done".to_string(),
                "finished successfully".to_string(),
                "completed all tasks".to_string(),
                "implementation is complete".to_string(),
                "changes have been made".to_string(),
            ],
            incomplete_phrases: vec![
                "now i'll".to_string(),
                "next step".to_string(),
                "let me also".to_string(),
                "i'll now".to_string(),
                "next, i".to_string(),
                "moving on to".to_string(),
                "i need to".to_string(),
                "let me continue".to_string(),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_completion_detector_default() {
        let detector = CompletionDetector::default();
        assert!(!detector.complete_phrases.is_empty());
        assert!(!detector.incomplete_phrases.is_empty());
    }

    #[test]
    fn test_is_complete_with_complete_phrase() {
        let detector = CompletionDetector::default();
        assert!(detector.is_complete("The task is complete and all tests pass."));
        assert!(detector.is_complete("I have successfully completed the implementation."));
        assert!(detector.is_complete("All done! The feature is working."));
    }

    #[test]
    fn test_is_complete_with_incomplete_phrase() {
        let detector = CompletionDetector::default();
        assert!(!detector.is_complete("Now I'll implement the next feature."));
        assert!(!detector.is_complete("Let me also add some tests."));
        assert!(!detector.is_complete("Moving on to the next step."));
    }

    #[test]
    fn test_incomplete_takes_priority() {
        let detector = CompletionDetector::default();
        // Contains both complete and incomplete phrases
        let text = "The task is complete, but now I'll add more tests.";
        assert!(!detector.is_complete(text));
    }

    #[test]
    fn test_is_complete_case_insensitive() {
        let detector = CompletionDetector::default();
        assert!(detector.is_complete("TASK IS COMPLETE"));
        assert!(detector.is_complete("Successfully Completed"));
    }

    #[test]
    fn test_is_complete_no_match() {
        let detector = CompletionDetector::default();
        assert!(!detector.is_complete("Here is some random text."));
    }

    #[test]
    fn test_custom_phrases() {
        let detector =
            CompletionDetector::new(vec!["finished".to_string()], vec!["pending".to_string()]);
        assert!(detector.is_complete("The work is finished."));
        assert!(!detector.is_complete("Some tasks are pending."));
    }
}
