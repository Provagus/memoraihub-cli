//! `meh show` command
//!
//! Shows a fact by path or ID (local or remote).
//!
//! # Usage
//! ```bash
//! meh show @products/alpha/api/timeout
//! meh show meh-01HQ3K2JN5
//! meh show @products/alpha/api/timeout --level summary
//! meh show meh-01HQ3K2JN5 --with-history
//! meh show @products/alpha --server http://localhost:3000 --kb my-kb
//! ```
//!
//! # Detail Levels (from DECISIONS_UNIFIED.md)
//! - L0 Catalog: just path
//! - L1 Index: path + title + trust
//! - L2 Summary: + summary
//! - L3 Full: complete content

use anyhow::{bail, Result};
use clap::{Args, ValueEnum};

use crate::core::fact::Fact;
use crate::core::kb::{KnowledgeBase, KnowledgeBaseBackend};

#[derive(ValueEnum, Clone, Debug, Default)]
pub enum DetailLevel {
    /// L0: Just path
    Catalog,
    /// L1: Path + title + trust
    Index,
    /// L2: Path + title + trust + summary
    #[default]
    Summary,
    /// L3: Full content
    Full,
}

#[derive(Args, Debug)]
pub struct ShowArgs {
    /// Path or ID of the fact
    pub target: String,

    /// Detail level to display
    #[arg(short, long, value_enum, default_value = "full")]
    pub level: DetailLevel,

    /// Show history chain (supersedes/extends)
    #[arg(long)]
    pub with_history: bool,

    /// Output format
    #[arg(short, long, default_value = "pretty")]
    pub format: String,

    /// Use remote server instead of local database
    #[arg(long, env = "MEH_SERVER_URL")]
    pub server: Option<String>,

    /// Knowledge base slug (for remote operations)
    #[arg(long, env = "MEH_KB")]
    pub kb: Option<String>,
}

pub async fn run(args: ShowArgs) -> Result<()> {
    // Load config and create KnowledgeBase (local or remote)
    let config = crate::config::Config::load()?;
    let kb = KnowledgeBase::from_args(args.server.as_deref(), args.kb.as_deref(), &config)?;

    // Get fact by path or ID using unified abstraction
    let fact = kb.get_fact(&args.target).await?;

    let fact = match fact {
        Some(f) => f,
        None => bail!("Fact not found: {}", args.target),
    };

    // Format output based on level
    match args.format.as_str() {
        "json" => print_json(&fact, &args.level)?,
        _ => print_pretty(&fact, &args.level),
    }

    // Note: --with-history only works for local KB
    if args.with_history {
        if matches!(kb, KnowledgeBase::Remote(_)) {
            println!("\n⚠️  History chain not available for remote knowledge bases.");
        } else {
            // For local, we'd need to access storage directly
            // This is a limitation of the current abstraction
            println!(
                "\n⚠️  Use local access for history: meh show {} --with-history",
                args.target
            );
        }
    }

    Ok(())
}

fn print_pretty(fact: &Fact, level: &DetailLevel) {
    match level {
        DetailLevel::Catalog => {
            println!("{}", fact.path);
        }
        DetailLevel::Index => {
            println!("📄 {}", fact.path);
            println!("   Title: {}", fact.title);
            println!("   Trust: {}", format_trust(fact.trust_score));
        }
        DetailLevel::Summary => {
            println!("📄 {}", fact.path);
            println!("   Title: {}", fact.title);
            println!("   Trust: {}", format_trust(fact.trust_score));
            if let Some(summary) = &fact.summary {
                println!("   Summary: {}", summary);
            }
        }
        DetailLevel::Full => {
            println!("\n─────────────────────────────────────────");
            println!("📄 {}", fact.path);
            println!("─────────────────────────────────────────");
            println!("ID:      meh-{}", fact.id);
            println!("Title:   {}", fact.title);
            println!("Trust:   {}", format_trust(fact.trust_score));
            println!("Status:  {:?}", fact.status);
            if !fact.tags.is_empty() {
                println!("Tags:    {}", fact.tags.join(", "));
            }
            println!("Created: {}", fact.created_at);
            println!();
            println!("{}", fact.content);
            println!("─────────────────────────────────────────");
        }
    }
}

fn print_json(fact: &Fact, _level: &DetailLevel) -> Result<()> {
    let json = serde_json::to_string_pretty(fact)?;
    println!("{}", json);
    Ok(())
}

fn format_trust(score: f32) -> String {
    let bars = (score * 10.0) as usize;
    let filled = "█".repeat(bars);
    let empty = "░".repeat(10 - bars);
    format!("{}{} {:.2}", filled, empty, score)
}
