//! Garbage Collection CLI command

use anyhow::Result;
use clap::Args;

use crate::config::Config;
use crate::core::storage::{GcReason, Storage};

#[derive(Args, Debug)]
pub struct GcArgs {
    /// Retention period in days (default: 30, from config)
    #[arg(long)]
    pub retention_days: Option<u32>,

    /// Show what would be deleted without actually deleting
    #[arg(long)]
    pub dry_run: bool,

    /// Skip confirmation prompt
    #[arg(short = 'y', long)]
    pub yes: bool,
}

pub fn run(args: GcArgs, config: &Config) -> Result<()> {
    let retention_days = args.retention_days.unwrap_or(config.core.gc_retention_days);

    let db_path = config.data_dir();

    if !db_path.exists() {
        println!("No database found. Nothing to clean up.");
        return Ok(());
    }

    let storage = Storage::open(&db_path)?;

    // First do a dry run to show candidates
    let preview = storage.garbage_collect(retention_days, true)?;

    if preview.candidates.is_empty() {
        println!(
            "✨ No deprecated/superseded facts older than {} days.",
            retention_days
        );
        return Ok(());
    }

    println!(
        "🧹 Found {} fact(s) to clean up (older than {} days):\n",
        preview.candidates.len(),
        retention_days
    );

    for candidate in &preview.candidates {
        let reason = match candidate.reason {
            GcReason::Deprecated => "deprecated",
            GcReason::Superseded => "superseded",
        };
        println!("  {} {} ({})", candidate.id, candidate.path, reason);
    }
    println!();

    if args.dry_run {
        println!("ℹ️  Dry run - no facts were deleted.");
        return Ok(());
    }

    // Confirm unless -y flag
    if !args.yes {
        print!("Delete {} fact(s)? [y/N] ", preview.candidates.len());
        use std::io::{self, Write};
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    // Actually delete
    let result = storage.garbage_collect(retention_days, false)?;

    println!("🗑️  Deleted {} fact(s).", result.deleted_count);

    Ok(())
}
