//! `meh search` command
//!
//! Searches facts across sources.
//!
//! # Usage
//! ```bash
//! meh search "api timeout"
//! meh search "timeout" --path "@products/"
//! meh search --tags critical,api "error"
//! meh search "@products/*/api/timeout"   # Wildcard path search
//! ```
//!
//! # Architecture
//! - Uses FTS5 with BM25 ranking
//! - Federated search across sources
//! - Returns L2 Summary level by default
//! - See `../../plan/ANALYSIS_AUTO_CONTEXT_SEARCH.md`

use anyhow::{bail, Result};
use clap::Args;
use std::path::PathBuf;

use super::show::DetailLevel;
use crate::core::fact::Fact;
use crate::core::storage::Storage;

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
}

pub fn run(args: SearchArgs) -> Result<()> {
    // 1. Find .meh directory
    let meh_dir = find_meh_dir()?;
    let db_path = meh_dir.join("data.db");
    let storage = Storage::open(&db_path)?;

    // 2. Execute search
    let facts = storage.search(&args.query, args.limit as i64)?;

    // 3. Apply additional filters
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

    // 4. Output results
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

/// Find .meh directory by walking up from current directory
fn find_meh_dir() -> Result<PathBuf> {
    let mut current = std::env::current_dir()?;

    loop {
        let meh_dir = current.join(".meh");
        if meh_dir.exists() {
            return Ok(meh_dir);
        }

        if !current.pop() {
            bail!(
                "Not a meh repository (or any parent directory). Run 'meh init' first."
            );
        }
    }
}