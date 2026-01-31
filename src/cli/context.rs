//! `meh context` command
//!
//! Manage active context (local or remote KB).
//!
//! # Usage
//! ```bash
//! meh context                              # Show current context
//! meh context set http://localhost:3000/my-kb  # Set remote context
//! meh context set local                    # Switch to local
//! meh context clear                        # Clear remote, use local
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
    /// Set active context (URL/kb-slug or "local")
    Set {
        /// Context: "local", "http://server/kb-slug", or just "kb-slug" (uses default server)
        context: String,
    },
    /// Clear remote context, use local
    Clear,
    /// Show current context
    Show,
}

pub fn run(args: ContextArgs) -> Result<()> {
    match args.command {
        None | Some(ContextCommand::Show) => show_context(),
        Some(ContextCommand::Set { context }) => set_context(&context),
        Some(ContextCommand::Clear) => clear_context(),
    }
}

fn show_context() -> Result<()> {
    let config = Config::load()?;
    
    println!("📍 Current Context\n");
    
    if let Some(url) = &config.server.url {
        if let Some(kb) = &config.server.default_kb {
            println!("   Mode:   Remote");
            println!("   Server: {}", url);
            println!("   KB:     {}", kb);
            if config.server.token.is_some() {
                println!("   Auth:   ✓ Token configured");
            }
        } else {
            println!("   Mode:   Remote (no KB set)");
            println!("   Server: {}", url);
            println!("   ⚠️  Use 'meh context set {}/KB_SLUG' to set KB", url);
        }
    } else {
        println!("   Mode:   Local");
        println!("   DB:     {}", config.data_dir().display());
    }
    
    println!("\n💡 Commands:");
    println!("   meh context set http://server:3000/kb-slug  # Use remote");
    println!("   meh context set local                       # Use local");
    println!("   meh context clear                           # Clear remote");
    
    Ok(())
}

fn set_context(context: &str) -> Result<()> {
    let mut config = Config::load()?;
    
    if context == "local" {
        // Switch to local
        config.server.url = None;
        config.server.default_kb = None;
        save_config(&config)?;
        println!("✅ Switched to local context");
        println!("   DB: {}", config.data_dir().display());
        return Ok(());
    }
    
    // Parse URL/kb-slug
    // Formats:
    //   http://localhost:3000/my-kb -> server=http://localhost:3000, kb=my-kb
    //   my-kb -> uses existing server URL, sets kb=my-kb
    
    if context.starts_with("http://") || context.starts_with("https://") {
        // Full URL with KB slug
        let url = url::Url::parse(context)?;
        let path = url.path().trim_start_matches('/');
        
        if path.is_empty() {
            bail!("URL must include KB slug: {}/KB_SLUG", context);
        }
        
        // Extract server base URL and KB slug
        let mut base_url = url.clone();
        base_url.set_path("");
        
        config.server.url = Some(base_url.to_string().trim_end_matches('/').to_string());
        config.server.default_kb = Some(path.to_string());
        
        save_config(&config)?;
        println!("✅ Remote context set");
        println!("   Server: {}", config.server.url.as_ref().unwrap());
        println!("   KB:     {}", path);
    } else {
        // Just KB slug - use existing server or error
        if config.server.url.is_none() {
            bail!(
                "No server configured. Use full URL:\n\
                 meh context set http://localhost:3000/{}", 
                context
            );
        }
        
        config.server.default_kb = Some(context.to_string());
        save_config(&config)?;
        println!("✅ KB changed to: {}", context);
        println!("   Server: {}", config.server.url.as_ref().unwrap());
    }
    
    Ok(())
}

fn clear_context() -> Result<()> {
    let mut config = Config::load()?;
    config.server.url = None;
    config.server.default_kb = None;
    save_config(&config)?;
    
    println!("✅ Remote context cleared");
    println!("   Now using local: {}", config.data_dir().display());
    
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
