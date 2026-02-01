//! MCP Tool handlers
//!
//! Each module handles a group of related tools.

pub mod facts;
pub mod kbs;
pub mod notifications;
pub mod search;

use serde_json::Value;

use super::state::ServerState;

/// Result type for tool handlers
pub type ToolResult = Result<String, String>;

/// Dispatch a tool call to the appropriate handler
pub fn dispatch_tool(state: &mut ServerState, name: &str, args: &Value) -> ToolResult {
    match name {
        // Search tools
        "meh_search" => search::do_search(state, args),
        "meh_federated_search" => search::do_federated_search(state, args),

        // Fact tools
        "meh_get_fact" => facts::do_get_fact(state, args),
        "meh_browse" => facts::do_browse(state, args),
        "meh_add" => facts::do_add(state, args),
        "meh_correct" => facts::do_correct(state, args),
        "meh_extend" => facts::do_extend(state, args),
        "meh_deprecate" => facts::do_deprecate(state, args),

        // Notification tools
        "meh_get_notifications" => notifications::do_get_notifications(state, args),
        "meh_ack_notifications" => notifications::do_ack_notifications(state, args),
        "meh_subscribe" => notifications::do_subscribe(state, args),

        // KB management tools
        "meh_bulk_vote" => kbs::do_bulk_vote(state, args),
        "meh_list_kbs" => kbs::do_list_kbs(state, args),
        "meh_switch_kb" => kbs::do_switch_kb(state, args),
        "meh_switch_context" => kbs::do_switch_context(state, args),
        "meh_show_context" => kbs::do_show_context(state, args),

        _ => Err(format!("Unknown tool: {}", name)),
    }
}
