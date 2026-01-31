//! `meh correct` command
//!
//! Creates a correction that supersedes an existing fact.
//! This is the append-only way to "edit" - never UPDATE, always INSERT.
//!
//! # Usage
//! ```bash
//! meh correct meh-01HQ3K2JN5 "API timeout is 60s (changed in v2.1)"
//! meh correct @products/alpha/api/timeout "New timeout value"
//! ```
//!
//! # Architecture
//! See `../../plan/ARCHITECTURE_FINAL.md` - Append-only model

use anyhow::{bail, Result};
use clap::Args;
use std::fs;
use std::path::PathBuf;
use ulid::Ulid;

use crate::core::fact::Fact;
use crate::core::storage::Storage;

#[derive(Args, Debug)]
pub struct CorrectArgs {
    /// ID or path of the fact to correct
    pub target: String,

    /// Corrected content
    pub content: String,

    /// Reason for correction
    #[arg(short, long)]
    pub reason: Option<String>,

    /// Read content from file
    #[arg(short = 'f', long)]
    pub file: Option<String>,
}

pub fn run(args: CorrectArgs) -> Result<()> {
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

    // 4. Create correction fact
    let mut correction = Fact::correction(&original, content);
    correction.author_type = crate::core::fact::AuthorType::Human;
    correction.author_id = "cli".to_string();
    correction.generate_summary(150);
    let meh_id = correction.meh_id();

    // 5. Insert new fact
    storage.insert(&correction)?;

    // 6. Mark original as superseded
    storage.mark_superseded(&original.id)?;

    println!("✅ Correction created: {}", meh_id);
    println!("   Supersedes: meh-{}", original.id);
    println!("   Path: {}", correction.path);

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
