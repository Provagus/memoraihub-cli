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

    // Run command
    match cli.command {
        Commands::Add(args) => meh::cli::add::run(args),
        Commands::Show(args) => meh::cli::show::run(args),
        Commands::Search(args) => meh::cli::search::run(args),
        Commands::Ls(args) => meh::cli::browse::run_ls(args),
        Commands::Tree(args) => meh::cli::browse::run_tree(args),
        Commands::Correct(args) => meh::cli::correct::run(args),
        Commands::Extend(args) => meh::cli::extend::run(args),
        Commands::Deprecate(args) => meh::cli::deprecate::run(args),
        Commands::Init(args) => meh::cli::init::run(args),
        Commands::Config(args) => meh::cli::config::run(args),
        Commands::Serve(args) => run_serve(args).await,
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
            anyhow::bail!("Database not found at {:?}. Run 'meh init' or use --auto-init.", db_path);
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
            anyhow::bail!("Unknown transport: {}. Use 'stdio' or 'http'.", args.transport);
        }
    }

    Ok(())
}
