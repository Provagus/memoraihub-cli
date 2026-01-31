//! `meh search` command
//!
//! Searches facts across sources (local or remote).
//!
//! # Usage
//! ```bash
//! meh search "api timeout"
//! meh search "timeout" --path "@products/"
//! meh search --tags critical,api "error"
//! meh search "@products/*/api/timeout"   # Wildcard path search
//! meh search "query" --server http://localhost:3000 --kb my-kb  # Remote
//! ```
//!
//! # Architecture
//! - Uses FTS5 with BM25 ranking (local)
//! - Or HTTP API (remote)
//! - Uses KnowledgeBase abstraction for unified access
//! - Returns L2 Summary level by default
//! - See `../../plan/ANALYSIS_AUTO_CONTEXT_SEARCH.md`

use anyhow::Result;
use clap::Args;

use super::show::DetailLevel;
use crate::core::fact::Fact;
use crate::core::kb::{KnowledgeBase, KnowledgeBaseBackend};

#[derive(Args, Debug)]
pub struct SearchArgs {
    /// Search query (text or path pattern with wildcards)
    pub query: String,

    /// Filter by path prefix
    #[arg(short, long)]
    pub path: Option<String>,

    /// Filter by tags (comma-separated, AND logic)
    #[arg(short, long, value_delimiter = ',')]
    pub tags: Option<Vec<String>>,

    /// Exclude tags (comma-separated)
    #[arg(long, value_delimiter = ',')]
    pub not_tags: Option<Vec<String>>,

    /// Sources to search (comma-separated, default: all)
    #[arg(short, long, value_delimiter = ',')]
    pub source: Option<Vec<String>>,

    /// Minimum trust score (0.0-1.0)
    #[arg(long)]
    pub min_trust: Option<f32>,

    /// Only show active (non-deprecated) facts
    #[arg(long)]
    pub active_only: bool,

    /// Maximum results
    #[arg(short, long, default_value = "20")]
    pub limit: usize,

    /// Detail level for results
    #[arg(long, value_enum, default_value = "summary")]
    pub level: DetailLevel,

    /// Token budget (for AI use)
    #[arg(long)]
    pub token_budget: Option<usize>,

    /// Output format (pretty, json, compact)
    #[arg(short, long, default_value = "pretty")]
    pub format: String,
    
    /// Use remote server instead of local database
    #[arg(long, env = "MEH_SERVER_URL")]
    pub server: Option<String>,
    
    /// Knowledge base slug (for remote operations)
    #[arg(long, env = "MEH_KB")]
    pub kb: Option<String>,
}

pub async fn run(args: SearchArgs) -> Result<()> {
    // Load config and create KnowledgeBase (local or remote)
    let config = crate::config::Config::load()?;
    let kb = KnowledgeBase::from_args(
        args.server.as_deref(),
        args.kb.as_deref(),
        &config,
    )?;

    // Execute search using unified abstraction
    let facts = kb.search(&args.query, args.limit).await?;

    // Apply additional filters (not supported by all backends yet)
    let facts: Vec<_> = facts
        .into_iter()
        .filter(|f| {
            // Path prefix filter
            if let Some(prefix) = &args.path {
                if !f.path.starts_with(prefix.trim_end_matches('/')) {
                    return false;
                }
            }
            // Tags filter
            if let Some(required_tags) = &args.tags {
                for tag in required_tags {
                    if !f.tags.contains(tag) {
                        return false;
                    }
                }
            }
            // Min trust filter
            if let Some(min) = args.min_trust {
                if f.trust_score < min {
                    return false;
                }
            }
            true
        })
        .collect();

    // Output results
    match args.format.as_str() {
        "json" => print_json(&facts)?,
        "compact" => print_compact(&facts),
        _ => print_pretty(&facts, &args.level),
    }

    Ok(())
}

fn print_pretty(facts: &[Fact], level: &DetailLevel) {
    if facts.is_empty() {
        println!("No results found.");
        return;
    }

    println!("\n📚 Found {} result(s):\n", facts.len());

    for (i, fact) in facts.iter().enumerate() {
        match level {
            DetailLevel::Catalog => {
                println!("{}. {}", i + 1, fact.path);
            }
            DetailLevel::Index => {
                println!("{}. {}", i + 1, fact.path);
                println!("   Trust: {:.2} | {}", fact.trust_score, fact.title);
            }
            DetailLevel::Summary | DetailLevel::Full => {
                println!("{}. {}", i + 1, fact.path);
                println!(
                    "   Trust: {:.2} | Tags: {}",
                    fact.trust_score,
                    if fact.tags.is_empty() {
                        "-".to_string()
                    } else {
                        fact.tags.join(", ")
                    }
                );
                if let Some(summary) = &fact.summary {
                    println!("   {}\n", summary);
                } else {
                    // Use first 100 chars as summary
                    let preview: String = fact.content.chars().take(100).collect();
                    println!("   {}\n", preview);
                }
            }
        }
    }
}

fn print_compact(facts: &[Fact]) {
    for fact in facts {
        println!("{}\t{}\t{:.2}", fact.path, fact.title, fact.trust_score);
    }
}

fn print_json(facts: &[Fact]) -> Result<()> {
    let json = serde_json::to_string_pretty(facts)?;
    println!("{}", json);
    Ok(())
}