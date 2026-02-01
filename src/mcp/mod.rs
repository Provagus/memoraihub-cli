//! MCP (Model Context Protocol) Server
//!
//! Exposes meh knowledge base via MCP tools for AI integration.
//!
//! # Tools
//! - `meh_search` - Semantic search across facts
//! - `meh_federated_search` - Search across multiple KBs
//! - `meh_get_fact` - Get single fact by ID or path
//! - `meh_browse` - Browse paths (ls/tree)
//! - `meh_add` - Add new fact
//! - `meh_correct` - Correct existing fact
//! - `meh_extend` - Extend existing fact
//! - `meh_deprecate` - Deprecate fact
//! - `meh_get_notifications` - Get pending notifications (per session)
//! - `meh_ack_notifications` - Acknowledge notifications
//! - `meh_subscribe` - Subscribe to categories/paths
//! - `meh_bulk_vote` - Vote on multiple facts at once
//! - `meh_list_kbs` - List available knowledge bases
//! - `meh_switch_kb` - Switch to different KB

mod handlers;
mod jsonrpc;
mod server;
mod state;
mod tools;

pub use server::run_mcp_server;
