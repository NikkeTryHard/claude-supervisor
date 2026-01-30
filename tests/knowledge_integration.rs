//! Integration tests for the knowledge layer.

use claude_supervisor::knowledge::{ClaudeMdSource, KnowledgeAggregator};

#[test]
fn test_aggregator_with_claude_md() {
    let markdown = "## Commands

Run tests with: cargo nextest run

## Conventions

- Use snake_case for variables
- Prefer Result over unwrap
";

    let source = ClaudeMdSource::from_content(markdown);
    let mut agg = KnowledgeAggregator::new();
    agg.add_source(Box::new(source));

    // Should find test command
    let facts = agg.query("how to run tests");
    assert!(!facts.is_empty());
    assert!(facts[0].content.contains("nextest"));

    // Should build context
    let context = agg.build_context();
    assert!(context.contains("Commands"));
    assert!(context.contains("Conventions"));
}
