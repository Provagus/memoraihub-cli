//! `meh config` command
//!
//! Get or set configuration values.
//!
//! # Usage
//! ```bash
//! meh config                  # Show all config
//! meh config user.name        # Get specific value
//! meh config user.name "AI"   # Set value
//! meh config --list           # List all
//! ```

use anyhow::Result;
use clap::Args;

#[derive(Args, Debug)]
pub struct ConfigArgs {
    /// Config key (e.g., user.name, core.default_source)
    pub key: Option<String>,

    /// Value to set
    pub value: Option<String>,

    /// List all config values
    #[arg(long)]
    pub list: bool,

    /// Edit config file in $EDITOR
    #[arg(short, long)]
    pub edit: bool,

    /// Show config file path
    #[arg(long)]
    pub path: bool,
}

pub fn run(args: ConfigArgs) -> Result<()> {
    // TODO: Implement
    // 1. Load config from ~/.meh/config.toml and .meh/config.toml
    // 2. Merge (local overrides global)
    // 3. Get/set/list based on args

    if args.path {
        println!("Global: ~/.meh/config.toml");
        println!("Local:  .meh/config.toml");
        return Ok(());
    }

    if args.list || (args.key.is_none() && args.value.is_none()) {
        // List all config
        println!("📋 Configuration:\n");
        println!("[user]");
        println!("  name = \"AI\"");
        println!("  agent_id = \"claude-opus-4\"");
        println!();
        println!("[core]");
        println!("  default_source = \"local\"");
        println!("  cache_max_mb = 100");
        println!();
        println!("[search]");
        println!("  default_limit = 20");
        println!("  token_budget = 3000");
        return Ok(());
    }

    if let Some(key) = &args.key {
        if let Some(value) = &args.value {
            // Set value
            println!("✅ Set {} = {}", key, value);
        } else {
            // Get value
            println!("{} = (value would be here)", key);
        }
    }

    Ok(())
}
