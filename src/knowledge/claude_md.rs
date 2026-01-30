//! CLAUDE.md file parser and knowledge source.
//!
//! Loads project conventions from CLAUDE.md files using comrak for
//! AST-based section extraction.

use std::collections::HashMap;
use std::path::Path;

use comrak::nodes::{AstNode, NodeValue};
use comrak::{parse_document, Arena, Options};

use super::source::{KnowledgeFact, KnowledgeSource};

/// Safely truncate a string at a character boundary.
///
/// Unlike byte slicing, this will not panic on multi-byte UTF-8 characters.
fn safe_truncate(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

/// Knowledge source backed by CLAUDE.md file(s).
pub struct ClaudeMdSource {
    /// Parsed sections: header -> content
    pub sections: HashMap<String, String>,
    /// Raw content for fallback
    raw_content: String,
}

impl ClaudeMdSource {
    /// Load CLAUDE.md from a project directory.
    ///
    /// Searches for CLAUDE.md in the directory and loads it.
    /// Returns an empty source if not found.
    pub async fn load(project_dir: &Path) -> Self {
        let claude_md_path = project_dir.join("CLAUDE.md");

        // Try to read directly - handles non-existence via error
        match tokio::fs::read_to_string(&claude_md_path).await {
            Ok(content) => Self::from_content(&content),
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    tracing::debug!("No CLAUDE.md found in {:?}", project_dir);
                } else {
                    tracing::warn!("Failed to read CLAUDE.md: {}", e);
                }
                Self::empty()
            }
        }
    }

    /// Load CLAUDE.md from both project directory and global ~/.claude/CLAUDE.md.
    ///
    /// Merges both sources, with project-level sections taking precedence
    /// over global sections with the same name.
    pub async fn load_with_global(project_dir: &Path) -> Self {
        let mut combined_sections = HashMap::new();
        let mut combined_content = String::new();

        // Load global CLAUDE.md first (lower priority)
        if let Some(home) = dirs::home_dir() {
            let global_path = home.join(".claude").join("CLAUDE.md");
            if let Ok(content) = tokio::fs::read_to_string(&global_path).await {
                let global = Self::from_content(&content);
                combined_sections.extend(global.sections);
                combined_content.push_str(&content);
                combined_content.push_str("\n\n---\n\n");
                tracing::debug!("Loaded global CLAUDE.md from {:?}", global_path);
            }
        }

        // Load project CLAUDE.md (higher priority, overwrites global sections)
        let project_path = project_dir.join("CLAUDE.md");
        if let Ok(content) = tokio::fs::read_to_string(&project_path).await {
            let project = Self::from_content(&content);
            combined_sections.extend(project.sections);
            combined_content.push_str(&content);
            tracing::debug!("Loaded project CLAUDE.md from {:?}", project_path);
        }

        if combined_sections.is_empty() {
            tracing::debug!("No CLAUDE.md files found");
            Self::empty()
        } else {
            Self {
                sections: combined_sections,
                raw_content: combined_content,
            }
        }
    }

    /// Create from raw markdown content.
    #[must_use]
    pub fn from_content(content: &str) -> Self {
        let sections = Self::parse_sections(content);
        Self {
            sections,
            raw_content: content.to_string(),
        }
    }

    /// Create an empty source.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            sections: HashMap::new(),
            raw_content: String::new(),
        }
    }

    /// Parse markdown into sections using comrak AST.
    fn parse_sections(content: &str) -> HashMap<String, String> {
        let arena = Arena::new();
        let options = Options::default();
        let root = parse_document(&arena, content, &options);

        let mut sections = HashMap::new();
        let mut current_header: Option<String> = None;
        let mut current_content = String::new();

        for node in root.children() {
            match &node.data.borrow().value {
                NodeValue::Heading(_) => {
                    // Save previous section
                    if let Some(header) = current_header.take() {
                        let trimmed = current_content.trim().to_string();
                        if !trimmed.is_empty() {
                            sections.insert(header, trimmed);
                        }
                    }
                    current_content.clear();

                    // Extract new header text
                    current_header = Some(Self::extract_text(node));
                }
                _ => {
                    // Accumulate content under current header
                    if current_header.is_some() {
                        let text = Self::node_to_text(node);
                        if !text.is_empty() {
                            if !current_content.is_empty() {
                                current_content.push_str("\n\n");
                            }
                            current_content.push_str(&text);
                        }
                    }
                }
            }
        }

        // Save final section
        if let Some(header) = current_header {
            let trimmed = current_content.trim().to_string();
            if !trimmed.is_empty() {
                sections.insert(header, trimmed);
            }
        }

        sections
    }

    /// Extract text content from a heading node.
    fn extract_text<'a>(node: &'a AstNode<'a>) -> String {
        let mut text = String::new();
        Self::collect_text(node, &mut text);
        text.trim().to_string()
    }

    /// Recursively collect text from nodes.
    fn collect_text<'a>(node: &'a AstNode<'a>, out: &mut String) {
        match &node.data.borrow().value {
            NodeValue::Text(t) => out.push_str(t),
            NodeValue::Code(c) => out.push_str(&c.literal),
            NodeValue::CodeBlock(cb) => {
                out.push_str(&cb.literal);
            }
            NodeValue::SoftBreak | NodeValue::LineBreak => out.push(' '),
            NodeValue::Item(_) => {
                out.push_str("- ");
                for child in node.children() {
                    Self::collect_text(child, out);
                }
                out.push('\n');
            }
            _ => {
                for child in node.children() {
                    Self::collect_text(child, out);
                }
            }
        }
    }

    /// Convert a node back to approximate markdown text.
    fn node_to_text<'a>(node: &'a AstNode<'a>) -> String {
        let mut text = String::new();
        Self::collect_text(node, &mut text);
        text
    }

    /// Find sections relevant to a query using simple keyword matching.
    #[must_use]
    pub fn find_relevant_sections(&self, query: &str) -> Vec<(&str, &str)> {
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();

        let mut matches: Vec<_> = self
            .sections
            .iter()
            .filter_map(|(header, content)| {
                let header_lower = header.to_lowercase();
                let content_lower = content.to_lowercase();

                // Score based on keyword matches
                let score: usize = query_words
                    .iter()
                    .map(|word| {
                        let mut s = 0;
                        if header_lower.contains(word) {
                            s += 2; // Header match worth more
                        }
                        if content_lower.contains(word) {
                            s += 1;
                        }
                        s
                    })
                    .sum();

                if score > 0 {
                    Some((score, header.as_str(), content.as_str()))
                } else {
                    None
                }
            })
            .collect();

        // Sort by score descending
        matches.sort_by(|a, b| b.0.cmp(&a.0));

        matches.into_iter().map(|(_, h, c)| (h, c)).collect()
    }

    /// Get the raw content.
    #[must_use]
    pub fn raw_content(&self) -> &str {
        &self.raw_content
    }
}

impl KnowledgeSource for ClaudeMdSource {
    fn source_name(&self) -> &'static str {
        "CLAUDE.md"
    }

    fn query(&self, question: &str) -> Option<KnowledgeFact> {
        let relevant = self.find_relevant_sections(question);

        if let Some((header, content)) = relevant.first() {
            Some(KnowledgeFact {
                source: format!("CLAUDE.md: {header}"),
                content: (*content).to_string(),
                relevance: 0.8,
            })
        } else {
            None
        }
    }

    fn context_summary(&self) -> Option<String> {
        if self.sections.is_empty() {
            return None;
        }

        let summary: String = self
            .sections
            .iter()
            .map(|(header, content)| {
                // Truncate long sections (using safe char-boundary truncation)
                let truncated = if content.chars().count() > 500 {
                    format!("{}...", safe_truncate(content, 500))
                } else {
                    content.clone()
                };
                format!("### {header}\n\n{truncated}")
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        Some(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sections() {
        let markdown = "# Project

## Commands

```bash
cargo test
```

## Conventions

- Use snake_case
- Prefer Result over panic

## Resources

Some links here.
";

        let source = ClaudeMdSource::from_content(markdown);

        assert!(source.sections.contains_key("Commands"));
        assert!(source.sections.contains_key("Conventions"));
        assert!(source.sections["Conventions"].contains("snake_case"));
    }

    #[test]
    fn test_empty_file() {
        let source = ClaudeMdSource::from_content("");
        assert!(source.sections.is_empty());
    }

    #[test]
    fn test_no_headers() {
        let markdown = "Just some plain text without headers.";
        let source = ClaudeMdSource::from_content(markdown);

        // Should be empty since no headers to section by
        assert!(source.sections.is_empty());
    }

    #[test]
    fn test_query_finds_relevant_section() {
        let markdown = "## Error Handling

Use thiserror for error types.

## Testing

Use cargo nextest.
";

        let source = ClaudeMdSource::from_content(markdown);
        let fact = source.query("error handling");

        assert!(
            fact.is_some(),
            "Sections: {:?}",
            source.sections.keys().collect::<Vec<_>>()
        );
        let fact = fact.unwrap();
        assert!(fact.content.contains("thiserror"));
    }

    #[test]
    fn test_context_summary() {
        let markdown = "## Commands

cargo test

## Style

Use rustfmt
";

        let source = ClaudeMdSource::from_content(markdown);
        let summary = source.context_summary();

        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert!(summary.contains("Commands"));
        assert!(summary.contains("cargo test"));
    }

    #[test]
    fn test_find_relevant_sections_scoring() {
        let markdown = "## Error Handling

Use thiserror.

## Testing Errors

Test error cases.

## Logging

Use tracing.
";

        let source = ClaudeMdSource::from_content(markdown);

        // "error" appears in header of first two sections
        let relevant = source.find_relevant_sections("error");

        assert!(!relevant.is_empty());
        // Both error-related sections should be found
        let headers: Vec<_> = relevant.iter().map(|(h, _)| *h).collect();
        assert!(headers.contains(&"Error Handling") || headers.contains(&"Testing Errors"));
    }
}
