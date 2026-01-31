//! Serve command - Start MCP server

use clap::Args;

/// Start MCP server for AI integration
#[derive(Args, Debug)]
pub struct ServeArgs {
    /// Transport mode (stdio or http)
    #[arg(long, default_value = "stdio")]
    pub transport: String,

    /// HTTP port (only for http transport)
    #[arg(long, default_value = "8080")]
    pub port: u16,
}
