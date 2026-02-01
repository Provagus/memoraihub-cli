//! `meh kbs` command
//!
//! Manage knowledge bases on a remote server.
//!
//! # Usage
//! ```bash
//! meh kbs list                       # List all KBs
//! meh kbs create my-notes "My Notes" # Create new KB
//! meh kbs info my-notes              # Show KB info
//! meh kbs delete my-notes            # Delete KB
//! meh kbs use my-notes               # Set as default
//! ```

use anyhow::Result;
use clap::{Args, Subcommand};
use colored::Colorize;

use crate::config::Config;
use crate::remote::{CreateKbRequest, RemoteClient};

#[derive(Args, Debug)]
pub struct KbsArgs {
    /// Server name (from config [[servers]])
    #[arg(long, short = 's')]
    pub server: Option<String>,

    /// Server URL (overrides config)
    #[arg(long, env = "MEH_SERVER_URL")]
    pub server_url: Option<String>,

    /// API key (overrides config)
    #[arg(long, env = "MEH_API_KEY")]
    pub api_key: Option<String>,

    #[command(subcommand)]
    pub command: KbsCommands,
}

#[derive(Subcommand, Debug)]
pub enum KbsCommands {
    /// List all knowledge bases
    List,

    /// Add a new knowledge base to local config (interactive wizard)
    Add,

    /// Create a new knowledge base on remote server
    Create {
        /// URL-friendly slug (e.g., "my-notes")
        slug: String,

        /// Display name
        name: String,

        /// Description
        #[arg(short, long)]
        description: Option<String>,

        /// Visibility: public or private
        #[arg(short, long, default_value = "public")]
        visibility: String,
    },

    /// Show info about a knowledge base
    Info {
        /// KB slug
        slug: String,
    },

    /// Delete a knowledge base
    Delete {
        /// KB slug
        slug: String,

        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },

    /// Set default KB for remote operations
    Use {
        /// KB slug (or "none" to clear)
        slug: String,
    },

    /// Test connection to server
    Ping,
}

/// Execute kbs command
pub async fn execute(args: KbsArgs) -> Result<()> {
    let config = Config::load()?;

    match &args.command {
        KbsCommands::Add => add_kb_interactive().await,
        KbsCommands::Ping => ping(&args, &config).await,
        KbsCommands::List => list(&args, &config).await,
        KbsCommands::Create {
            slug,
            name,
            description,
            visibility,
        } => {
            create(
                &args,
                &config,
                slug.clone(),
                name.clone(),
                description.clone(),
                visibility.clone(),
            )
            .await
        }
        KbsCommands::Info { slug } => info(&args, &config, slug.clone()).await,
        KbsCommands::Delete { slug, force } => delete(&args, &config, slug.clone(), *force).await,
        KbsCommands::Use { slug } => use_kb(&config, slug.clone()),
    }
}

/// Create remote client from args + config
fn create_client(args: &KbsArgs, config: &Config) -> Result<RemoteClient> {
    // Priority: --server-url flag > --server name from config > first server in config
    let (url, api_key, timeout) = if let Some(url) = &args.server_url {
        // Explicit URL provided
        (url.clone(), args.api_key.clone(), 30)
    } else if let Some(server_name) = &args.server {
        // Server name provided - look up in config
        let server = config.get_server(server_name).ok_or_else(|| {
            anyhow::anyhow!(
                "Server '{}' not found in config.\n\
                 Add it with: meh kbs add\n\
                 Or use --server-url flag",
                server_name
            )
        })?;
        (
            server.url.clone(),
            server.api_key.clone(),
            server.timeout_secs,
        )
    } else if let Some(server) = config.servers.first() {
        // Use first server from config
        (
            server.url.clone(),
            server.api_key.clone(),
            server.timeout_secs,
        )
    } else {
        return Err(anyhow::anyhow!(
            "No server configured.\n\
             Add one with: meh kbs add\n\
             Or use --server-url flag"
        ));
    };

    // Override api_key from args if provided
    let final_api_key = args.api_key.clone().or(api_key);

    RemoteClient::new(&url, None, final_api_key, timeout)
}

/// Test connection to server
async fn ping(args: &KbsArgs, config: &Config) -> Result<()> {
    let client = create_client(args, config)?;

    eprint!("Connecting to server... ");
    let health = client.health().await?;

    println!("{}", "OK".green().bold());
    println!("  Status:  {}", health.status);
    println!("  Version: {}", health.version);

    Ok(())
}

/// List knowledge bases
async fn list(args: &KbsArgs, config: &Config) -> Result<()> {
    let client = create_client(args, config)?;
    let kbs = client.list_kbs().await?;

    if kbs.is_empty() {
        println!("No knowledge bases found.");
        println!("\nCreate one with: meh kbs create <slug> <name>");
        return Ok(());
    }

    println!("{}", "Knowledge Bases".bold());
    println!("{}", "═".repeat(60));

    for kb in &kbs {
        let visibility_badge = if kb.visibility == "private" {
            "🔒".to_string()
        } else {
            "🌍".to_string()
        };

        // Check if any KB in config uses this slug
        let is_configured = config
            .kbs
            .kb
            .iter()
            .any(|k| k.slug.as_ref().map(|s| s == &kb.slug).unwrap_or(false));

        let configured_badge = if is_configured {
            " ★".yellow().to_string()
        } else {
            "".to_string()
        };

        println!(
            "{} {}{}",
            visibility_badge,
            kb.slug.cyan().bold(),
            configured_badge
        );
        println!("   {}", kb.name);
        if let Some(desc) = &kb.description {
            if !desc.is_empty() {
                println!("   {}", desc.dimmed());
            }
        }
    }

    println!("\n{} knowledge base(s)", kbs.len());
    Ok(())
}

/// Create a knowledge base
async fn create(
    args: &KbsArgs,
    config: &Config,
    slug: String,
    name: String,
    description: Option<String>,
    visibility: String,
) -> Result<()> {
    let client = create_client(args, config)?;

    let req = CreateKbRequest {
        slug: slug.clone(),
        name,
        description,
        visibility: Some(visibility),
    };

    let kb = client.create_kb(req).await?;

    println!(
        "{} Created knowledge base: {}",
        "✓".green(),
        kb.slug.cyan().bold()
    );
    println!("  Name:       {}", kb.name);
    println!("  Visibility: {}", kb.visibility);
    println!("  ID:         {}", kb.id.dimmed());

    println!("\nTo use this KB by default:");
    println!("  meh kbs use {}", slug);

    Ok(())
}

/// Show KB info
async fn info(args: &KbsArgs, config: &Config, slug: String) -> Result<()> {
    let client = create_client(args, config)?;
    let kb = client.get_kb(&slug).await?;

    println!("{}", "Knowledge Base".bold());
    println!("{}", "═".repeat(40));
    println!("Slug:        {}", kb.slug.cyan());
    println!("Name:        {}", kb.name);
    println!(
        "Description: {}",
        kb.description.as_deref().unwrap_or("(none)")
    );
    println!("Visibility:  {}", kb.visibility);
    println!("Owner:       {}", kb.owner_id);
    println!("Created:     {}", kb.created_at);
    println!("ID:          {}", kb.id.dimmed());

    Ok(())
}

/// Delete a KB
async fn delete(args: &KbsArgs, config: &Config, slug: String, force: bool) -> Result<()> {
    if !force {
        eprint!(
            "Delete knowledge base '{}'? This cannot be undone. [y/N] ",
            slug.red()
        );

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    let client = create_client(args, config)?;
    client.delete_kb(&slug).await?;

    println!("{} Deleted knowledge base: {}", "✓".green(), slug);
    Ok(())
}

/// Set default KB (sets as primary in kbs config)
fn use_kb(config: &Config, slug: String) -> Result<()> {
    let mut config = config.clone();

    if slug == "none" || slug.is_empty() {
        // Find current primary and reset to "local"
        config.kbs.primary = "local".to_string();
        println!("Cleared default knowledge base (reset to local).");
    } else {
        // Check if KB exists in config
        if !config.kbs.kb.iter().any(|k| k.name == slug) {
            println!("{} KB '{}' not found in config.", "⚠".yellow(), slug);
            println!("  Add it first with: meh kbs add");
            return Ok(());
        }
        config.kbs.primary = slug.clone();
        println!(
            "{} Set primary knowledge base: {}",
            "✓".green(),
            slug.cyan()
        );
    }

    // Save to config
    let config_path = Config::find_local_config()
        .or_else(Config::global_config_path)
        .ok_or_else(|| anyhow::anyhow!("Cannot determine config path"))?;

    config.save_to(&config_path)?;
    println!("  Saved to: {}", config_path.display());

    Ok(())
}

/// Interactive wizard to add a new KB to local config
async fn add_kb_interactive() -> Result<()> {
    use crate::config::{KbConfig, ServerEntry, WritePolicy};
    use std::io::{self, Write};

    println!("{}", "📚 Add Knowledge Base to Config".bold());
    println!("{}", "═".repeat(40));
    println!("This wizard will help you add a new KB to your config.\n");

    // Helper to read line
    fn read_line(prompt: &str) -> Result<String> {
        print!("{}", prompt);
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        Ok(input.trim().to_string())
    }

    fn read_line_default(prompt: &str, default: &str) -> Result<String> {
        print!("{} [{}]: ", prompt, default.dimmed());
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let trimmed = input.trim();
        if trimmed.is_empty() {
            Ok(default.to_string())
        } else {
            Ok(trimmed.to_string())
        }
    }

    // Step 1: KB name
    println!("{}", "Step 1: KB Name".cyan().bold());
    println!("A short identifier for this KB (used in config and --kb flag)");
    let name = read_line("Name: ")?;
    if name.is_empty() {
        anyhow::bail!("Name cannot be empty");
    }
    println!();

    // Step 2: KB type
    println!("{}", "Step 2: KB Type".cyan().bold());
    println!("  1. sqlite  - Local SQLite database");
    println!("  2. remote  - Remote server (memoraihub or self-hosted)");
    let kb_type_choice = read_line_default("Type (1 or 2)", "1")?;
    let kb_type = match kb_type_choice.as_str() {
        "1" | "sqlite" => "sqlite",
        "2" | "remote" => "remote",
        _ => "sqlite",
    };
    println!();

    let mut config = Config::load()?;

    // Check if KB with this name already exists
    if config.kbs.kb.iter().any(|k| k.name == name) {
        anyhow::bail!(
            "KB '{}' already exists in config. Remove it first or use a different name.",
            name
        );
    }

    let (path, server_name, slug) = if kb_type == "sqlite" {
        // SQLite KB
        println!("{}", "Step 3: Database Path".cyan().bold());
        println!("Path to SQLite database file (relative or absolute)");
        let path = read_line_default("Path", ".meh/data.db")?;
        (Some(path), None, None)
    } else {
        // Remote KB - need server
        println!("{}", "Step 3: Server".cyan().bold());

        // Show existing servers
        if !config.servers.is_empty() {
            println!("Existing servers:");
            for (i, s) in config.servers.iter().enumerate() {
                println!("  {}. {} ({})", i + 1, s.name.cyan(), s.url.dimmed());
            }
            println!("  N. Add new server");
            println!();
        }

        let server_choice = if config.servers.is_empty() {
            "n".to_string()
        } else {
            read_line_default("Choose server (number or 'n' for new)", "1")?
        };

        let server_name = if server_choice.eq_ignore_ascii_case("n") {
            // Add new server
            println!();
            println!("{}", "New Server".cyan().bold());

            let srv_name = read_line("Server name: ")?;
            if srv_name.is_empty() {
                anyhow::bail!("Server name cannot be empty");
            }

            let srv_url = read_line("Server URL (e.g., https://api.memoraihub.com): ")?;
            if srv_url.is_empty() {
                anyhow::bail!("Server URL cannot be empty");
            }

            println!();
            println!("API key (paste your key, or leave empty to add later):");
            let srv_api_key = read_line("API key: ")?;

            let server_entry = ServerEntry {
                name: srv_name.clone(),
                url: srv_url,
                api_key: if srv_api_key.is_empty() {
                    None
                } else {
                    Some(srv_api_key)
                },
                timeout_secs: 30,
            };

            config.servers.push(server_entry);
            srv_name
        } else {
            // Use existing server
            let idx: usize = server_choice
                .parse::<usize>()
                .unwrap_or(1)
                .saturating_sub(1);
            config
                .servers
                .get(idx)
                .map(|s| s.name.clone())
                .ok_or_else(|| anyhow::anyhow!("Invalid server selection"))?
        };

        println!();
        println!("{}", "Step 4: KB Slug".cyan().bold());
        println!("The slug of the KB on the server");
        let slug = read_line_default("Slug", &name)?;

        (None, Some(server_name), Some(slug))
    };
    println!();

    // Write policy
    println!("{}", "Write Policy".cyan().bold());
    println!("  allow - AI can write freely");
    println!("  deny  - Read-only (AI cannot write)");
    println!("  ask   - AI writes go to pending review");
    let write_choice = read_line_default("Write policy", "allow")?;
    let write = match write_choice.as_str() {
        "deny" => WritePolicy::Deny,
        "ask" => WritePolicy::Ask,
        _ => WritePolicy::Allow,
    };
    println!();

    // Set as primary?
    println!("{}", "Set as Primary?".cyan().bold());
    let set_primary = read_line_default("Set as primary KB? (y/n)", "n")?;
    let set_as_primary =
        set_primary.eq_ignore_ascii_case("y") || set_primary.eq_ignore_ascii_case("yes");
    println!();

    // Build KB config
    let server_name_for_print = server_name.clone();
    let kb_config = KbConfig {
        name: name.clone(),
        kb_type: kb_type.to_string(),
        path,
        server: server_name,
        slug,
        write,
    };

    config.kbs.kb.push(kb_config);

    if set_as_primary {
        config.kbs.primary = name.clone();
    }

    // Add to search order if not already there
    if !config.kbs.search_order.contains(&name) {
        config.kbs.search_order.push(name.clone());
    }

    // Determine config path
    let config_path = Config::find_local_config()
        .or_else(Config::global_config_path)
        .ok_or_else(|| anyhow::anyhow!("Cannot determine config path"))?;

    // Save
    config.save_to(&config_path)?;

    // Summary
    println!("{}", "═".repeat(40));
    println!(
        "{} Added KB '{}' to config",
        "✓".green(),
        name.cyan().bold()
    );
    println!("  Type:   {}", kb_type);
    println!("  Write:  {:?}", write);
    if let Some(ref srv) = server_name_for_print {
        println!("  Server: {}", srv);
    }
    if set_as_primary {
        println!("  Primary: {}", "yes".green());
    }
    println!("  Config: {}", config_path.display());

    if kb_type == "remote" {
        // Check if server has API key
        let server = server_name_for_print
            .as_ref()
            .and_then(|n| config.get_server(n));
        if server.map(|s| s.api_key.is_none()).unwrap_or(true) {
            println!("\n{}", "⚠️  Next step:".yellow().bold());
            println!("  Add your API key to the server in config.toml");
        }
    }

    Ok(())
}
