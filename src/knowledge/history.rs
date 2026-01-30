//! Session history knowledge source.
//!
//! Extracts Q&A pairs from Claude Code JSONL conversation files
//! to maintain consistency across sessions.

use std::collections::HashMap;

use super::source::{KnowledgeFact, KnowledgeSource};
use crate::watcher::{AssistantEntry, AssistantMessage, ContentBlock, JournalEntry, UserEntry};

/// A question-answer pair extracted from session history.
#[derive(Debug, Clone)]
pub struct QAPair {
    pub question: String,
    pub answer: String,
    pub timestamp: String,
}

/// Knowledge source backed by session history.
pub struct SessionHistorySource {
    pub pairs: Vec<QAPair>,
}

impl SessionHistorySource {
    /// Create from a list of journal entries.
    #[must_use]
    pub fn from_entries(entries: &[JournalEntry]) -> Self {
        let pairs = extract_qa_pairs(entries);
        Self { pairs }
    }

    /// Create an empty source.
    #[must_use]
    pub fn empty() -> Self {
        Self { pairs: Vec::new() }
    }

    /// Maximum number of JSONL files to load.
    const MAX_FILES: usize = 10;
    /// Maximum number of entries per file.
    const MAX_ENTRIES_PER_FILE: usize = 1000;

    /// Load from JSONL files in a project directory.
    pub async fn load(project_dir: &std::path::Path) -> Self {
        // Find the Claude projects directory for this path
        let Some(home) = dirs::home_dir() else {
            tracing::debug!("Could not determine home directory");
            return Self::empty();
        };
        let claude_projects = home.join(".claude").join("projects");

        // Use async check for directory existence
        if !tokio::fs::try_exists(&claude_projects)
            .await
            .unwrap_or(false)
        {
            tracing::debug!("No Claude projects directory found");
            return Self::empty();
        }

        // Hash the project path to find the right directory
        // Claude Code format: /home/user/path -> -home-user-path (keeps leading dash)
        let project_hash = project_dir.to_string_lossy().replace('/', "-");

        let project_sessions = claude_projects.join(&project_hash);

        if !tokio::fs::try_exists(&project_sessions)
            .await
            .unwrap_or(false)
        {
            tracing::debug!("No session history for project: {:?}", project_dir);
            return Self::empty();
        }

        // Load JSONL files with limits to prevent memory exhaustion
        let mut all_entries = Vec::new();
        let mut files_loaded = 0;

        if let Ok(mut entries) = tokio::fs::read_dir(&project_sessions).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if files_loaded >= Self::MAX_FILES {
                    tracing::debug!("Reached max file limit ({})", Self::MAX_FILES);
                    break;
                }

                let path = entry.path();
                if path.extension().is_some_and(|e| e == "jsonl") {
                    match crate::watcher::parse_jsonl_file(&path).await {
                        Ok(entries) => {
                            // Limit entries per file
                            let limited: Vec<_> = entries
                                .into_iter()
                                .take(Self::MAX_ENTRIES_PER_FILE)
                                .collect();
                            all_entries.extend(limited);
                            files_loaded += 1;
                        }
                        Err(e) => tracing::warn!("Failed to parse {:?}: {}", path, e),
                    }
                }
            }
        }

        Self::from_entries(&all_entries)
    }

    /// Find Q&A pairs matching a query.
    fn find_matching_pairs(&self, query: &str) -> Vec<&QAPair> {
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();

        let mut matches: Vec<_> = self
            .pairs
            .iter()
            .filter_map(|pair| {
                let q_lower = pair.question.to_lowercase();

                let score: usize = query_words
                    .iter()
                    .filter(|word| q_lower.contains(*word))
                    .count();

                if score > 0 {
                    Some((score, pair))
                } else {
                    None
                }
            })
            .collect();

        matches.sort_by(|a, b| b.0.cmp(&a.0));
        matches.into_iter().map(|(_, p)| p).collect()
    }
}

impl KnowledgeSource for SessionHistorySource {
    fn source_name(&self) -> &'static str {
        "Session History"
    }

    fn query(&self, question: &str) -> Option<KnowledgeFact> {
        let matches = self.find_matching_pairs(question);

        matches.first().map(|pair| KnowledgeFact {
            source: "Session History".to_string(),
            content: format!("Q: {}\nA: {}", pair.question, pair.answer),
            relevance: 0.7,
        })
    }

    fn context_summary(&self) -> Option<String> {
        if self.pairs.is_empty() {
            return None;
        }

        // Return recent Q&A pairs (up to 10)
        let recent: Vec<_> = self.pairs.iter().rev().take(10).collect();

        let summary = recent
            .iter()
            .map(|pair| format!("Q: {}\nA: {}", pair.question, pair.answer))
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");

        Some(summary)
    }
}

/// Extract Q&A pairs from journal entries.
#[must_use]
pub fn extract_qa_pairs(entries: &[JournalEntry]) -> Vec<QAPair> {
    let mut pairs = Vec::new();

    // Build a map of uuid -> entry for parent lookup
    let mut user_messages: HashMap<String, &UserEntry> = HashMap::new();
    let mut assistant_messages: Vec<&AssistantEntry> = Vec::new();

    for entry in entries {
        match entry {
            JournalEntry::User(u) => {
                // Skip tool results (they have source_tool_use_id)
                if u.source_tool_use_id.is_none() {
                    user_messages.insert(u.uuid.clone(), u);
                }
            }
            JournalEntry::Assistant(a) => {
                assistant_messages.push(a);
            }
            _ => {}
        }
    }

    // Match assistant messages to their parent user messages
    for assistant in assistant_messages {
        if let Some(parent_uuid) = &assistant.parent_uuid {
            if let Some(user) = user_messages.get(parent_uuid) {
                let question = user.message.content.as_text();
                let answer = extract_assistant_text(&assistant.message);

                // Only include if both have meaningful content
                if !question.trim().is_empty() && !answer.trim().is_empty() {
                    pairs.push(QAPair {
                        question,
                        answer,
                        timestamp: assistant.timestamp.clone(),
                    });
                }
            }
        }
    }

    pairs
}

/// Extract text content from an assistant message.
fn extract_assistant_text(message: &AssistantMessage) -> String {
    message
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watcher::{Message, MessageContent};

    fn make_user_entry(uuid: &str, parent: Option<&str>, content: &str) -> JournalEntry {
        JournalEntry::User(UserEntry {
            uuid: uuid.to_string(),
            parent_uuid: parent.map(std::string::ToString::to_string),
            session_id: "s1".to_string(),
            timestamp: "2026-01-29T10:00:00Z".to_string(),
            message: Message {
                role: "user".to_string(),
                content: MessageContent::Text(content.to_string()),
            },
            user_type: "external".to_string(),
            cwd: "/tmp".to_string(),
            git_branch: None,
            version: "2.1.25".to_string(),
            is_sidechain: None,
            source_tool_use_id: None,
            tool_use_result: None,
        })
    }

    fn make_assistant_entry(uuid: &str, parent: &str, content: &str) -> JournalEntry {
        JournalEntry::Assistant(AssistantEntry {
            uuid: uuid.to_string(),
            parent_uuid: Some(parent.to_string()),
            session_id: "s1".to_string(),
            timestamp: "2026-01-29T10:00:01Z".to_string(),
            message: AssistantMessage {
                role: "assistant".to_string(),
                content: vec![ContentBlock::Text {
                    text: content.to_string(),
                }],
                model: Some("claude-3".to_string()),
            },
            cwd: "/tmp".to_string(),
            version: "2.1.25".to_string(),
            git_branch: None,
            is_sidechain: None,
        })
    }

    #[test]
    fn test_extract_qa_pairs() {
        let entries = vec![
            make_user_entry("q1", None, "How do I run tests?"),
            make_assistant_entry("a1", "q1", "Use cargo nextest run"),
        ];

        let pairs = extract_qa_pairs(&entries);

        assert_eq!(pairs.len(), 1);
        assert!(pairs[0].question.contains("run tests"));
        assert!(pairs[0].answer.contains("nextest"));
    }

    #[test]
    fn test_history_source_query() {
        let pairs = vec![
            QAPair {
                question: "What test framework?".to_string(),
                answer: "Use cargo nextest".to_string(),
                timestamp: "2026-01-29T10:00:00Z".to_string(),
            },
            QAPair {
                question: "How to format?".to_string(),
                answer: "Use cargo fmt".to_string(),
                timestamp: "2026-01-29T10:00:01Z".to_string(),
            },
        ];

        let source = SessionHistorySource { pairs };
        let fact = source.query("test framework");

        assert!(fact.is_some());
        assert!(fact.unwrap().content.contains("nextest"));
    }

    #[test]
    fn test_skip_tool_results() {
        let entries = vec![JournalEntry::User(UserEntry {
            uuid: "tr1".to_string(),
            parent_uuid: Some("prev".to_string()),
            session_id: "s1".to_string(),
            timestamp: "2026-01-29T10:00:00Z".to_string(),
            message: Message {
                role: "user".to_string(),
                content: MessageContent::Text(String::new()),
            },
            user_type: "external".to_string(),
            cwd: "/tmp".to_string(),
            git_branch: None,
            version: "2.1.25".to_string(),
            is_sidechain: None,
            source_tool_use_id: Some("tool-123".to_string()),
            tool_use_result: Some(serde_json::json!({"result": "ok"})),
        })];

        let pairs = extract_qa_pairs(&entries);

        assert!(pairs.is_empty());
    }

    #[test]
    fn test_context_summary_limits() {
        let pairs: Vec<QAPair> = (0..20)
            .map(|i| QAPair {
                question: format!("Question {i}"),
                answer: format!("Answer {i}"),
                timestamp: format!("2026-01-29T10:00:{i:02}Z"),
            })
            .collect();

        let source = SessionHistorySource { pairs };
        let summary = source.context_summary().unwrap();

        // Should only include recent 10
        assert!(summary.contains("Question 19"));
        assert!(summary.contains("Question 10"));
        assert!(!summary.contains("Question 0"));
    }
}
