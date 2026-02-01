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

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
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

    /// Use global config (~/.meh/config.toml) instead of local
    #[arg(short, long)]
    pub global: bool,
}

fn get_config_path(global: bool) -> PathBuf {
    if global {
        crate::config::dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".meh")
            .join("config.toml")
    } else {
        PathBuf::from(".meh").join("config.toml")
    }
}

pub fn run(args: ConfigArgs) -> Result<()> {
    let config_path = get_config_path(args.global);

    if args.path {
        println!("Global: {}", get_config_path(true).display());
        println!("Local:  {}", get_config_path(false).display());
        println!();
        if config_path.exists() {
            println!("✓ Active: {}", config_path.display());
        } else {
            println!("⚠ No config file found at {}", config_path.display());
        }
        return Ok(());
    }

    if args.edit {
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "notepad".to_string());
        
        // Create config file if it doesn't exist
        if !config_path.exists() {
            if let Some(parent) = config_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&config_path, "# meh configuration\n\n")?;
            println!("Created {}", config_path.display());
        }
        
        std::process::Command::new(&editor)
            .arg(&config_path)
            .status()
            .with_context(|| format!("Failed to open editor: {}", editor))?;
        return Ok(());
    }

    if args.list || (args.key.is_none() && args.value.is_none()) {
        // List all config from file
        if config_path.exists() {
            let content = fs::read_to_string(&config_path)?;
            println!("📋 Configuration ({}):\n", config_path.display());
            println!("{}", content);
        } else {
            println!("📋 No config file at {}", config_path.display());
            println!();
            println!("Create one with:");
            println!("  meh config --edit");
            println!("  meh config core.gc_auto true");
        }
        return Ok(());
    }

    if let Some(key) = &args.key {
        if let Some(value) = &args.value {
            // Set value
            set_config_value(&config_path, key, value)?;
            println!("✅ Set {} = {} (in {})", key, value, config_path.display());
        } else {
            // Get value
            let val = get_config_value(&config_path, key)?;
            if let Some(v) = val {
                println!("{}", v);
            } else {
                println!("(not set)");
            }
        }
    }

    Ok(())
}

/// Set a nested config value using dot notation (e.g., "core.gc_auto")
fn set_config_value(path: &PathBuf, key: &str, val: &str) -> Result<()> {
    use toml_edit::{value, DocumentMut};

    // Create parent directory if needed
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Load or create document
    let content = if path.exists() {
        fs::read_to_string(path)?
    } else {
        String::new()
    };

    let mut doc: DocumentMut = content.parse().context("Failed to parse config.toml")?;

    // Parse key into parts (e.g., "core.gc_auto" -> ["core", "gc_auto"])
    let parts: Vec<&str> = key.split('.').collect();

    if parts.len() == 1 {
        // Top-level key
        doc[parts[0]] = value(parse_toml_value(val));
    } else if parts.len() == 2 {
        // Section.key
        if doc.get(parts[0]).is_none() {
            doc[parts[0]] = toml_edit::table();
        }
        doc[parts[0]][parts[1]] = value(parse_toml_value(val));
    } else {
        anyhow::bail!("Key too deep: {}. Max depth is section.key", key);
    }

    fs::write(path, doc.to_string())?;
    Ok(())
}

/// Get a config value by dot notation key
fn get_config_value(path: &PathBuf, key: &str) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path)?;
    let doc: toml::Value = content.parse().context("Failed to parse config.toml")?;

    let parts: Vec<&str> = key.split('.').collect();

    let val = if parts.len() == 1 {
        doc.get(parts[0])
    } else if parts.len() == 2 {
        doc.get(parts[0]).and_then(|t| t.get(parts[1]))
    } else {
        None
    };

    Ok(val.map(|v| match v {
        toml::Value::String(s) => s.clone(),
        other => other.to_string(),
    }))
}

/// Parse string value to appropriate TOML type
fn parse_toml_value(s: &str) -> toml_edit::Value {
    // Try bool
    if s == "true" {
        return toml_edit::value(true).into_value().unwrap();
    }
    if s == "false" {
        return toml_edit::value(false).into_value().unwrap();
    }
    
    // Try integer
    if let Ok(i) = s.parse::<i64>() {
        return toml_edit::value(i).into_value().unwrap();
    }
    
    // Try float
    if let Ok(f) = s.parse::<f64>() {
        return toml_edit::value(f).into_value().unwrap();
    }
    
    // Default to string
    toml_edit::value(s).into_value().unwrap()
}
