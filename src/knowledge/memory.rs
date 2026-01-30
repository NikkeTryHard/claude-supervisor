//! Memory file for learned Q&A facts.
//!
//! Persists question→answer mappings to avoid re-researching.

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::io::AsyncWriteExt;

use super::source::{KnowledgeFact, KnowledgeSource};

/// A learned fact from research.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryFact {
    /// The question that was asked.
    pub question: String,
    /// The answer that was learned.
    pub answer: String,
    /// ISO 8601 timestamp when this was learned.
    pub learned_at: String,
}

/// Container for memory file JSON format.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryFile {
    /// List of learned facts.
    pub facts: Vec<MemoryFact>,
}

/// Errors from memory operations.
#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("Failed to read memory file: {0}")]
    ReadError(#[from] io::Error),
    #[error("Failed to serialize memory: {0}")]
    SerializeError(#[from] serde_json::Error),
}

/// Knowledge source backed by persistent memory file.
pub struct MemorySource {
    /// Loaded facts.
    facts: Vec<MemoryFact>,
    /// Path to the memory file.
    file_path: PathBuf,
}

impl MemorySource {
    /// Maximum number of facts to include in context summary.
    const MAX_CONTEXT_FACTS: usize = 20;

    /// Calculate the memory file path for a project.
    /// Format: `~/.claude/projects/<encoded-path>/memory.json`
    #[must_use]
    pub fn path_for_project(project_dir: &Path) -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let encoded = project_dir.to_string_lossy().replace('/', "-");
        home.join(".claude")
            .join("projects")
            .join(&encoded)
            .join("memory.json")
    }

    /// Create an empty memory source.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            facts: Vec::new(),
            file_path: PathBuf::new(),
        }
    }

    /// Load memory from the project's memory file.
    pub async fn load(project_dir: &Path) -> Self {
        let file_path = Self::path_for_project(project_dir);

        let facts = match tokio::fs::read_to_string(&file_path).await {
            Ok(content) => match serde_json::from_str::<MemoryFile>(&content) {
                Ok(file) => {
                    tracing::debug!(count = file.facts.len(), "Loaded memory facts");
                    file.facts
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Corrupt memory file, starting fresh");
                    Vec::new()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::debug!("No memory file found");
                Vec::new()
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to read memory file");
                Vec::new()
            }
        };

        Self { facts, file_path }
    }

    /// Add a new fact to memory (deduplicates by normalized question).
    pub fn add_fact(&mut self, question: String, answer: String) {
        let learned_at = chrono::Utc::now().to_rfc3339();
        let normalized_q = question.trim().to_lowercase();

        if let Some(existing) = self
            .facts
            .iter_mut()
            .find(|f| f.question.trim().to_lowercase() == normalized_q)
        {
            existing.answer = answer;
            existing.learned_at = learned_at;
            tracing::debug!(question = %existing.question, "Updated existing memory fact");
        } else {
            self.facts.push(MemoryFact {
                question,
                answer,
                learned_at,
            });
            tracing::debug!(count = self.facts.len(), "Added new memory fact");
        }
    }

    /// Save memory to disk atomically (temp file + sync + rename).
    ///
    /// # Errors
    ///
    /// Returns `MemoryError` if file operations fail.
    pub async fn save(&self) -> Result<(), MemoryError> {
        if self.file_path.as_os_str().is_empty() {
            tracing::warn!("Cannot save: no file path set");
            return Ok(());
        }

        if let Some(parent) = self.file_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let memory_file = MemoryFile {
            facts: self.facts.clone(),
        };
        let json = serde_json::to_string_pretty(&memory_file)?;

        let temp_path = self.file_path.with_extension("json.tmp");
        let mut file = tokio::fs::File::create(&temp_path).await?;
        file.write_all(json.as_bytes()).await?;
        file.sync_data().await?;
        drop(file);

        tokio::fs::rename(&temp_path, &self.file_path).await?;
        tracing::info!(path = %self.file_path.display(), count = self.facts.len(), "Saved memory file");
        Ok(())
    }

    /// Find facts matching the query, sorted by relevance.
    fn find_matching_facts(&self, query: &str) -> Vec<&MemoryFact> {
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();

        let mut matches: Vec<_> = self
            .facts
            .iter()
            .filter_map(|fact| {
                let q_lower = fact.question.to_lowercase();
                let a_lower = fact.answer.to_lowercase();
                let score: usize = query_words
                    .iter()
                    .map(|word| {
                        let mut s = 0;
                        if q_lower.contains(word) {
                            s += 2;
                        }
                        if a_lower.contains(word) {
                            s += 1;
                        }
                        s
                    })
                    .sum();
                if score > 0 {
                    Some((score, fact))
                } else {
                    None
                }
            })
            .collect();

        matches.sort_by(|a, b| b.0.cmp(&a.0));
        matches.into_iter().map(|(_, f)| f).collect()
    }

    /// Get the number of facts in memory.
    #[must_use]
    pub fn len(&self) -> usize {
        self.facts.len()
    }

    /// Check if memory is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.facts.is_empty()
    }
}

impl KnowledgeSource for MemorySource {
    fn source_name(&self) -> &'static str {
        "Memory"
    }

    fn query(&self, question: &str) -> Option<KnowledgeFact> {
        self.find_matching_facts(question)
            .first()
            .map(|fact| KnowledgeFact {
                source: "Memory".to_string(),
                content: format!("Q: {}\nA: {}", fact.question, fact.answer),
                relevance: 0.9,
            })
    }

    fn context_summary(&self) -> Option<String> {
        if self.facts.is_empty() {
            return None;
        }
        let recent: Vec<_> = self
            .facts
            .iter()
            .rev()
            .take(Self::MAX_CONTEXT_FACTS)
            .collect();
        let summary = recent
            .iter()
            .map(|f| format!("Q: {}\nA: {}", f.question, f.answer))
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");
        Some(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_fact_serialization() {
        let fact = MemoryFact {
            question: "What is Rust?".to_string(),
            answer: "A systems programming language".to_string(),
            learned_at: "2024-01-01T00:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&fact).unwrap();
        let parsed: MemoryFact = serde_json::from_str(&json).unwrap();
        assert_eq!(fact, parsed);
    }

    #[test]
    fn test_memory_file_serialization() {
        let file = MemoryFile {
            facts: vec![MemoryFact {
                question: "Q1".to_string(),
                answer: "A1".to_string(),
                learned_at: "2024-01-01T00:00:00Z".to_string(),
            }],
        };

        let json = serde_json::to_string(&file).unwrap();
        let parsed: MemoryFile = serde_json::from_str(&json).unwrap();
        assert_eq!(file.facts.len(), parsed.facts.len());
    }

    #[test]
    fn test_memory_source_empty() {
        let source = MemorySource::empty();
        assert!(source.is_empty());
        assert_eq!(source.len(), 0);
    }

    #[test]
    fn test_memory_source_add_fact() {
        let mut source = MemorySource::empty();
        source.add_fact("What is Rust?".to_string(), "A language".to_string());

        assert_eq!(source.len(), 1);
        assert!(!source.is_empty());
    }

    #[test]
    fn test_memory_source_add_fact_deduplicates() {
        let mut source = MemorySource::empty();
        source.add_fact("What is Rust?".to_string(), "A language".to_string());
        source.add_fact("what is rust?".to_string(), "Updated answer".to_string());

        assert_eq!(source.len(), 1);
        // The answer should be updated
        let summary = source.context_summary().unwrap();
        assert!(summary.contains("Updated answer"));
    }

    #[test]
    fn test_memory_source_query() {
        let mut source = MemorySource::empty();
        source.add_fact(
            "What is Rust?".to_string(),
            "A systems language".to_string(),
        );
        source.add_fact("How to compile?".to_string(), "Use cargo build".to_string());

        let result = source.query("Rust");
        assert!(result.is_some());
        let fact = result.unwrap();
        assert_eq!(fact.source, "Memory");
        assert!(fact.content.contains("Rust"));
    }

    #[test]
    fn test_memory_source_query_no_match() {
        let mut source = MemorySource::empty();
        source.add_fact("What is Rust?".to_string(), "A language".to_string());

        let result = source.query("Python");
        assert!(result.is_none());
    }

    #[test]
    fn test_memory_source_context_summary_empty() {
        let source = MemorySource::empty();
        assert!(source.context_summary().is_none());
    }

    #[test]
    fn test_memory_source_context_summary() {
        let mut source = MemorySource::empty();
        source.add_fact("Q1".to_string(), "A1".to_string());
        source.add_fact("Q2".to_string(), "A2".to_string());

        let summary = source.context_summary().unwrap();
        assert!(summary.contains("Q1"));
        assert!(summary.contains("A1"));
        assert!(summary.contains("Q2"));
        assert!(summary.contains("A2"));
    }

    #[test]
    fn test_memory_source_source_name() {
        let source = MemorySource::empty();
        assert_eq!(source.source_name(), "Memory");
    }

    #[test]
    fn test_path_for_project() {
        let path = MemorySource::path_for_project(Path::new("/home/user/project"));
        assert!(path.to_string_lossy().contains("memory.json"));
        assert!(path.to_string_lossy().contains(".claude"));
    }

    #[tokio::test]
    async fn test_memory_source_load_nonexistent() {
        let temp_dir = std::env::temp_dir().join("nonexistent_memory_test_12345");
        let source = MemorySource::load(&temp_dir).await;
        assert!(source.is_empty());
    }

    #[tokio::test]
    async fn test_memory_source_save_and_load() {
        let temp_dir = tempfile::tempdir().unwrap();
        let project_dir = temp_dir.path();

        // Create memory file directory structure
        let memory_path = MemorySource::path_for_project(project_dir);
        if let Some(parent) = memory_path.parent() {
            tokio::fs::create_dir_all(parent).await.unwrap();
        }

        // Create source with facts
        let mut source = MemorySource {
            facts: Vec::new(),
            file_path: memory_path.clone(),
        };
        source.add_fact("Test Q".to_string(), "Test A".to_string());

        // Save
        source.save().await.unwrap();

        // Verify file exists
        assert!(memory_path.exists());

        // Load and verify
        let loaded = MemorySource::load(project_dir).await;
        assert_eq!(loaded.len(), 1);
        let summary = loaded.context_summary().unwrap();
        assert!(summary.contains("Test Q"));
        assert!(summary.contains("Test A"));
    }

    #[tokio::test]
    async fn test_memory_source_save_empty_path() {
        let source = MemorySource::empty();
        // Should not error, just warn
        let result = source.save().await;
        assert!(result.is_ok());
    }
}
