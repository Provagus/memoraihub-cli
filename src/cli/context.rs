//! `meh context` command
//!
//! Manage active context (which KB is primary).
//!
//! # Usage
//! ```bash
//! meh context                   # Show current context
//! meh context set <kb-name>     # Set primary KB
//! meh context clear             # Clear primary, use first in list
//! ```

use anyhow::{bail, Result};
use clap::{Args, Subcommand};
use std::path::PathBuf;

use crate::config::Config;

#[derive(Args, Debug)]
pub struct ContextArgs {
    #[command(subcommand)]
    pub command: Option<ContextCommand>,
}

#[derive(Subcommand, Debug)]
pub enum ContextCommand {
    /// Set active context (KB name from config)
    Set {
        /// KB name as defined in config
        kb_name: String,
    },
    /// Clear primary KB setting
    Clear,
    /// Show current context
    Show,
}

pub fn run(args: ContextArgs) -> Result<()> {
    match args.command {
        None | Some(ContextCommand::Show) => show_context(),
        Some(ContextCommand::Set { kb_name }) => set_context(&kb_name),
        Some(ContextCommand::Clear) => clear_context(),
    }
}

fn show_context() -> Result<()> {
    let config = Config::load()?;

    println!("📍 Current Context\n");

    // Find primary KB
    let primary_name = &config.kbs.primary;
    let primary_kb = config.kbs.kb.iter().find(|k| &k.name == primary_name);

    if let Some(kb) = primary_kb {
        println!("   Primary: {} ({})", kb.name, kb.kb_type);

        match kb.kb_type.as_str() {
            "sqlite" => {
                if let Some(path) = &kb.path {
                    println!("   Path:    {}", path);
                }
            }
            "remote" => {
                if let Some(server_name) = &kb.server {
                    if let Some(server) = config.get_server(server_name) {
                        println!("   Server:  {} ({})", server_name, server.url);
                    } else {
                        println!("   Server:  {} (not configured)", server_name);
                    }
                }
                if let Some(slug) = &kb.slug {
                    println!("   Slug:    {}", slug);
                }
            }
            _ => {}
        }
        println!("   Write:   {:?}", kb.write);
    } else if !config.kbs.kb.is_empty() {
        println!("   ⚠️  Primary '{}' not found in config", primary_name);
        println!(
            "   Available KBs: {}",
            config
                .kbs
                .kb
                .iter()
                .map(|k| k.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    } else {
        println!("   No KBs configured");
    }

    println!("\n💡 Commands:");
    println!("   meh context set <kb-name>   # Set primary KB");
    println!("   meh kbs list                # List all KBs");
    println!("   meh kbs add                 # Add a new KB");

    Ok(())
}

fn set_context(kb_name: &str) -> Result<()> {
    let mut config = Config::load()?;

    // Check if KB exists
    if !config.kbs.kb.iter().any(|k| k.name == kb_name) {
        let available: Vec<_> = config.kbs.kb.iter().map(|k| k.name.as_str()).collect();
        if available.is_empty() {
            bail!("No KBs configured. Use 'meh kbs add' to add one.");
        } else {
            bail!(
                "KB '{}' not found. Available: {}",
                kb_name,
                available.join(", ")
            );
        }
    }

    config.kbs.primary = kb_name.to_string();
    save_config(&config)?;

    let kb = config.kbs.kb.iter().find(|k| k.name == kb_name).unwrap();
    println!("✅ Primary KB set to: {}", kb_name);
    println!("   Type: {}", kb.kb_type);

    Ok(())
}

fn clear_context() -> Result<()> {
    let mut config = Config::load()?;

    // Set to first KB or "default"
    let new_primary = config
        .kbs
        .kb
        .first()
        .map(|k| k.name.clone())
        .unwrap_or_else(|| "default".to_string());

    config.kbs.primary = new_primary.clone();
    save_config(&config)?;

    println!("✅ Primary KB reset to: {}", new_primary);

    Ok(())
}

fn save_config(config: &Config) -> Result<()> {
    // Save to local .meh/config.toml if exists, otherwise global
    let path = Config::find_local_config()
        .or_else(|| {
            let global = Config::global_config_path()?;
            // Create parent dir if needed
            if let Some(parent) = global.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            Some(global)
        })
        .unwrap_or_else(|| PathBuf::from(".meh/config.toml"));

    config.save_to(&path)?;
    Ok(())
}
