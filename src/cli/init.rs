//! `meh init` command
//!
//! Initializes a new meh repository.
//!
//! # Usage
//! ```bash
//! meh init                    # Initialize in current directory
//! meh init /path/to/project   # Initialize in specific path
//! meh init --global           # Initialize global ~/.meh
//! ```

use anyhow::{bail, Result};
use clap::Args;
use std::fs;
use std::path::PathBuf;

use crate::config::Config;
use crate::core::storage::Storage;

#[derive(Args, Debug)]
pub struct InitArgs {
    /// Path to initialize (default: current directory)
    pub path: Option<PathBuf>,

    /// Initialize global config (~/.meh)
    #[arg(long)]
    pub global: bool,

    /// Force re-initialization
    #[arg(short, long)]
    pub force: bool,
}

pub fn run(args: InitArgs) -> Result<()> {
    // 1. Determine target path
    let base_path = if args.global {
        directories::UserDirs::new()
            .map(|u| u.home_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        args.path.unwrap_or_else(|| PathBuf::from("."))
    };

    let meh_dir = base_path.join(".meh");

    // 2. Check if already initialized
    if is_meh_repo(&base_path) && !args.force {
        bail!(
            "Directory {} is already a meh repository. Use --force to reinitialize.",
            base_path.display()
        );
    }

    println!("🚀 Initializing meh in: {}", base_path.display());

    // 3. Create .meh/ directory structure
    fs::create_dir_all(&meh_dir)?;
    fs::create_dir_all(meh_dir.join("cache"))?;

    // 4. Create config file
    let config = Config::default();
    let config_path = meh_dir.join("config.toml");
    let config_content = toml::to_string_pretty(&config)?;
    fs::write(&config_path, config_content)?;

    // 5. Initialize SQLite database with schema
    let db_path = meh_dir.join("data.db");
    let _storage = Storage::open(&db_path)?;

    println!("\n✅ Initialized meh repository");
    println!("   Config: {}", config_path.display());
    println!("   Database: {}", db_path.display());
    println!("\nNext steps:");
    println!("  meh add @topic/subtopic \"Your first fact\"");
    println!("  meh search \"query\"");
    println!("  meh ls");

    Ok(())
}

/// Check if a directory is already a meh repository
fn is_meh_repo(path: &std::path::Path) -> bool {
    path.join(".meh").exists()
}
