//! Search - Full-text search engine
//!
//! Uses SQLite FTS5 with BM25 ranking.
//!
//! # Architecture
//! See `../../plan/ANALYSIS_AUTO_CONTEXT_SEARCH.md`

use anyhow::Result;

use super::fact::Fact;
use super::storage::Storage;

/// Search query builder
#[derive(Debug, Default)]
pub struct SearchQuery {
    /// Full-text query
    pub text: Option<String>,

    /// Path prefix filter
    pub path_prefix: Option<String>,

    /// Required tags (AND logic)
    pub tags: Vec<String>,

    /// Excluded tags
    pub not_tags: Vec<String>,

    /// Minimum trust score
    pub min_trust: Option<f32>,

    /// Include deprecated facts
    pub include_deprecated: bool,

    /// Maximum results
    pub limit: usize,

    /// Token budget (for AI context)
    pub token_budget: Option<usize>,
}

impl SearchQuery {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            limit: 20,
            ..Default::default()
        }
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path_prefix = Some(path.into());
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    pub fn with_min_trust(mut self, min_trust: f32) -> Self {
        self.min_trust = Some(min_trust);
        self
    }

    pub fn include_deprecated(mut self) -> Self {
        self.include_deprecated = true;
        self
    }
}

/// Search result with relevance score
#[derive(Debug)]
pub struct SearchResult {
    pub fact: Fact,
    pub relevance: f32,
    pub token_count: usize,
}

/// Execute a search query
pub fn search(storage: &Storage, query: &SearchQuery) -> Result<Vec<SearchResult>> {
    // TODO: Implement full search with all filters
    // For now, just use basic FTS search

    let text = query.text.as_deref().unwrap_or("*");
    let facts = storage.search(text, query.limit as i64)?;

    let results: Vec<SearchResult> = facts
        .into_iter()
        .filter(|f| {
            // Apply path filter
            if let Some(prefix) = &query.path_prefix {
                if !f.path.starts_with(prefix.trim_end_matches('/')) {
                    return false;
                }
            }

            // Apply trust filter
            if let Some(min) = query.min_trust {
                if f.trust_score < min {
                    return false;
                }
            }

            // Apply tag filter
            if !query.tags.is_empty() {
                if !query.tags.iter().all(|t| f.tags.contains(t)) {
                    return false;
                }
            }

            true
        })
        .map(|f| {
            let token_count = estimate_tokens(&f);
            SearchResult {
                fact: f,
                relevance: 1.0, // TODO: Get actual BM25 score
                token_count,
            }
        })
        .collect();

    Ok(results)
}

/// Estimate token count for a fact
fn estimate_tokens(fact: &Fact) -> usize {
    // Rough estimation: ~4 chars per token
    let content_tokens = fact.content.len() / 4;
    let title_tokens = fact.title.len() / 4;
    let path_tokens = fact.path.len() / 4;
    let overhead = 20; // Metadata overhead

    content_tokens + title_tokens + path_tokens + overhead
}

/// Truncate results to fit token budget
pub fn truncate_to_budget(results: Vec<SearchResult>, budget: usize) -> Vec<SearchResult> {
    let mut total_tokens = 0;
    let mut truncated = Vec::new();

    for result in results {
        if total_tokens + result.token_count > budget {
            break;
        }
        total_tokens += result.token_count;
        truncated.push(result);
    }

    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens() {
        let mut fact = Fact::new("@test", "Title", "This is content.");
        fact.generate_summary(100);

        let tokens = estimate_tokens(&fact);
        assert!(tokens > 0);
    }

    #[test]
    fn test_truncate_to_budget() {
        let results = vec![
            SearchResult {
                fact: Fact::new("@a", "A", "Content A"),
                relevance: 1.0,
                token_count: 100,
            },
            SearchResult {
                fact: Fact::new("@b", "B", "Content B"),
                relevance: 0.9,
                token_count: 100,
            },
            SearchResult {
                fact: Fact::new("@c", "C", "Content C"),
                relevance: 0.8,
                token_count: 100,
            },
        ];

        let truncated = truncate_to_budget(results, 250);
        assert_eq!(truncated.len(), 2);
    }
}
