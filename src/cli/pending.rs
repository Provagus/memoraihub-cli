//! Pending review management CLI commands
//!
//! Handles both:
//! - Local pending_review facts (in data.db)
//! - Pending queue for remote KB writes (in pending_queue.db)

use anyhow::Result;
use clap::{Args, Subcommand};
use dialoguer::{theme::ColorfulTheme, Select};
use ulid::Ulid;

use crate::config::Config;
use crate::core::fact::Fact;
use crate::core::pending_queue::{PendingQueue, PendingWrite, PendingWriteType};
use crate::core::storage::Storage;

#[derive(Args, Debug)]
pub struct PendingArgs {
    #[command(subcommand)]
    pub command: Option<PendingCommands>,

    /// Interactive review mode
    #[arg(short = 'i', long = "interactive", global = true)]
    pub interactive: bool,
}

#[derive(Subcommand, Debug)]
pub enum PendingCommands {
    /// List all pending items (local facts + remote queue)
    List,

    /// Interactive review of pending items
    Review,

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

    // If -i flag or no subcommand, run interactive
    if args.interactive || args.command.is_none() {
        return run_interactive_review(storage.as_ref(), &queue, config);
    }

    match args.command.unwrap() {
        PendingCommands::List => list_pending(storage.as_ref(), &queue),
        PendingCommands::Review => run_interactive_review(storage.as_ref(), &queue, config),
        PendingCommands::Approve { id } => approve_one(storage.as_ref(), &queue, &id, config),
        PendingCommands::Reject { id } => reject_one(storage.as_ref(), &queue, &id),
        PendingCommands::ApproveAll { yes } => approve_all(storage.as_ref(), &queue, yes, config),
        PendingCommands::RejectAll { yes } => reject_all(storage.as_ref(), &queue, yes),
    }
}

// ============================================================================
// Interactive Review Mode
// ============================================================================

/// Unified pending item for interactive review
enum PendingItem {
    Local(Fact),
    Remote(PendingWrite),
}

impl PendingItem {
    fn id_string(&self) -> String {
        match self {
            PendingItem::Local(f) => format_meh_id(&f.id),
            PendingItem::Remote(w) => format_queue_id(&w.id),
        }
    }

    fn path(&self) -> &str {
        match self {
            PendingItem::Local(f) => &f.path,
            PendingItem::Remote(w) => &w.path,
        }
    }

    fn title(&self) -> String {
        match self {
            PendingItem::Local(f) => f.title.clone(),
            PendingItem::Remote(w) => w.title.clone().unwrap_or_else(|| "Untitled".to_string()),
        }
    }

    fn content(&self) -> &str {
        match self {
            PendingItem::Local(f) => &f.content,
            PendingItem::Remote(w) => &w.content,
        }
    }

    fn item_type(&self) -> &str {
        match self {
            PendingItem::Local(_) => "local",
            PendingItem::Remote(w) => match w.write_type {
                PendingWriteType::Add => "add",
                PendingWriteType::Correct => "correct",
                PendingWriteType::Extend => "extend",
                PendingWriteType::Deprecate => "deprecate",
            },
        }
    }

    fn target_kb(&self) -> Option<&str> {
        match self {
            PendingItem::Local(_) => None,
            PendingItem::Remote(w) => Some(&w.target_kb),
        }
    }
}

struct ReviewStats {
    approved: usize,
    rejected: usize,
    skipped: usize,
}

fn run_interactive_review(
    storage: Option<&Storage>,
    queue: &PendingQueue,
    config: &Config,
) -> Result<()> {
    // Collect all pending items
    let mut items: Vec<PendingItem> = Vec::new();

    if let Some(storage) = storage {
        let pending = storage.get_pending_review()?;
        for fact in pending {
            items.push(PendingItem::Local(fact));
        }
    }

    let queued = queue.list_all()?;
    for write in queued {
        items.push(PendingItem::Remote(write));
    }

    if items.is_empty() {
        println!("✨ No pending items to review.");
        return Ok(());
    }

    println!("\n📋 Interactive Pending Review ({} items)\n", items.len());
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let theme = ColorfulTheme::default();
    let mut stats = ReviewStats {
        approved: 0,
        rejected: 0,
        skipped: 0,
    };

    let mut idx = 0;
    while idx < items.len() {
        let item = &items[idx];

        // Display item header
        println!(
            "\x1b[1m[{}/{}]\x1b[0m {} \x1b[90m({})\x1b[0m",
            idx + 1,
            items.len(),
            item.id_string(),
            item.item_type()
        );
        println!("\x1b[36mPath:\x1b[0m  {}", item.path());
        println!("\x1b[36mTitle:\x1b[0m {}", item.title());
        if let Some(kb) = item.target_kb() {
            println!("\x1b[36mTarget:\x1b[0m {}", kb);
        }
        println!();

        // Show content preview (first 10 lines)
        let content_lines: Vec<&str> = item.content().lines().take(10).collect();
        println!("\x1b[90m┌─────────────────────────────────────────────────────────────────────────────┐\x1b[0m");
        for line in &content_lines {
            let truncated = if line.len() > 75 {
                format!("{}...", &line[..72])
            } else {
                line.to_string()
            };
            println!("\x1b[90m│\x1b[0m {:<76}\x1b[90m│\x1b[0m", truncated);
        }
        let total_lines = item.content().lines().count();
        if total_lines > 10 {
            println!(
                "\x1b[90m│\x1b[0m \x1b[33m... ({} more lines)\x1b[0m{:<53}\x1b[90m│\x1b[0m",
                total_lines - 10,
                ""
            );
        }
        println!("\x1b[90m└─────────────────────────────────────────────────────────────────────────────┘\x1b[0m");
        println!();

        // Action selection
        let actions = &[
            "✅ Approve",
            "❌ Reject",
            "📄 View full content",
            "⏭️  Skip (leave pending)",
            "🚀 Approve all remaining",
            "🚪 Quit",
        ];

        let selection = Select::with_theme(&theme)
            .with_prompt("Action")
            .items(actions)
            .default(0)
            .interact()?;

        match selection {
            0 => {
                // Approve
                match &items[idx] {
                    PendingItem::Local(fact) => {
                        if let Some(storage) = storage {
                            if let Some(ref supersedes_id) = fact.supersedes {
                                storage.mark_superseded(supersedes_id)?;
                            }
                            storage.approve_fact(&fact.id)?;
                        }
                    }
                    PendingItem::Remote(write) => {
                        push_to_remote(write, config)?;
                        queue.remove(&write.id)?;
                    }
                }
                println!("\x1b[32m✅ Approved!\x1b[0m\n");
                stats.approved += 1;
                idx += 1;
            }
            1 => {
                // Reject
                match &items[idx] {
                    PendingItem::Local(fact) => {
                        if let Some(storage) = storage {
                            storage.reject_fact(&fact.id)?;
                        }
                    }
                    PendingItem::Remote(write) => {
                        queue.remove(&write.id)?;
                    }
                }
                println!("\x1b[31m❌ Rejected!\x1b[0m\n");
                stats.rejected += 1;
                idx += 1;
            }
            2 => {
                // View full content
                println!("\n\x1b[1m=== Full Content ===\x1b[0m\n");
                println!("{}", item.content());
                println!("\n\x1b[1m=== End ===\x1b[0m\n");
                // Don't increment idx, let user act on same item
            }
            3 => {
                // Skip
                println!("\x1b[33m⏭️  Skipped\x1b[0m\n");
                stats.skipped += 1;
                idx += 1;
            }
            4 => {
                // Approve all remaining
                for i in idx..items.len() {
                    match &items[i] {
                        PendingItem::Local(fact) => {
                            if let Some(storage) = storage {
                                if let Some(ref supersedes_id) = fact.supersedes {
                                    let _ = storage.mark_superseded(supersedes_id);
                                }
                                let _ = storage.approve_fact(&fact.id);
                            }
                        }
                        PendingItem::Remote(write) => {
                            if push_to_remote(write, config).is_ok() {
                                let _ = queue.remove(&write.id);
                            }
                        }
                    }
                    stats.approved += 1;
                }
                println!(
                    "\x1b[32m🚀 Approved {} remaining items!\x1b[0m\n",
                    items.len() - idx
                );
                break;
            }
            5 => {
                // Quit
                println!("\n\x1b[33m🚪 Exiting review\x1b[0m\n");
                break;
            }
            _ => {}
        }
    }

    // Final summary
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("\n📊 Review complete:");
    println!("   ✅ Approved: {}", stats.approved);
    println!("   ❌ Rejected: {}", stats.rejected);
    println!("   ⏭️  Skipped:  {}", stats.skipped);
    println!();

    Ok(())
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

    // Get server info for this KB
    let server = config
        .get_server_for_kb(&item.target_kb)
        .ok_or_else(|| anyhow::anyhow!("No server configured for KB '{}'", item.target_kb))?;

    let slug = kb_config
        .slug
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No slug configured for KB '{}'", item.target_kb))?;

    // Build HTTP client
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(server.timeout_secs))
        .build()?;

    // Determine endpoint and payload based on write type
    let (endpoint, payload) = match item.write_type {
        PendingWriteType::Add => {
            let endpoint = format!("{}/api/v1/kbs/{}/facts", server.url, slug);
            let payload = serde_json::json!({
                "path": item.path,
                "content": item.content,
                "tags": item.tags,
            });
            (endpoint, payload)
        }
        PendingWriteType::Correct => {
            let endpoint = format!(
                "{}/api/v1/kbs/{}/facts/{}/correct",
                server.url,
                slug,
                item.supersedes.as_deref().unwrap_or("")
            );
            let payload = serde_json::json!({
                "new_content": item.content,
            });
            (endpoint, payload)
        }
        PendingWriteType::Extend => {
            let endpoint = format!(
                "{}/api/v1/kbs/{}/facts/{}/extend",
                server.url,
                slug,
                item.extends.as_deref().unwrap_or("")
            );
            let payload = serde_json::json!({
                "extension": item.content,
            });
            (endpoint, payload)
        }
        PendingWriteType::Deprecate => {
            let endpoint = format!(
                "{}/api/v1/kbs/{}/facts/{}/deprecate",
                server.url, slug, item.path
            );
            let payload = serde_json::json!({
                "reason": item.reason,
            });
            (endpoint, payload)
        }
    };

    // Make request
    let mut request = client.post(&endpoint).json(&payload);

    if let Some(ref key) = server.api_key {
        request = request.header("X-API-Key", key);
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
