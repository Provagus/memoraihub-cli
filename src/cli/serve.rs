//! Serve command - Start MCP server

use clap::Args;
use std::path::PathBuf;

/// Start MCP server for AI integration
#[derive(Args, Debug)]
pub struct ServeArgs {
    /// Transport mode (stdio or http)
    #[arg(long, default_value = "stdio")]
    pub transport: String,

    /// HTTP port (only for http transport)
    #[arg(long, default_value = "8080")]
    pub port: u16,

    /// Path to database file (default: auto-detect .meh/data.db)
    #[arg(long)]
    pub db: Option<PathBuf>,

    /// Create database if it doesn't exist
    #[arg(long, default_value = "true")]
    pub auto_init: bool,
}
