//! CLI module - Command definitions and handlers
//!
//! Architecture: See `../../plan/DESIGN_CLI_MCP_SERVER.md`

use clap::{Parser, Subcommand};

pub mod add;
pub mod browse;
pub mod config;
pub mod context;
pub mod correct;
pub mod deprecate;
pub mod extend;
pub mod gc;
pub mod init;
pub mod kbs;
pub mod notifications;
pub mod pending;
pub mod remote_ops;
pub mod search;
pub mod show;
pub mod stats;

/// meh - AI Knowledge Management CLI
///
/// Git for AI memory. Append-only knowledge base with path-based organization.
#[derive(Parser, Debug)]
#[command(name = "meh")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Config file path
    #[arg(short, long, global = true, env = "MEH_CONFIG")]
    pub config: Option<String>,

    /// Use remote server instead of local database
    #[arg(long, global = true, env = "MEH_SERVER_URL")]
    pub server: Option<String>,

    /// Knowledge base slug (for remote operations)
    #[arg(long, global = true, env = "MEH_KB")]
    pub kb: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Initialize a new meh repository
    Init(init::InitArgs),

    /// Add a new fact
    Add(add::AddArgs),

    /// Show a fact by path or ID
    Show(show::ShowArgs),

    /// Search facts
    Search(search::SearchArgs),

    /// List paths (like ls)
    Ls(browse::LsArgs),

    /// Show path tree
    Tree(browse::TreeArgs),

    /// Correct a fact (creates superseding fact)
    Correct(correct::CorrectArgs),

    /// Extend a fact (adds related information)
    Extend(extend::ExtendArgs),

    /// Deprecate a fact
    Deprecate(deprecate::DeprecateArgs),

    /// Garbage collect old deprecated/superseded facts
    Gc(gc::GcArgs),

    /// Get or set configuration
    Config(config::ConfigArgs),

    /// Manage active context (local or remote KB)
    Context(context::ContextArgs),

    /// Manage notifications
    Notifications(notifications::NotificationsArgs),

    /// Manage pending review facts (approve/reject)
    Pending(pending::PendingArgs),

    /// Show database statistics
    Stats(stats::StatsArgs),

    /// Start MCP server (for AI integration)
    Serve(serve::ServeArgs),

    /// Manage remote knowledge bases
    Kbs(kbs::KbsArgs),
}

pub mod serve;
