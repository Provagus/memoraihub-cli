//! `meh add` command
//!
//! Adds a new fact to the knowledge base.
//!
//! # Usage
//! ```bash
//! meh add "@products/alpha/api/timeout" "Timeout is 30s"
//! meh add "Timeout is 30s" --path "@products/alpha/api/timeout"
//! meh add "Timeout is 30s" --tags api,config
//! ```
//!
//! # Architecture
//! See `../../plan/ANALYSIS_KNOWLEDGE_ORGANIZATION.md` for path structure.

use anyhow::{bail, Result};
use clap::Args;
use std::fs;
use std::path::PathBuf;

use crate::core::fact::Fact;
use crate::core::path::Path;
use crate::core::storage::Storage;

#[derive(Args, Debug)]
pub struct AddArgs {
    /// Path for the fact (e.g., @products/alpha/api/timeout)
    #[arg(short, long)]
    pub path: Option<String>,

    /// Content of the fact (can be first positional if path is flag)
    pub content: String,

    /// Tags for the fact (comma-separated)
    #[arg(short, long, value_delimiter = ',')]
    pub tags: Option<Vec<String>>,

    /// Source to add to (default: local)
    #[arg(short, long, default_value = "local")]
    pub source: String,

    /// Title for the fact (auto-generated if not provided)
    #[arg(long)]
    pub title: Option<String>,

    /// Read content from file
    #[arg(short = 'f', long)]
    pub file: Option<String>,
}

pub fn run(args: AddArgs) -> Result<()> {
    // 1. Find .meh directory
    let meh_dir = find_meh_dir()?;
    let db_path = meh_dir.join("data.db");

    // 2. Parse path
    let path = match &args.path {
        Some(p) => Path::parse(p)?,
        None => {
            bail!("Path is required. Use --path or provide as first argument.");
        }
    };

    // 3. Load content from file if --file
    let content = if let Some(file_path) = &args.file {
        fs::read_to_string(file_path)?
    } else {
        args.content.clone()
    };

    // 4. Generate title if not provided
    let title = args.title.clone().unwrap_or_else(|| {
        // Use first line or first 50 chars as title
        content
            .lines()
            .next()
            .unwrap_or(&content)
            .chars()
            .take(50)
            .collect()
    });

    // 5. Create Fact struct with builder pattern
    let mut fact = Fact::new(path.to_string(), title, content);
    fact.tags = args.tags.unwrap_or_default();
    fact.author_type = crate::core::fact::AuthorType::Human;
    fact.author_id = "cli".to_string();
    fact.generate_summary(150);

    let meh_id = fact.meh_id();

    // 6. Insert into storage
    let storage = Storage::open(&db_path)?;
    storage.insert(&fact)?;

    println!("✅ Fact added: {}", meh_id);
    println!("   Path: {}", path);
    println!("   Title: {}", fact.title);

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
