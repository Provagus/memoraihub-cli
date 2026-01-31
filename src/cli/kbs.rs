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
    /// Server URL (overrides config)
    #[arg(long, env = "MEH_SERVER_URL")]
    pub server_url: Option<String>,

    /// Auth token (overrides config)
    #[arg(long, env = "MEH_SERVER_TOKEN")]
    pub token: Option<String>,

    #[command(subcommand)]
    pub command: KbsCommands,
}

#[derive(Subcommand, Debug)]
pub enum KbsCommands {
    /// List all knowledge bases
    List,

    /// Create a new knowledge base
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
    let url = args
        .server_url
        .as_ref()
        .or(config.server.url.as_ref())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Server URL not configured.\n\
             Use --server-url flag or set server.url in config:\n  \
             meh config set server.url http://localhost:3000"
            )
        })?;

    let token = args
        .token
        .as_ref()
        .or(config.server.token.as_ref())
        .cloned();

    let api_key = config.server.api_key.clone();

    RemoteClient::new(url, token, api_key, config.server.timeout_secs)
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

        let is_default = config
            .server
            .default_kb
            .as_ref()
            .map(|d| d == &kb.slug)
            .unwrap_or(false);

        let default_badge = if is_default {
            " ★".yellow().to_string()
        } else {
            "".to_string()
        };

        println!(
            "{} {}{}",
            visibility_badge,
            kb.slug.cyan().bold(),
            default_badge
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

/// Set default KB
fn use_kb(config: &Config, slug: String) -> Result<()> {
    let mut config = config.clone();

    if slug == "none" || slug.is_empty() {
        config.server.default_kb = None;
        println!("Cleared default knowledge base.");
    } else {
        config.server.default_kb = Some(slug.clone());
        println!(
            "{} Set default knowledge base: {}",
            "✓".green(),
            slug.cyan()
        );
    }

    // Save to global config
    if let Some(config_path) = Config::global_config_path() {
        config.save_to(&config_path)?;
        println!("  Saved to: {}", config_path.display());
    }

    Ok(())
}
