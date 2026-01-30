//! Boss AI for answering questions from accumulated knowledge.

use serde::{Deserialize, Serialize};

/// Decision from the Boss AI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "decision", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BossDecision {
    /// Boss can answer from available knowledge.
    Answer {
        answer: String,
        confidence: f64,
        save_as_fact: bool,
    },
    /// Boss needs more information via research.
    ResearchNeeded {
        reason: String,
        queries: Vec<String>,
    },
}

/// System prompt for the Boss AI.
pub const BOSS_SYSTEM_PROMPT: &str = r#"You are a project manager AI. You do NOT write or review code.

Your role is to answer questions from workers using available knowledge sources.

## Available Knowledge

{context}

## Worker Question

{question}

## Decision Framework

Answer directly when:
- The answer is clearly available in the knowledge sources above
- You have high confidence in the answer

Request research when:
- The knowledge sources don't contain relevant information
- You need to examine specific files or code

## Response Format

For direct answers:
{"decision": "ANSWER", "answer": "Your answer", "confidence": 0.95, "save_as_fact": true}

For research requests:
{"decision": "RESEARCH_NEEDED", "reason": "Why", "queries": ["search query"]}

Always respond with ONLY the JSON object."#;

/// Format the boss prompt with context and question.
#[must_use]
pub fn format_boss_prompt(context: &str, question: &str) -> String {
    BOSS_SYSTEM_PROMPT
        .replace("{context}", context)
        .replace("{question}", question)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boss_decision_answer_serialization() {
        let decision = BossDecision::Answer {
            answer: "Use cargo build".to_string(),
            confidence: 0.95,
            save_as_fact: true,
        };

        let json = serde_json::to_string(&decision).unwrap();
        assert!(json.contains("ANSWER"));
        assert!(json.contains("cargo build"));

        let parsed: BossDecision = serde_json::from_str(&json).unwrap();
        assert_eq!(decision, parsed);
    }

    #[test]
    fn test_boss_decision_research_needed_serialization() {
        let decision = BossDecision::ResearchNeeded {
            reason: "Need to check the config file".to_string(),
            queries: vec!["config.toml".to_string(), "settings".to_string()],
        };

        let json = serde_json::to_string(&decision).unwrap();
        assert!(json.contains("RESEARCH_NEEDED"));
        assert!(json.contains("config file"));

        let parsed: BossDecision = serde_json::from_str(&json).unwrap();
        assert_eq!(decision, parsed);
    }

    #[test]
    fn test_boss_decision_parse_answer_json() {
        let json = r#"{"decision": "ANSWER", "answer": "Test answer", "confidence": 0.9, "save_as_fact": false}"#;
        let decision: BossDecision = serde_json::from_str(json).unwrap();

        match decision {
            BossDecision::Answer {
                answer,
                confidence,
                save_as_fact,
            } => {
                assert_eq!(answer, "Test answer");
                assert!((confidence - 0.9).abs() < f64::EPSILON);
                assert!(!save_as_fact);
            }
            BossDecision::ResearchNeeded { .. } => panic!("Expected Answer variant"),
        }
    }

    #[test]
    fn test_boss_decision_parse_research_json() {
        let json = r#"{"decision": "RESEARCH_NEEDED", "reason": "Unknown", "queries": ["query1", "query2"]}"#;
        let decision: BossDecision = serde_json::from_str(json).unwrap();

        match decision {
            BossDecision::ResearchNeeded { reason, queries } => {
                assert_eq!(reason, "Unknown");
                assert_eq!(queries.len(), 2);
                assert_eq!(queries[0], "query1");
            }
            BossDecision::Answer { .. } => panic!("Expected ResearchNeeded variant"),
        }
    }

    #[test]
    fn test_format_boss_prompt() {
        let context = "Some project context";
        let question = "How do I build?";

        let prompt = format_boss_prompt(context, question);

        assert!(prompt.contains("Some project context"));
        assert!(prompt.contains("How do I build?"));
        assert!(prompt.contains("project manager AI"));
        assert!(!prompt.contains("{context}"));
        assert!(!prompt.contains("{question}"));
    }

    #[test]
    fn test_boss_system_prompt_contains_expected_sections() {
        assert!(BOSS_SYSTEM_PROMPT.contains("Available Knowledge"));
        assert!(BOSS_SYSTEM_PROMPT.contains("Worker Question"));
        assert!(BOSS_SYSTEM_PROMPT.contains("Decision Framework"));
        assert!(BOSS_SYSTEM_PROMPT.contains("Response Format"));
    }
}
