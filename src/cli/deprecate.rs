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

use anyhow::Result;
use clap::Args;

use super::utils::{find_fact, find_meh_dir};
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
