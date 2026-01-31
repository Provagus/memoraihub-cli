//! meh - AI Knowledge Management CLI
//!
//! Git for AI memory. Append-only knowledge base with path-based organization.
//!
//! # Architecture
//!
//! See `../plan/DECISIONS_UNIFIED.md` for architectural decisions.
//!
//! ## Key Concepts
//!
//! - **Path-based organization**: Facts have paths like `@products/alpha/api/timeout`
//! - **Detail Levels (L0-L3)**: How much detail to return (Catalog/Index/Summary/Full)
//! - **Append-only**: Never UPDATE, only INSERT with `supersedes` relation
//! - **Multi-source**: local, company, global, npm

pub mod cli;
pub mod core;
pub mod config;
pub mod mcp;

pub use core::fact::Fact;
pub use mcp::run_mcp_server;
pub use core::path::Path;
pub use core::storage::Storage;
