//! `meh show` command
//!
//! Shows a fact by path or ID.
//!
//! # Usage
//! ```bash
//! meh show @products/alpha/api/timeout
//! meh show meh-01HQ3K2JN5
//! meh show @products/alpha/api/timeout --level summary
//! meh show meh-01HQ3K2JN5 --with-history
//! ```
//!
//! # Detail Levels (from DECISIONS_UNIFIED.md)
//! - L0 Catalog: just path
//! - L1 Index: path + title + trust
//! - L2 Summary: + summary
//! - L3 Full: complete content

use anyhow::{bail, Result};
use clap::{Args, ValueEnum};
use std::path::PathBuf;
use ulid::Ulid;

use crate::core::fact::Fact;
use crate::core::storage::Storage;

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
}

pub fn run(args: ShowArgs) -> Result<()> {
    // 1. Find .meh directory
    let meh_dir = find_meh_dir()?;
    let db_path = meh_dir.join("data.db");
    let storage = Storage::open(&db_path)?;

    // 2. Determine if target is path or ID
    let fact = if args.target.starts_with("meh-") {
        // It's an ID
        let id_str = args.target.trim_start_matches("meh-");
        let id = Ulid::from_string(id_str)
            .map_err(|_| anyhow::anyhow!("Invalid meh ID: {}", args.target))?;
        storage.get_by_id(&id)?
    } else {
        // It's a path - get the latest active fact with that path
        let facts = storage.get_by_path(&args.target)?;
        facts.into_iter().next()
    };

    let fact = match fact {
        Some(f) => f,
        None => bail!("Fact not found: {}", args.target),
    };

    // 3. Format output based on level
    match args.format.as_str() {
        "json" => print_json(&fact, &args.level)?,
        _ => print_pretty(&fact, &args.level),
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
