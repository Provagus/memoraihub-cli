//! KB management handlers for MCP (list_kbs, switch_kb, bulk_vote)

use serde_json::Value;
use ulid::Ulid;

use super::ToolResult;
use crate::config::{Config, WritePolicy};
use crate::core::fact::{Fact, FactType, Status};
use crate::core::PendingWrite;
use crate::mcp::state::ServerState;
use crate::mcp::tools::{MehBulkVoteTool, MehListKbsTool, MehSwitchContextTool, MehSwitchKbTool};

/// List available knowledge bases
pub fn do_list_kbs(state: &ServerState, args: &Value) -> ToolResult {
    let tool_args: MehListKbsTool =
        serde_json::from_value(args.clone()).map_err(|e| format!("Invalid params: {}", e))?;

    let config = Config::load().map_err(|e| format!("Config error: {}", e))?;

    if config.kbs.kb.is_empty() {
        return Ok(
            "No knowledge bases configured.\nUse `meh kbs add` in CLI to add one.".to_string(),
        );
    }

    let mut result = format!(
        "📚 Available Knowledge Bases (current: {})\n\n",
        state.kb_name
    );

    for kb in &config.kbs.kb {
        let is_current = kb.name == state.kb_name;
        let marker = if is_current { "▶ " } else { "  " };

        if tool_args.detailed {
            let server_info = if kb.kb_type == "remote" {
                kb.server.as_deref().unwrap_or("(no server)")
            } else {
                kb.path.as_deref().unwrap_or("(default path)")
            };

            result.push_str(&format!(
                "{}{}\n    Type: {}\n    {}: {}\n    Write: {:?}\n\n",
                marker,
                kb.name,
                kb.kb_type,
                if kb.kb_type == "remote" {
                    "Server"
                } else {
                    "Path"
                },
                server_info,
                kb.write
            ));
        } else {
            let type_emoji = if kb.kb_type == "remote" {
                "🌐"
            } else {
                "💾"
            };
            result.push_str(&format!(
                "{}{} {} ({})\n",
                marker, type_emoji, kb.name, kb.kb_type
            ));
        }
    }

    result.push_str("\n💡 Use meh_switch_kb({\"kb_name\": \"<name>\"}) to switch.");

    Ok(result)
}

/// Switch to a different knowledge base
pub fn do_switch_kb(state: &mut ServerState, args: &Value) -> ToolResult {
    let tool_args: MehSwitchKbTool =
        serde_json::from_value(args.clone()).map_err(|e| format!("Invalid params: {}", e))?;

    state.switch_kb(&tool_args.kb_name)?;

    Ok(format!(
        "✓ Switched to '{}'\n  Type: {}\n  Write: {:?}",
        state.kb_name,
        if state.is_remote_kb {
            "remote"
        } else {
            "sqlite"
        },
        state.write_policy
    ))
}

/// Record multiple votes in a single call
pub fn do_bulk_vote(state: &mut ServerState, args: &Value) -> ToolResult {
    state.check_write_allowed()?;

    let tool_args: MehBulkVoteTool =
        serde_json::from_value(args.clone()).map_err(|e| format!("Invalid params: {}", e))?;

    // If remote KB with "ask" policy, queue as a single pending item
    if state.is_remote_kb && state.write_policy == WritePolicy::Ask {
        let queue = state.open_pending_queue()?;
        let summary = format!("Bulk vote - {} votes", tool_args.votes.len());
        let pending = PendingWrite::new_add(
            &state.kb_name,
            state.remote_url.as_deref().unwrap_or(""),
            "@meh/pending/bulk-vote",
            &summary,
            vec!["bulk-vote".to_string()],
        );
        let id = pending.id;
        queue
            .enqueue(&pending)
            .map_err(|e| format!("Queue error: {}", e))?;

        return Ok(format!(
            "⏳ Queued bulk vote for remote KB '{}' (pending approval): queue-{}\n  Votes: {}\n  ℹ️ Human review required. Run `meh pending -i` for interactive review",
            state.kb_name, id, tool_args.votes.len()
        ));
    }

    // For each vote, create an extension fact referencing the original
    let mut created: Vec<String> = Vec::new();

    for v in &tool_args.votes {
        // Parse original fact ID
        let ulid_str = v
            .fact_id
            .strip_prefix("meh-")
            .ok_or("Invalid ID format - expected meh-XXX")?;
        let original_ulid =
            Ulid::from_string(ulid_str).map_err(|e| format!("Invalid ULID: {}", e))?;

        let original = state
            .storage
            .get_by_id(&original_ulid)
            .map_err(|e| format!("Error: {}", e))?
            .ok_or_else(|| format!("Original fact not found: {}", v.fact_id))?;

        // Compose extension content
        let reason = v.reason.as_deref().unwrap_or("");
        let extension_content = format!("## 🗳️ Vote\n{} — {}\n", v.vote, reason);

        let title = format!("Vote: {}", original.title);
        let mut extension = Fact::new(&original.path, &title, &extension_content);
        extension.extends = vec![original_ulid];
        extension.fact_type = FactType::Extension;
        extension.author_id = state.session_id.clone();

        // If write policy is ask (local), set pending
        let is_pending = state.write_policy == WritePolicy::Ask;
        if is_pending {
            extension.status = Status::PendingReview;
        }

        let new_id = extension.id;
        state
            .storage
            .insert(&extension)
            .map_err(|e| format!("Insert error: {}", e))?;

        created.push(format!("meh-{}", new_id));
    }

    Ok(format!(
        "✓ Recorded {} vote(s): {}",
        created.len(),
        created.join(", ")
    ))
}

/// Switch session context (local or remote URL)
pub fn do_switch_context(state: &mut ServerState, args: &Value) -> ToolResult {
    let tool_args: MehSwitchContextTool =
        serde_json::from_value(args.clone()).map_err(|e| format!("Invalid params: {}", e))?;

    state.switch_session_context(&tool_args.context)
}

/// Show current session context
pub fn do_show_context(state: &ServerState, _args: &Value) -> ToolResult {
    Ok(state.show_session_context())
}
