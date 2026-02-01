//! MCP Tool handlers
//!
//! Each module handles a group of related tools.
//!
//! # Merged Tools (v2)
//! To reduce tool count from 17 to 4:
//! - `meh_facts` → search, get, browse, federated_search
//! - `meh_write` → add, correct, extend, deprecate, bulk_vote
//! - `meh_notify` → get, ack, subscribe
//! - `meh_context` → list_kbs, switch_kb, switch_context, show

pub mod facts;
pub mod kbs;
pub mod notifications;
pub mod search;

use serde_json::Value;

use super::state::ServerState;
use super::tools::{MehContextTool, MehFactsTool, MehNotifyTool, MehWriteTool};

/// Result type for tool handlers
pub type ToolResult = Result<String, String>;

/// Dispatch a tool call to the appropriate handler
pub fn dispatch_tool(state: &mut ServerState, name: &str, args: &Value) -> ToolResult {
    match name {
        // ====== Merged Tools (v2) ======
        "meh_facts" => dispatch_facts(state, args),
        "meh_write" => dispatch_write(state, args),
        "meh_notify" => dispatch_notify(state, args),
        "meh_context" => dispatch_context(state, args),

        // ====== Legacy Tools (for backwards compatibility) ======
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

// ============================================================================
// Merged Tool Dispatchers
// ============================================================================

/// Dispatch meh_facts tool based on action
fn dispatch_facts(state: &mut ServerState, args: &Value) -> ToolResult {
    let tool_args: MehFactsTool =
        serde_json::from_value(args.clone()).map_err(|e| format!("Invalid params: {}", e))?;

    match tool_args.action.as_str() {
        "search" => {
            let query = tool_args.query.ok_or("Missing 'query' for search action")?;
            let legacy_args = serde_json::json!({
                "query": query,
                "path_filter": tool_args.path_filter,
                "limit": tool_args.limit
            });
            search::do_search(state, &legacy_args)
        }
        "get" => {
            let id_or_path = tool_args.id_or_path.ok_or("Missing 'id_or_path' for get action")?;
            let legacy_args = serde_json::json!({
                "id_or_path": id_or_path,
                "include_history": tool_args.include_history
            });
            facts::do_get_fact(state, &legacy_args)
        }
        "browse" => {
            let legacy_args = serde_json::json!({
                "path": tool_args.path.unwrap_or_else(|| "@".to_string()),
                "mode": tool_args.mode.unwrap_or_else(|| "ls".to_string()),
                "depth": tool_args.depth.unwrap_or(3),
                "limit": tool_args.limit,
                "cursor": tool_args.cursor
            });
            facts::do_browse(state, &legacy_args)
        }
        "federated_search" => {
            let query = tool_args.query.ok_or("Missing 'query' for federated_search action")?;
            let legacy_args = serde_json::json!({
                "query": query,
                "kbs": tool_args.kbs,
                "limit_per_kb": tool_args.limit_per_kb.unwrap_or(10)
            });
            search::do_federated_search(state, &legacy_args)
        }
        _ => Err(format!(
            "Unknown action '{}' for meh_facts. Use: search, get, browse, federated_search",
            tool_args.action
        )),
    }
}

/// Dispatch meh_write tool based on action
fn dispatch_write(state: &mut ServerState, args: &Value) -> ToolResult {
    let tool_args: MehWriteTool =
        serde_json::from_value(args.clone()).map_err(|e| format!("Invalid params: {}", e))?;

    match tool_args.action.as_str() {
        "add" => {
            let path = tool_args.path.ok_or("Missing 'path' for add action")?;
            let content = tool_args.content.ok_or("Missing 'content' for add action")?;
            let legacy_args = serde_json::json!({
                "path": path,
                "content": content,
                "tags": tool_args.tags
            });
            facts::do_add(state, &legacy_args)
        }
        "correct" => {
            let fact_id = tool_args.fact_id.ok_or("Missing 'fact_id' for correct action")?;
            let new_content = tool_args.new_content.ok_or("Missing 'new_content' for correct action")?;
            let legacy_args = serde_json::json!({
                "fact_id": fact_id,
                "new_content": new_content,
                "reason": tool_args.reason
            });
            facts::do_correct(state, &legacy_args)
        }
        "extend" => {
            let fact_id = tool_args.fact_id.ok_or("Missing 'fact_id' for extend action")?;
            let extension = tool_args.extension.ok_or("Missing 'extension' for extend action")?;
            let legacy_args = serde_json::json!({
                "fact_id": fact_id,
                "extension": extension
            });
            facts::do_extend(state, &legacy_args)
        }
        "deprecate" => {
            let fact_id = tool_args.fact_id.ok_or("Missing 'fact_id' for deprecate action")?;
            let legacy_args = serde_json::json!({
                "fact_id": fact_id,
                "reason": tool_args.reason
            });
            facts::do_deprecate(state, &legacy_args)
        }
        "bulk_vote" => {
            let legacy_args = serde_json::json!({
                "votes": tool_args.votes
            });
            kbs::do_bulk_vote(state, &legacy_args)
        }
        _ => Err(format!(
            "Unknown action '{}' for meh_write. Use: add, correct, extend, deprecate, bulk_vote",
            tool_args.action
        )),
    }
}

/// Dispatch meh_notify tool based on action
fn dispatch_notify(state: &mut ServerState, args: &Value) -> ToolResult {
    let tool_args: MehNotifyTool =
        serde_json::from_value(args.clone()).map_err(|e| format!("Invalid params: {}", e))?;

    match tool_args.action.as_str() {
        "get" => {
            let legacy_args = serde_json::json!({
                "priority_min": tool_args.priority_min,
                "limit": tool_args.limit
            });
            notifications::do_get_notifications(state, &legacy_args)
        }
        "ack" => {
            let legacy_args = serde_json::json!({
                "notification_ids": tool_args.notification_ids
            });
            notifications::do_ack_notifications(state, &legacy_args)
        }
        "subscribe" => {
            let legacy_args = serde_json::json!({
                "categories": tool_args.categories,
                "path_prefixes": tool_args.path_prefixes,
                "priority_min": tool_args.priority_min,
                "show": tool_args.show
            });
            notifications::do_subscribe(state, &legacy_args)
        }
        _ => Err(format!(
            "Unknown action '{}' for meh_notify. Use: get, ack, subscribe",
            tool_args.action
        )),
    }
}

/// Dispatch meh_context tool based on action
fn dispatch_context(state: &mut ServerState, args: &Value) -> ToolResult {
    let tool_args: MehContextTool =
        serde_json::from_value(args.clone()).map_err(|e| format!("Invalid params: {}", e))?;

    match tool_args.action.as_str() {
        "list_kbs" => {
            let legacy_args = serde_json::json!({
                "detailed": tool_args.detailed
            });
            kbs::do_list_kbs(state, &legacy_args)
        }
        "switch_kb" => {
            let kb_name = tool_args.kb_name.ok_or("Missing 'kb_name' for switch_kb action")?;
            let legacy_args = serde_json::json!({
                "kb_name": kb_name
            });
            kbs::do_switch_kb(state, &legacy_args)
        }
        "switch_context" => {
            let context = tool_args.context.ok_or("Missing 'context' for switch_context action")?;
            let legacy_args = serde_json::json!({
                "context": context
            });
            kbs::do_switch_context(state, &legacy_args)
        }
        "show" => {
            kbs::do_show_context(state, &serde_json::json!({}))
        }
        _ => Err(format!(
            "Unknown action '{}' for meh_context. Use: list_kbs, switch_kb, switch_context, show",
            tool_args.action
        )),
    }
}
