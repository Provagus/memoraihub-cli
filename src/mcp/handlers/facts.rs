//! Fact management handlers for MCP (get_fact, browse, add, correct, extend, deprecate)

use serde_json::Value;
use ulid::Ulid;

use super::ToolResult;
use crate::config::WritePolicy;
use crate::core::fact::{Fact, FactType, Status};
use crate::core::PendingWrite;
use crate::mcp::state::ServerState;
use crate::mcp::tools::{
    MehAddTool, MehBrowseTool, MehCorrectTool, MehDeprecateTool, MehExtendTool, MehGetFactTool,
};

/// Get a single fact by ID or path
pub fn do_get_fact(state: &ServerState, args: &Value) -> ToolResult {
    let tool_args: MehGetFactTool =
        serde_json::from_value(args.clone()).map_err(|e| format!("Invalid params: {}", e))?;

    let fact = if tool_args.id_or_path.starts_with("meh-") {
        // Parse ULID from string (format: meh-01ABC...)
        let ulid_str = tool_args
            .id_or_path
            .strip_prefix("meh-")
            .ok_or("Invalid ID format")?;
        let ulid = Ulid::from_string(ulid_str).map_err(|e| format!("Invalid ULID: {}", e))?;
        state
            .storage
            .get_by_id(&ulid)
            .map_err(|e| format!("Error: {}", e))?
    } else {
        // Get by path returns Vec, take first active
        state
            .storage
            .get_by_path(&tool_args.id_or_path)
            .map_err(|e| format!("Error: {}", e))?
            .into_iter()
            .next()
    };

    let fact = fact.ok_or_else(|| format!("Fact not found: {}", tool_args.id_or_path))?;

    let mut result = format!(
        "# {} (meh-{})\n\n**Path:** {}\n**Status:** {:?}\n**Trust:** {:.2}\n**Author:** {:?} ({})\n**Created:** {}\n\n## Content\n\n{}\n",
        fact.title,
        fact.id,
        fact.path,
        fact.status,
        fact.trust_score,
        fact.author_type,
        fact.author_id,
        fact.created_at.format("%Y-%m-%d %H:%M"),
        fact.content
    );

    if !fact.tags.is_empty() {
        result.push_str(&format!("\n**Tags:** {}\n", fact.tags.join(", ")));
    }

    if let Some(ref sup) = fact.supersedes {
        result.push_str(&format!("\n**Supersedes:** meh-{}\n", sup));
    }

    if !fact.extends.is_empty() {
        let extends_str: Vec<String> = fact.extends.iter().map(|u| format!("meh-{}", u)).collect();
        result.push_str(&format!("\n**Extends:** {}\n", extends_str.join(", ")));
    }

    Ok(result)
}

/// Browse facts by path
pub fn do_browse(state: &ServerState, args: &Value) -> ToolResult {
    let tool_args: MehBrowseTool =
        serde_json::from_value(args.clone()).map_err(|e| format!("Invalid params: {}", e))?;

    // Use list_children with pagination
    let (entries, has_more) = state
        .storage
        .list_children(&tool_args.path, tool_args.limit, tool_args.cursor.as_deref())
        .map_err(|e| format!("Browse error: {}", e))?;

    if entries.is_empty() {
        return Ok(format!("No entries found under path: {}", tool_args.path));
    }

    let mut result = String::new();
    for entry in &entries {
        result.push_str(&format!("{} ({})", entry.path, entry.fact_count));
        result.push('\n');
    }

    if has_more {
        if let Some(last) = entries.last() {
            result.push_str(&format!(
                "\n[More results available. Use cursor: \"{}\"]",
                last.path
            ));
        }
    }
    Ok(result)
}

/// Add a new fact
pub fn do_add(state: &mut ServerState, args: &Value) -> ToolResult {
    // Check write policy
    state.check_write_allowed()?;

    let tool_args: MehAddTool =
        serde_json::from_value(args.clone()).map_err(|e| format!("Invalid params: {}", e))?;

    // If remote KB with "ask" policy, queue locally instead of writing to remote
    if state.is_remote_kb && state.write_policy == WritePolicy::Ask {
        let queue = state.open_pending_queue()?;
        let pending = PendingWrite::new_add(
            &state.kb_name,
            state.remote_url.as_deref().unwrap_or(""),
            &tool_args.path,
            &tool_args.content,
            tool_args.tags.clone(),
        );
        let id = pending.id;
        queue
            .enqueue(&pending)
            .map_err(|e| format!("Queue error: {}", e))?;

        return Ok(format!(
            "⏳ Queued for remote KB '{}' (pending approval): queue-{}\n  Path: {}\n  ℹ️ Use `meh pending approve queue-{}` to push to remote",
            state.kb_name, id, tool_args.path, id
        ));
    }

    // Create title from first line or first 50 chars
    let title = tool_args
        .content
        .lines()
        .next()
        .unwrap_or(&tool_args.content)
        .chars()
        .take(50)
        .collect::<String>();

    // Create new fact
    let mut fact = Fact::new(&tool_args.path, &title, &tool_args.content);
    fact.tags = tool_args.tags.clone();

    // If write policy is "ask" (local KB), set status to pending_review
    let is_pending = state.write_policy == WritePolicy::Ask;
    if is_pending {
        fact.status = Status::PendingReview;
    }

    let id = fact.id;
    state
        .storage
        .insert(&fact)
        .map_err(|e| format!("Add error: {}", e))?;

    if is_pending {
        Ok(format!(
            "⏳ Created fact (pending review): meh-{}\n  Path: {}\n  ℹ️ Use `meh pending approve meh-{}` to activate",
            id, tool_args.path, id
        ))
    } else {
        Ok(format!(
            "✓ Created fact: meh-{}\n  Path: {}",
            id, tool_args.path
        ))
    }
}

/// Correct (supersede) an existing fact
pub fn do_correct(state: &mut ServerState, args: &Value) -> ToolResult {
    state.check_write_allowed()?;

    let tool_args: MehCorrectTool =
        serde_json::from_value(args.clone()).map_err(|e| format!("Invalid params: {}", e))?;

    // Parse original fact ID
    let ulid_str = tool_args
        .fact_id
        .strip_prefix("meh-")
        .ok_or("Invalid ID format - expected meh-XXX")?;
    let original_ulid =
        Ulid::from_string(ulid_str).map_err(|e| format!("Invalid ULID: {}", e))?;

    // Get original fact (for path)
    let original = state
        .storage
        .get_by_id(&original_ulid)
        .map_err(|e| format!("Error: {}", e))?
        .ok_or_else(|| format!("Original fact not found: {}", tool_args.fact_id))?;

    // If remote KB with "ask" policy, queue locally
    if state.is_remote_kb && state.write_policy == WritePolicy::Ask {
        let queue = state.open_pending_queue()?;
        let pending = PendingWrite::new_correct(
            &state.kb_name,
            state.remote_url.as_deref().unwrap_or(""),
            &original.path,
            &tool_args.new_content,
            &tool_args.fact_id,
        );
        let id = pending.id;
        queue
            .enqueue(&pending)
            .map_err(|e| format!("Queue error: {}", e))?;

        return Ok(format!(
            "⏳ Queued correction for remote KB '{}' (pending approval): queue-{}\n  Will supersede: {}\n  ℹ️ Use `meh pending approve queue-{}` to push to remote",
            state.kb_name, id, tool_args.fact_id, id
        ));
    }

    // Create correction fact
    let title = format!("Correction: {}", original.title);
    let mut correction = Fact::new(&original.path, &title, &tool_args.new_content);
    correction.supersedes = Some(original_ulid);
    correction.fact_type = FactType::Correction;

    let is_pending = state.write_policy == WritePolicy::Ask;
    if is_pending {
        correction.status = Status::PendingReview;
    }

    let new_id = correction.id;

    state
        .storage
        .insert(&correction)
        .map_err(|e| format!("Insert error: {}", e))?;

    // Only mark original as superseded if not pending
    if !is_pending {
        state
            .storage
            .mark_superseded(&original_ulid)
            .map_err(|e| format!("Update error: {}", e))?;
    }

    if is_pending {
        Ok(format!(
            "⏳ Created correction (pending review): meh-{}\n  Will supersede: {}\n  ℹ️ Use `meh pending approve meh-{}` to activate",
            new_id, tool_args.fact_id, new_id
        ))
    } else {
        Ok(format!(
            "✓ Created correction: meh-{}\n  Supersedes: {}",
            new_id, tool_args.fact_id
        ))
    }
}

/// Extend an existing fact with additional information
pub fn do_extend(state: &mut ServerState, args: &Value) -> ToolResult {
    state.check_write_allowed()?;

    let tool_args: MehExtendTool =
        serde_json::from_value(args.clone()).map_err(|e| format!("Invalid params: {}", e))?;

    // Parse original fact ID
    let ulid_str = tool_args
        .fact_id
        .strip_prefix("meh-")
        .ok_or("Invalid ID format - expected meh-XXX")?;
    let original_ulid =
        Ulid::from_string(ulid_str).map_err(|e| format!("Invalid ULID: {}", e))?;

    // Get original fact
    let original = state
        .storage
        .get_by_id(&original_ulid)
        .map_err(|e| format!("Error: {}", e))?
        .ok_or_else(|| format!("Original fact not found: {}", tool_args.fact_id))?;

    // If remote KB with "ask" policy, queue locally
    if state.is_remote_kb && state.write_policy == WritePolicy::Ask {
        let queue = state.open_pending_queue()?;
        let pending = PendingWrite::new_extend(
            &state.kb_name,
            state.remote_url.as_deref().unwrap_or(""),
            &original.path,
            &tool_args.extension,
            &tool_args.fact_id,
        );
        let id = pending.id;
        queue
            .enqueue(&pending)
            .map_err(|e| format!("Queue error: {}", e))?;

        return Ok(format!(
            "⏳ Queued extension for remote KB '{}' (pending approval): queue-{}\n  Will extend: {}\n  ℹ️ Use `meh pending approve queue-{}` to push to remote",
            state.kb_name, id, tool_args.fact_id, id
        ));
    }

    // Create extension fact
    let title = format!("Extension: {}", original.title);
    let mut extension = Fact::new(&original.path, &title, &tool_args.extension);
    extension.extends = vec![original_ulid];
    extension.fact_type = FactType::Extension;
    extension.author_id = state.session_id.clone();

    let is_pending = state.write_policy == WritePolicy::Ask;
    if is_pending {
        extension.status = Status::PendingReview;
    }

    let new_id = extension.id;

    state
        .storage
        .insert(&extension)
        .map_err(|e| format!("Insert error: {}", e))?;

    if is_pending {
        Ok(format!(
            "⏳ Created extension (pending review): meh-{}\n  Will extend: {}\n  ℹ️ Use `meh pending approve meh-{}` to activate",
            new_id, tool_args.fact_id, new_id
        ))
    } else {
        Ok(format!(
            "✓ Created extension: meh-{}\n  Extends: {}",
            new_id, tool_args.fact_id
        ))
    }
}

/// Mark a fact as deprecated
pub fn do_deprecate(state: &mut ServerState, args: &Value) -> ToolResult {
    state.check_write_allowed()?;

    let tool_args: MehDeprecateTool =
        serde_json::from_value(args.clone()).map_err(|e| format!("Invalid params: {}", e))?;

    // If remote KB with "ask" policy, queue locally
    if state.is_remote_kb && state.write_policy == WritePolicy::Ask {
        let queue = state.open_pending_queue()?;
        let pending = PendingWrite::new_deprecate(
            &state.kb_name,
            state.remote_url.as_deref().unwrap_or(""),
            &tool_args.fact_id,
            tool_args.reason.as_deref(),
        );
        let id = pending.id;
        queue
            .enqueue(&pending)
            .map_err(|e| format!("Queue error: {}", e))?;

        return Ok(format!(
            "⏳ Queued deprecation for remote KB '{}' (pending approval): queue-{}\n  Fact: {}\n  ℹ️ Use `meh pending approve queue-{}` to push to remote",
            state.kb_name, id, tool_args.fact_id, id
        ));
    }

    // Parse fact ID
    let ulid_str = tool_args
        .fact_id
        .strip_prefix("meh-")
        .ok_or("Invalid ID format - expected meh-XXX")?;
    let ulid = Ulid::from_string(ulid_str).map_err(|e| format!("Invalid ULID: {}", e))?;

    state
        .storage
        .mark_deprecated(&ulid)
        .map_err(|e| format!("Deprecate error: {}", e))?;

    Ok(format!("✓ Deprecated fact: {}", tool_args.fact_id))
}
