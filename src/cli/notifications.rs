//! Notifications CLI commands
//!
//! List, acknowledge, and manage notifications.
//! CLI uses session_id "cli" for tracking read position.

use anyhow::Result;
use chrono::{Local, TimeZone};
use clap::{Args, Subcommand};

use crate::config::Config;
use crate::core::notifications::{Category, NotificationStorage, Priority, Subscription};

/// CLI session ID - used for tracking what CLI user has seen
const CLI_SESSION_ID: &str = "cli";

#[derive(Args, Debug)]
pub struct NotificationsArgs {
    #[command(subcommand)]
    pub command: Option<NotificationsCommand>,

    /// Filter by category (facts, ci, security, docs, system)
    #[arg(long, short = 'c')]
    pub category: Option<String>,

    /// Filter by minimum priority (normal, high, critical)
    #[arg(long, short = 'p')]
    pub priority: Option<String>,

    /// Maximum number of notifications to show
    #[arg(long, short = 'n', default_value = "20")]
    pub limit: usize,

    /// Show in JSON format
    #[arg(long)]
    pub json: bool,

    /// Show all notifications (not just new since last read)
    #[arg(long)]
    pub all: bool,
}

#[derive(Subcommand, Debug)]
pub enum NotificationsCommand {
    /// Acknowledge all pending notifications (mark as read)
    Ack,

    /// Clear old notifications
    Clear {
        /// Days to keep (default: 7)
        #[arg(long, default_value = "7")]
        keep_days: i64,
    },

    /// Show notification counts
    Count,

    /// List available categories
    Categories,

    /// Subscribe to specific categories/paths
    Subscribe {
        /// Categories to subscribe to (comma-separated)
        #[arg(long, short = 'c')]
        categories: Option<String>,

        /// Path prefixes to subscribe to (comma-separated)
        #[arg(long, short = 'p')]
        paths: Option<String>,

        /// Minimum priority (normal, high, critical)
        #[arg(long)]
        priority: Option<String>,

        /// Show current subscription
        #[arg(long)]
        show: bool,
    },
}

/// Execute notifications command
pub fn execute(args: NotificationsArgs, config: &Config) -> Result<()> {
    let storage = open_notification_storage(config)?;

    match args.command {
        Some(NotificationsCommand::Ack) => execute_ack(&storage),
        Some(NotificationsCommand::Clear { keep_days }) => execute_clear(&storage, keep_days),
        Some(NotificationsCommand::Count) => execute_count(&storage, args.json),
        Some(NotificationsCommand::Categories) => execute_categories(&storage, args.json),
        Some(NotificationsCommand::Subscribe { categories, paths, priority, show }) => {
            execute_subscribe(&storage, categories, paths, priority, show, args.json)
        }
        None => execute_list(&storage, args),
    }
}

fn open_notification_storage(config: &Config) -> Result<NotificationStorage> {
    let data_dir = config.data_dir();
    let db_path = data_dir.parent()
        .map(|p| p.join("notifications.db"))
        .unwrap_or_else(|| data_dir.with_extension("notifications.db"));
    
    NotificationStorage::open(&db_path)
}

fn execute_list(storage: &NotificationStorage, args: NotificationsArgs) -> Result<()> {
    let pending = if args.all {
        // Get all notifications (ignoring session cursor)
        storage.get_for_session("_all", args.limit)?
    } else {
        storage.get_for_session(CLI_SESSION_ID, args.limit)?
    };

    // Apply additional filters
    let pending: Vec<_> = pending.into_iter()
        .filter(|n| {
            // Category filter
            if let Some(ref cat) = args.category {
                if n.category.as_str() != cat {
                    return false;
                }
            }
            // Priority filter
            if let Some(ref p) = args.priority {
                if let Some(min_p) = Priority::from_str(p) {
                    if n.priority < min_p {
                        return false;
                    }
                }
            }
            true
        })
        .collect();

    if pending.is_empty() {
        if !args.json {
            println!("✓ No new notifications");
        } else {
            println!("[]");
        }
        return Ok(());
    }

    if args.json {
        let json_list: Vec<serde_json::Value> = pending
            .iter()
            .map(|n| {
                serde_json::json!({
                    "id": n.id.to_string(),
                    "category": n.category.as_str(),
                    "priority": n.priority.as_str(),
                    "source": n.source,
                    "type": n.notification_type.as_str(),
                    "title": n.title,
                    "summary": n.summary,
                    "fact_id": n.fact_id.map(|id| id.to_string()),
                    "path": n.path,
                    "created_at": n.created_at.to_rfc3339(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json_list)?);
        return Ok(());
    }

    // Human-readable format
    println!("📬 {} new notification(s):\n", pending.len());

    for notif in &pending {
        let priority_icon = match notif.priority {
            Priority::Critical => "🔴",
            Priority::High => "🟠",
            Priority::Normal => "🟢",
        };

        let cat_icon = match notif.category {
            Category::Facts => "📝",
            Category::Ci => "🔧",
            Category::Security => "🔒",
            Category::Docs => "📚",
            Category::System => "⚙️",
            Category::Custom(_) => "📌",
        };

        let time = Local.from_utc_datetime(&notif.created_at.naive_utc());
        let time_str = time.format("%Y-%m-%d %H:%M").to_string();

        println!(
            "{} {} {} [{}] {}",
            priority_icon,
            cat_icon,
            notif.title,
            notif.category.as_str(),
            time_str
        );
        
        if !notif.summary.is_empty() {
            println!("   {}", notif.summary);
        }
        
        if let Some(path) = &notif.path {
            println!("   📁 {}", path);
        }
        
        println!("   ID: {}\n", notif.id);
    }

    // Auto-mark as seen (unless --all)
    if !args.all {
        if let Some(last) = pending.last() {
            storage.mark_seen(CLI_SESSION_ID, &last.id)?;
            println!("(Marked {} notifications as seen)", pending.len());
        }
    }

    Ok(())
}

fn execute_ack(storage: &NotificationStorage) -> Result<()> {
    let count = storage.acknowledge_all(CLI_SESSION_ID)?;
    println!("✓ Acknowledged {} notification(s)", count);
    Ok(())
}

fn execute_clear(storage: &NotificationStorage, keep_days: i64) -> Result<()> {
    let cleared = storage.clear_old(keep_days)?;
    println!("✓ Cleared {} old notification(s) (kept last {} days)", cleared, keep_days);
    Ok(())
}

fn execute_count(storage: &NotificationStorage, json: bool) -> Result<()> {
    let pending = storage.pending_count(CLI_SESSION_ID)?;
    let critical = storage.critical_count(CLI_SESSION_ID)?;
    let total = storage.unread_count()?;

    if json {
        println!("{}", serde_json::json!({
            "pending": pending,
            "critical": critical,
            "total": total,
        }));
    } else {
        println!("📬 Notifications:");
        println!("   New for you: {}", pending);
        println!("   Critical: {}", critical);
        println!("   Total in DB: {}", total);
    }

    Ok(())
}

fn execute_categories(storage: &NotificationStorage, json: bool) -> Result<()> {
    let categories = storage.list_categories()?;

    if json {
        println!("{}", serde_json::to_string_pretty(&categories)?);
    } else {
        println!("📂 Categories:");
        for (cat, count) in categories {
            let icon = match cat.as_str() {
                "facts" => "📝",
                "ci" => "🔧",
                "security" => "🔒",
                "docs" => "📚",
                "system" => "⚙️",
                _ => "📌",
            };
            println!("   {} {} ({})", icon, cat, count);
        }
    }

    Ok(())
}

fn execute_subscribe(
    storage: &NotificationStorage,
    categories: Option<String>,
    paths: Option<String>,
    priority: Option<String>,
    show: bool,
    json: bool,
) -> Result<()> {
    if show {
        // Show current subscription
        let (_, sub) = storage.get_or_create_session(CLI_SESSION_ID)?;
        
        if json {
            println!("{}", sub.to_json());
        } else {
            println!("📋 Current subscription:");
            if sub.categories.is_empty() {
                println!("   Categories: all");
            } else {
                let cats: Vec<&str> = sub.categories.iter().map(|c| c.as_str()).collect();
                println!("   Categories: {}", cats.join(", "));
            }
            if sub.path_prefixes.is_empty() {
                println!("   Paths: all");
            } else {
                println!("   Paths: {}", sub.path_prefixes.join(", "));
            }
            if let Some(p) = sub.priority_min {
                println!("   Min priority: {}", p.as_str());
            } else {
                println!("   Min priority: all");
            }
        }
        return Ok(());
    }

    // Build new subscription
    let mut sub = Subscription::default();

    if let Some(cats) = categories {
        let cat_list: Vec<Category> = cats
            .split(',')
            .map(|s| Category::from_str(s.trim()))
            .collect();
        sub = sub.categories(cat_list);
    }

    if let Some(ps) = paths {
        let path_list: Vec<String> = ps
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();
        sub = sub.paths(path_list);
    }

    if let Some(p) = priority {
        if let Some(prio) = Priority::from_str(&p) {
            sub = sub.priority_min(prio);
        }
    }

    storage.update_subscription(CLI_SESSION_ID, &sub)?;

    println!("✓ Subscription updated");
    if !sub.categories.is_empty() {
        let cats: Vec<&str> = sub.categories.iter().map(|c| c.as_str()).collect();
        println!("   Categories: {}", cats.join(", "));
    }
    if !sub.path_prefixes.is_empty() {
        println!("   Paths: {}", sub.path_prefixes.join(", "));
    }
    if let Some(p) = sub.priority_min {
        println!("   Min priority: {}", p.as_str());
    }

    Ok(())
}
