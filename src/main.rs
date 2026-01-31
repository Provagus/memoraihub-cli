//! meh CLI - Entry point
//!
//! Usage: meh <command> [options]
//!
//! See `../AGENTS.md` for development guidelines.

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use meh::cli::{Cli, Commands};

fn main() -> Result<()> {
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
    }
}
