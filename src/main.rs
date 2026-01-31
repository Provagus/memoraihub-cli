//! meh CLI - Entry point
//!
//! Usage: meh <command> [options]
//!
//! See `../AGENTS.md` for development guidelines.

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use meh::cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // Parse CLI arguments
    let cli = Cli::parse();

    // Check if this command should show notifications hint
    let show_hint = should_show_notifications_hint(&cli.command);

    // Run command
    let result = match cli.command {
        Commands::Add(args) => meh::cli::add::run(args),
        Commands::Show(args) => meh::cli::show::run(args).await,
        Commands::Search(args) => meh::cli::search::run(args).await,
        Commands::Ls(args) => meh::cli::browse::run_ls(args),
        Commands::Tree(args) => meh::cli::browse::run_tree(args),
        Commands::Correct(args) => meh::cli::correct::run(args),
        Commands::Extend(args) => meh::cli::extend::run(args),
        Commands::Deprecate(args) => meh::cli::deprecate::run(args),
        Commands::Gc(args) => run_gc(args),
        Commands::Init(args) => meh::cli::init::run(args),
        Commands::Config(args) => meh::cli::config::run(args),
        Commands::Context(args) => meh::cli::context::run(args),
        Commands::Notifications(args) => run_notifications(args),
        Commands::Pending(args) => run_pending(args),
        Commands::Stats(args) => meh::cli::stats::execute(args),
        Commands::Serve(args) => run_serve(args).await,
        Commands::Kbs(args) => meh::cli::kbs::execute(args).await,
    };

    // Show notifications hint if appropriate
    if show_hint && result.is_ok() {
        show_pending_notifications_hint();
    }

    result
}

/// Check if command should show notifications hint
fn should_show_notifications_hint(command: &Commands) -> bool {
    !matches!(
        command,
        Commands::Serve(_)
            | Commands::Notifications(_)
            | Commands::Context(_)
            | Commands::Config(_)
            | Commands::Init(_)
    )
}

/// Show hint about pending notifications (if any)
fn show_pending_notifications_hint() {
    // Try to get pending count - don't fail if it doesn't work
    if let Ok(config) = meh::config::Config::load() {
        let data_dir = config.data_dir();
        // Notifications are in notifications.db, not data.db
        let notifications_db = data_dir
            .parent()
            .map(|p| p.join("notifications.db"))
            .unwrap_or_else(|| data_dir.with_extension("notifications.db"));

        if notifications_db.exists() {
            if let Ok(storage) =
                meh::core::notifications::NotificationStorage::open(&notifications_db)
            {
                // Use a consistent session ID for CLI
                let session_id = "cli-session";
                if let Ok(pending) = storage.count_pending_for_session(session_id) {
                    if pending > 0 {
                        eprintln!(
                            "\n📬 {} notification(s) pending (meh notifications)",
                            pending
                        );
                    }
                }
            }
        }
    }
}

async fn run_serve(args: meh::cli::serve::ServeArgs) -> Result<()> {
    use meh::config::Config;
    use meh::core::storage::Storage;

    // Determine database path
    let db_path = if let Some(ref path) = args.db {
        path.clone()
    } else {
        let config = Config::load()?;
        config.data_dir()
    };

    // Auto-create database if needed
    if !db_path.exists() {
        if args.auto_init {
            eprintln!("📁 Creating database at {:?}", db_path);
            if let Some(parent) = db_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            // Opening storage will create the database
            let _ = Storage::open(&db_path)?;
            eprintln!("✅ Database initialized");
        } else {
            anyhow::bail!(
                "Database not found at {:?}. Run 'meh init' or use --auto-init.",
                db_path
            );
        }
    }

    eprintln!("🚀 Starting MCP server (transport: {})", args.transport);
    eprintln!("📂 Database: {:?}", db_path);

    match args.transport.as_str() {
        "stdio" => {
            // run_mcp_server is sync, no await needed
            meh::run_mcp_server(db_path)?;
        }
        "http" => {
            eprintln!("HTTP transport not yet implemented. Use stdio.");
            anyhow::bail!("HTTP transport not yet implemented");
        }
        _ => {
            anyhow::bail!(
                "Unknown transport: {}. Use 'stdio' or 'http'.",
                args.transport
            );
        }
    }

    Ok(())
}

fn run_gc(args: meh::cli::gc::GcArgs) -> Result<()> {
    let config = meh::config::Config::load()?;
    meh::cli::gc::run(args, &config)
}

fn run_notifications(args: meh::cli::notifications::NotificationsArgs) -> Result<()> {
    let config = meh::config::Config::load()?;
    meh::cli::notifications::execute(args, &config)
}

fn run_pending(args: meh::cli::pending::PendingArgs) -> Result<()> {
    let config = meh::config::Config::load()?;
    meh::cli::pending::execute(args, &config)
}
