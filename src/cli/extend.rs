//! `meh extend` command
//!
//! Adds additional information to an existing fact without replacing it.
//!
//! # Usage
//! ```bash
//! meh extend meh-01HQ3K2JN5 "Also applies to websocket connections"
//! meh extend @products/alpha/api/timeout "Exception: /health endpoint uses 5s"
//! ```
//!
//! # Difference from correct
//! - `correct` = "this replaces that" (supersedes)
//! - `extend` = "this adds to that" (extends)

use anyhow::{bail, Result};
use clap::Args;
use std::fs;
use std::path::PathBuf;
use ulid::Ulid;

use crate::core::fact::Fact;
use crate::core::storage::Storage;

#[derive(Args, Debug)]
pub struct ExtendArgs {
    /// ID or path of the fact to extend
    pub target: String,

    /// Additional content
    pub content: String,

    /// Read content from file
    #[arg(short = 'f', long)]
    pub file: Option<String>,
}

pub fn run(args: ExtendArgs) -> Result<()> {
    // 1. Find .meh directory
    let meh_dir = find_meh_dir()?;
    let db_path = meh_dir.join("data.db");
    let storage = Storage::open(&db_path)?;

    // 2. Find original fact
    let original = find_fact(&storage, &args.target)?;

    // 3. Get content
    let content = if let Some(file_path) = &args.file {
        fs::read_to_string(file_path)?
    } else {
        args.content.clone()
    };

    // 4. Create extension fact
    let mut extension = Fact::extension(&original, content);
    extension.author_type = crate::core::fact::AuthorType::Human;
    extension.author_id = "cli".to_string();
    extension.generate_summary(150);
    let meh_id = extension.meh_id();

    // 5. Insert new fact
    storage.insert(&extension)?;

    println!("✅ Extension created: {}", meh_id);
    println!("   Extends: meh-{}", original.id);
    println!("   Path: {}", extension.path);

    Ok(())
}

/// Find a fact by ID or path
fn find_fact(storage: &Storage, target: &str) -> Result<Fact> {
    if target.starts_with("meh-") {
        let id_str = target.trim_start_matches("meh-");
        let id = Ulid::from_string(id_str)
            .map_err(|_| anyhow::anyhow!("Invalid meh ID: {}", target))?;
        storage.get_by_id(&id)?
            .ok_or_else(|| anyhow::anyhow!("Fact not found: {}", target))
    } else {
        let facts = storage.get_by_path(target)?;
        facts.into_iter().next()
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
            bail!(
                "Not a meh repository (or any parent directory). Run 'meh init' first."
            );
        }
    }
}
