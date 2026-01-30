//! Knowledge source trait and aggregator.

/// A fact retrieved from a knowledge source.
#[derive(Debug, Clone)]
pub struct KnowledgeFact {
    /// Which source provided this fact
    pub source: String,
    /// The fact content
    pub content: String,
    /// Relevance score (0.0 - 1.0)
    pub relevance: f32,
}

/// A source of knowledge for the supervisor.
pub trait KnowledgeSource: Send + Sync {
    /// Unique name for this source.
    fn source_name(&self) -> &'static str;

    /// Query for facts relevant to a question.
    fn query(&self, question: &str) -> Option<KnowledgeFact>;

    /// Get a summary of all knowledge for context building.
    fn context_summary(&self) -> Option<String>;
}

/// Aggregates multiple knowledge sources.
pub struct KnowledgeAggregator {
    sources: Vec<Box<dyn KnowledgeSource>>,
}

impl KnowledgeAggregator {
    /// Create a new empty aggregator.
    #[must_use]
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
        }
    }

    /// Add a knowledge source.
    pub fn add_source(&mut self, source: Box<dyn KnowledgeSource>) {
        self.sources.push(source);
    }

    /// Query all sources for facts relevant to a question.
    #[must_use]
    pub fn query(&self, question: &str) -> Vec<KnowledgeFact> {
        self.sources
            .iter()
            .filter_map(|s| s.query(question))
            .collect()
    }

    /// Build a context string from all sources for the boss prompt.
    #[must_use]
    pub fn build_context(&self) -> String {
        let mut parts = Vec::new();

        for source in &self.sources {
            if let Some(summary) = source.context_summary() {
                parts.push(format!("## {}\n\n{}", source.source_name(), summary));
            }
        }

        parts.join("\n\n---\n\n")
    }

    /// Check if any knowledge is available.
    #[must_use]
    pub fn has_knowledge(&self) -> bool {
        self.sources.iter().any(|s| s.context_summary().is_some())
    }
}

impl Default for KnowledgeAggregator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockSource {
        name: &'static str,
        response: Option<String>,
    }

    impl KnowledgeSource for MockSource {
        fn source_name(&self) -> &'static str {
            self.name
        }

        fn query(&self, _question: &str) -> Option<KnowledgeFact> {
            self.response.as_ref().map(|r| KnowledgeFact {
                source: self.name.to_string(),
                content: r.clone(),
                relevance: 1.0,
            })
        }

        fn context_summary(&self) -> Option<String> {
            self.response.clone()
        }
    }

    #[test]
    fn test_aggregator_queries_all_sources() {
        let mut agg = KnowledgeAggregator::new();
        agg.add_source(Box::new(MockSource {
            name: "source1",
            response: Some("fact1".to_string()),
        }));
        agg.add_source(Box::new(MockSource {
            name: "source2",
            response: Some("fact2".to_string()),
        }));

        let facts = agg.query("test question");

        assert_eq!(facts.len(), 2);
        assert!(facts.iter().any(|f| f.content == "fact1"));
        assert!(facts.iter().any(|f| f.content == "fact2"));
    }

    #[test]
    fn test_aggregator_skips_empty_sources() {
        let mut agg = KnowledgeAggregator::new();
        agg.add_source(Box::new(MockSource {
            name: "empty",
            response: None,
        }));
        agg.add_source(Box::new(MockSource {
            name: "full",
            response: Some("fact".to_string()),
        }));

        let facts = agg.query("test");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].source, "full");
    }

    #[test]
    fn test_aggregator_builds_context() {
        let mut agg = KnowledgeAggregator::new();
        agg.add_source(Box::new(MockSource {
            name: "rules",
            response: Some("Use Rust".to_string()),
        }));
        agg.add_source(Box::new(MockSource {
            name: "history",
            response: Some("Previously decided X".to_string()),
        }));

        let context = agg.build_context();

        assert!(context.contains("Use Rust"));
        assert!(context.contains("Previously decided X"));
    }

    #[test]
    fn test_aggregator_has_knowledge() {
        let mut agg = KnowledgeAggregator::new();
        assert!(!agg.has_knowledge());

        agg.add_source(Box::new(MockSource {
            name: "empty",
            response: None,
        }));
        assert!(!agg.has_knowledge());

        agg.add_source(Box::new(MockSource {
            name: "full",
            response: Some("fact".to_string()),
        }));
        assert!(agg.has_knowledge());
    }
}
