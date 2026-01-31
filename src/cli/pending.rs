//! Pending review management CLI commands
//!
//! Handles both:
//! - Local pending_review facts (in data.db)
//! - Pending queue for remote KB writes (in pending_queue.db)

use anyhow::Result;
use clap::{Args, Subcommand};
use ulid::Ulid;

use crate::config::Config;
use crate::core::pending_queue::{PendingQueue, PendingWrite, PendingWriteType};
use crate::core::storage::Storage;

#[derive(Args, Debug)]
pub struct PendingArgs {
    #[command(subcommand)]
    pub command: PendingCommands,
}

#[derive(Subcommand, Debug)]
pub enum PendingCommands {
    /// List all pending items (local facts + remote queue)
    List,

    /// Approve a pending item
    Approve {
        /// ID to approve (meh-xxx for local, queue-xxx for remote)
        id: String,
    },

    /// Reject a pending item (delete it)
    Reject {
        /// ID to reject (meh-xxx for local, queue-xxx for remote)
        id: String,
    },

    /// Approve all pending items
    ApproveAll {
        /// Skip confirmation
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Reject all pending items
    RejectAll {
        /// Skip confirmation
        #[arg(short = 'y', long)]
        yes: bool,
    },
}

pub fn execute(args: PendingArgs, config: &Config) -> Result<()> {
    let db_path = config.data_dir();

    // Open storage if exists
    let storage = if db_path.exists() {
        Some(Storage::open(&db_path)?)
    } else {
        None
    };

    // Open pending queue
    let queue_path = db_path
        .parent()
        .map(|p| p.join("pending_queue.db"))
        .unwrap_or_else(|| std::path::PathBuf::from(".meh/pending_queue.db"));
    let queue = PendingQueue::open(&queue_path)?;

    match args.command {
        PendingCommands::List => list_pending(storage.as_ref(), &queue),
        PendingCommands::Approve { id } => approve_one(storage.as_ref(), &queue, &id, config),
        PendingCommands::Reject { id } => reject_one(storage.as_ref(), &queue, &id),
        PendingCommands::ApproveAll { yes } => approve_all(storage.as_ref(), &queue, yes, config),
        PendingCommands::RejectAll { yes } => reject_all(storage.as_ref(), &queue, yes),
    }
}

fn list_pending(storage: Option<&Storage>, queue: &PendingQueue) -> Result<()> {
    let mut has_items = false;

    // List local pending_review facts
    if let Some(storage) = storage {
        let pending = storage.get_pending_review()?;
        if !pending.is_empty() {
            has_items = true;
            println!("📋 Local pending review ({} fact(s)):\n", pending.len());

            for fact in &pending {
                println!("  {} {}", format_meh_id(&fact.id), fact.path);
                if let Some(ref summary) = fact.summary {
                    println!("    {}", summary);
                } else {
                    let first_line = fact.content.lines().next().unwrap_or("");
                    let truncated = if first_line.len() > 60 {
                        format!("{}...", &first_line[..60])
                    } else {
                        first_line.to_string()
                    };
                    println!("    {}", truncated);
                }
                println!();
            }
        }
    }

    // List remote queue
    let queued = queue.list_all()?;
    if !queued.is_empty() {
        has_items = true;
        println!("📤 Remote KB queue ({} item(s)):\n", queued.len());

        for item in &queued {
            println!(
                "  {} → {} [{}]",
                format_queue_id(&item.id),
                item.target_kb,
                item.write_type
            );
            println!("    Path: {}", item.path);
            if let Some(ref title) = item.title {
                println!("    Title: {}", title);
            }
            if let Some(ref supersedes) = item.supersedes {
                println!("    Supersedes: {}", supersedes);
            }
            if let Some(ref extends) = item.extends {
                println!("    Extends: {}", extends);
            }
            println!();
        }
    }

    if !has_items {
        println!("✨ No pending items.");
        return Ok(());
    }

    println!("Commands:");
    println!("  meh pending approve <id>   - Approve (meh-xxx or queue-xxx)");
    println!("  meh pending reject <id>    - Reject/delete");

    Ok(())
}

fn approve_one(
    storage: Option<&Storage>,
    queue: &PendingQueue,
    id_str: &str,
    config: &Config,
) -> Result<()> {
    // Check if it's a queue item (remote KB)
    if id_str.starts_with("queue-") {
        let ulid_str = id_str.strip_prefix("queue-").unwrap();
        let id = Ulid::from_string(ulid_str)
            .map_err(|e| anyhow::anyhow!("Invalid queue ID '{}': {}", id_str, e))?;

        if let Some(item) = queue.get(&id)? {
            println!("🚀 Approving remote write to KB '{}'...", item.target_kb);

            if item.target_url.is_empty() {
                anyhow::bail!("Remote KB URL not configured for '{}'", item.target_kb);
            }

            // Push to remote
            push_to_remote(&item, config)?;

            // Remove from queue
            queue.remove(&id)?;
            println!("✅ Pushed to remote: {} {}", item.write_type, item.path);
        } else {
            anyhow::bail!("Queue item {} not found", id_str);
        }
    } else {
        // Local pending_review fact
        let storage = storage.ok_or_else(|| anyhow::anyhow!("No local database found"))?;
        let id = parse_meh_id(id_str)?;

        if let Some(fact) = storage.get_by_id(&id)? {
            // If it's a correction, also mark original as superseded
            if let Some(ref supersedes_id) = fact.supersedes {
                storage.mark_superseded(supersedes_id)?;
            }

            storage.approve_fact(&id)?;
            println!("✅ Approved: {} {}", format_meh_id(&id), fact.path);
        } else {
            anyhow::bail!("Fact {} not found", id_str);
        }
    }

    Ok(())
}

fn reject_one(storage: Option<&Storage>, queue: &PendingQueue, id_str: &str) -> Result<()> {
    if id_str.starts_with("queue-") {
        let ulid_str = id_str.strip_prefix("queue-").unwrap();
        let id = Ulid::from_string(ulid_str)
            .map_err(|e| anyhow::anyhow!("Invalid queue ID '{}': {}", id_str, e))?;

        if let Some(item) = queue.get(&id)? {
            queue.remove(&id)?;
            println!("🗑️  Rejected queue item: {} {}", item.write_type, item.path);
        } else {
            anyhow::bail!("Queue item {} not found", id_str);
        }
    } else {
        let storage = storage.ok_or_else(|| anyhow::anyhow!("No local database found"))?;
        let id = parse_meh_id(id_str)?;

        if let Some(fact) = storage.get_by_id(&id)? {
            storage.reject_fact(&id)?;
            println!("🗑️  Rejected: {} {}", format_meh_id(&id), fact.path);
        } else {
            anyhow::bail!("Fact {} not found", id_str);
        }
    }

    Ok(())
}

fn approve_all(
    storage: Option<&Storage>,
    queue: &PendingQueue,
    skip_confirm: bool,
    config: &Config,
) -> Result<()> {
    let mut local_pending = Vec::new();

    if let Some(storage) = storage {
        local_pending = storage.get_pending_review()?;
    }
    let queue_pending = queue.list_all()?;

    let total = local_pending.len() + queue_pending.len();

    if total == 0 {
        println!("✨ No pending items.");
        return Ok(());
    }

    if !skip_confirm {
        println!("About to approve {} item(s):", total);

        for fact in &local_pending {
            println!("  {} {}", format_meh_id(&fact.id), fact.path);
        }
        for item in &queue_pending {
            println!(
                "  {} → {} {}",
                format_queue_id(&item.id),
                item.target_kb,
                item.path
            );
        }

        print!("\nApprove all? [y/N] ");
        use std::io::{self, Write};
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    // Approve local facts
    if let Some(storage) = storage {
        for fact in &local_pending {
            // Handle corrections - mark original as superseded
            if let Some(ref supersedes_id) = fact.supersedes {
                storage.mark_superseded(supersedes_id)?;
            }
            storage.approve_fact(&fact.id)?;
            println!("✅ {}", fact.path);
        }
    }

    // Approve queue items (push to remote)
    for item in &queue_pending {
        match push_to_remote(item, config) {
            Ok(_) => {
                queue.remove(&item.id)?;
                println!("🚀 {} → {}", item.path, item.target_kb);
            }
            Err(e) => {
                println!("❌ Failed to push {}: {}", item.path, e);
            }
        }
    }

    println!("\n✅ Approved {} item(s).", total);

    Ok(())
}

fn reject_all(storage: Option<&Storage>, queue: &PendingQueue, skip_confirm: bool) -> Result<()> {
    let mut local_pending = Vec::new();

    if let Some(storage) = storage {
        local_pending = storage.get_pending_review()?;
    }
    let queue_pending = queue.list_all()?;

    let total = local_pending.len() + queue_pending.len();

    if total == 0 {
        println!("✨ No pending items.");
        return Ok(());
    }

    if !skip_confirm {
        println!("About to REJECT (delete) {} item(s):", total);

        for fact in &local_pending {
            println!("  {} {}", format_meh_id(&fact.id), fact.path);
        }
        for item in &queue_pending {
            println!(
                "  {} → {} {}",
                format_queue_id(&item.id),
                item.target_kb,
                item.path
            );
        }

        print!("\nReject all? This cannot be undone! [y/N] ");
        use std::io::{self, Write};
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    // Reject local facts
    if let Some(storage) = storage {
        for fact in &local_pending {
            storage.reject_fact(&fact.id)?;
            println!("🗑️  {}", fact.path);
        }
    }

    // Reject queue items
    for item in &queue_pending {
        queue.remove(&item.id)?;
        println!("🗑️  {} (queued for {})", item.path, item.target_kb);
    }

    println!("\n🗑️  Rejected {} item(s).", total);

    Ok(())
}

/// Push a pending write to remote KB via HTTP
fn push_to_remote(item: &PendingWrite, config: &Config) -> Result<()> {
    use std::time::Duration;

    let kb_config = config
        .get_kb(&item.target_kb)
        .ok_or_else(|| anyhow::anyhow!("KB '{}' not found in config", item.target_kb))?;

    let url = kb_config
        .url
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No URL configured for KB '{}'", item.target_kb))?;

    // Get API key from environment
    let api_key = if let Some(ref env_var) = kb_config.api_key_env {
        std::env::var(env_var).ok()
    } else {
        None
    };

    // Build HTTP client
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(config.server.timeout_secs))
        .build()?;

    // Determine endpoint and payload based on write type
    let (endpoint, payload) = match item.write_type {
        PendingWriteType::Add => {
            let endpoint = format!("{}/api/v1/facts", url);
            let payload = serde_json::json!({
                "path": item.path,
                "content": item.content,
                "tags": item.tags,
            });
            (endpoint, payload)
        }
        PendingWriteType::Correct => {
            let endpoint = format!(
                "{}/api/v1/facts/{}/correct",
                url,
                item.supersedes.as_deref().unwrap_or("")
            );
            let payload = serde_json::json!({
                "new_content": item.content,
            });
            (endpoint, payload)
        }
        PendingWriteType::Extend => {
            let endpoint = format!(
                "{}/api/v1/facts/{}/extend",
                url,
                item.extends.as_deref().unwrap_or("")
            );
            let payload = serde_json::json!({
                "extension": item.content,
            });
            (endpoint, payload)
        }
        PendingWriteType::Deprecate => {
            let endpoint = format!("{}/api/v1/facts/{}/deprecate", url, item.path);
            let payload = serde_json::json!({
                "reason": item.reason,
            });
            (endpoint, payload)
        }
    };

    // Make request
    let mut request = client.post(&endpoint).json(&payload);

    if let Some(key) = api_key {
        request = request.header("Authorization", format!("Bearer {}", key));
    }

    let response = request.send()?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        anyhow::bail!("Remote API error ({}): {}", status, body);
    }

    Ok(())
}

fn parse_meh_id(id_str: &str) -> Result<Ulid> {
    let clean = id_str.trim_start_matches("meh-");
    Ulid::from_string(clean).map_err(|e| anyhow::anyhow!("Invalid ID '{}': {}", id_str, e))
}

fn format_meh_id(id: &Ulid) -> String {
    format!("meh-{}", id.to_string().to_lowercase())
}

fn format_queue_id(id: &Ulid) -> String {
    format!("queue-{}", id.to_string().to_lowercase())
}
