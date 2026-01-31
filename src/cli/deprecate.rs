//! `meh deprecate` command
//!
//! Marks a fact as deprecated. Does not delete - just flags.
//!
//! # Usage
//! ```bash
//! meh deprecate meh-01HQ3K2JN5 --reason "Replaced by dynamic timeout in v3.0"
//! meh deprecate @products/alpha/api/timeout
//! ```
//!
//! # Architecture
//! See `../../plan/ANALYSIS_ADVERSARIAL_SECURITY.md` for deprecation protection

use anyhow::{bail, Result};
use clap::Args;
use std::path::PathBuf;
use ulid::Ulid;

use crate::core::fact::Fact;
use crate::core::storage::Storage;

#[derive(Args, Debug)]
pub struct DeprecateArgs {
    /// ID or path of the fact to deprecate
    pub target: String,

    /// Reason for deprecation
    #[arg(short, long)]
    pub reason: Option<String>,

    /// ID of replacement fact (if any)
    #[arg(long)]
    pub replaced_by: Option<String>,
}

pub fn run(args: DeprecateArgs) -> Result<()> {
    // 1. Find .meh directory
    let meh_dir = find_meh_dir()?;
    let db_path = meh_dir.join("data.db");
    let storage = Storage::open(&db_path)?;

    // 2. Find fact to deprecate
    let fact = find_fact(&storage, &args.target)?;

    // 3. Mark as deprecated
    storage.mark_deprecated(&fact.id)?;

    println!("⚠️  Deprecated: meh-{}", fact.id);
    println!("   Path: {}", fact.path);
    if let Some(reason) = &args.reason {
        println!("   Reason: {}", reason);
    }
    if let Some(replacement) = &args.replaced_by {
        println!("   Replaced by: {}", replacement);
    }

    Ok(())
}

/// Find a fact by ID or path
fn find_fact(storage: &Storage, target: &str) -> Result<Fact> {
    if target.starts_with("meh-") {
        let id_str = target.trim_start_matches("meh-");
        let id =
            Ulid::from_string(id_str).map_err(|_| anyhow::anyhow!("Invalid meh ID: {}", target))?;
        storage
            .get_by_id(&id)?
            .ok_or_else(|| anyhow::anyhow!("Fact not found: {}", target))
    } else {
        let facts = storage.get_by_path(target)?;
        facts
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Fact not found: {}", target))
    }
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
            bail!("Not a meh repository (or any parent directory). Run 'meh init' first.");
        }
    }
}
