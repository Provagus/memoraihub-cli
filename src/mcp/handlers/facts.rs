//! Fact management handlers for MCP (get_fact, browse, add, correct, extend, deprecate)

use serde_json::Value;
use ulid::Ulid;

use super::ToolResult;
use crate::config::{Config, WritePolicy};
use crate::core::fact::{Fact, FactType, Status};
use crate::core::PendingWrite;
use crate::mcp::state::ServerState;
use crate::mcp::tools::{
    MehAddTool, MehBrowseTool, MehCorrectTool, MehDeprecateTool, MehExtendTool, MehGetFactTool,
};
use crate::remote::BlockingRemoteClient;

/// Create a blocking remote client from MCP state
fn create_remote_client(state: &ServerState) -> Result<BlockingRemoteClient, String> {
    let config = Config::load().map_err(|e| format!("Config error: {}", e))?;
    let server_url = state.remote_url.as_deref().ok_or("No remote URL set")?;
    
    // Use kb_slug for API calls (not kb_name which is just a config display name)
    let slug = state.kb_slug.as_deref().ok_or_else(|| {
        format!("No KB slug configured for '{}'. Check config.", state.kb_name)
    })?;

    BlockingRemoteClient::from_url(server_url, slug, &config)
        .map_err(|e| format!("Failed to create remote client: {}", e))
}

/// Get a single fact by ID or path
pub fn do_get_fact(state: &ServerState, args: &Value) -> ToolResult {
    let tool_args: MehGetFactTool =
        serde_json::from_value(args.clone()).map_err(|e| format!("Invalid params: {}", e))?;

    // Special case: @readme when it doesn't exist - return default
    if tool_args.id_or_path == "@readme" || tool_args.id_or_path.starts_with("@readme/") {
        // Try KB-specific readme first
        let kb_readme_path = format!("@readme/{}", state.kb_name);
        let readme = state
            .storage
            .get_by_path(&kb_readme_path)
            .ok()
            .and_then(|facts| facts.into_iter().next())
            .or_else(|| {
                // Fall back to generic @readme
                state
                    .storage
                    .get_by_path("@readme")
                    .ok()
                    .and_then(|facts| facts.into_iter().next())
            });

        if readme.is_none() {
            // No @readme in KB - return hardcoded default
            return Ok(get_default_readme_content());
        }
    }

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
        .list_children(
            &tool_args.path,
            tool_args.limit,
            tool_args.cursor.as_deref(),
        )
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
            "⏳ Queued for remote KB '{}' (pending approval): queue-{}\n  Path: {}\n  ℹ️ Human review required. Run `meh pending -i` for interactive review",
            state.kb_name, id, tool_args.path
        ));
    }

    // If remote KB with "allow" policy, send directly to remote server
    if state.is_remote_kb && state.write_policy == WritePolicy::Allow {
        let client = create_remote_client(state)?;
        let result = client
            .add_fact(&tool_args.path, &tool_args.content, &tool_args.tags)
            .map_err(|e| format!("Remote error: {}", e))?;

        return Ok(format!(
            "✓ Created fact on remote: {}\n  Path: {}",
            result.id, tool_args.path
        ));
    }

    // Local KB: create fact locally
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
            "⏳ Created fact (pending review): meh-{}\n  Path: {}\n  ℹ️ Human review required. Run `meh pending -i` for interactive review",
            id, tool_args.path
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

    // Resolve to latest version (handles both ID and path, auto-follows supersede chain)
    let (original, was_resolved) = state
        .storage
        .resolve_to_latest(&tool_args.fact_id)
        .map_err(|e| format!("Error: {}", e))?
        .ok_or_else(|| format!("Fact not found: {}", tool_args.fact_id))?;

    let original_ulid = original.id;
    let original_id_str = format!("meh-{}", original_ulid);

    // Warn if we had to resolve to a newer version
    let resolve_note = if was_resolved {
        format!(
            "\n  ⚠️ Note: Original was superseded, correcting latest version: {}",
            original_id_str
        )
    } else {
        String::new()
    };

    // If remote KB with "ask" policy, queue locally
    if state.is_remote_kb && state.write_policy == WritePolicy::Ask {
        let queue = state.open_pending_queue()?;
        let pending = PendingWrite::new_correct(
            &state.kb_name,
            state.remote_url.as_deref().unwrap_or(""),
            &original.path,
            &tool_args.new_content,
            &original_id_str,
        );
        let id = pending.id;
        queue
            .enqueue(&pending)
            .map_err(|e| format!("Queue error: {}", e))?;

        return Ok(format!(
            "⏳ Queued correction for remote KB '{}' (pending approval): queue-{}\n  Will supersede: {}{}\n  ℹ️ Human review required. Run `meh pending -i` for interactive review",
            state.kb_name, id, original_id_str, resolve_note
        ));
    }

    // If remote KB with "allow" policy, send directly to remote server
    if state.is_remote_kb && state.write_policy == WritePolicy::Allow {
        let client = create_remote_client(state)?;
        let result = client
            .correct_fact(&original_id_str, &tool_args.new_content)
            .map_err(|e| format!("Remote error: {}", e))?;

        return Ok(format!(
            "✓ Created correction on remote: {}\n  Supersedes: {}{}",
            result.id, original_id_str, resolve_note
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
            "⏳ Created correction (pending review): meh-{}\n  Will supersede: {}{}\n  ℹ️ Human review required. Run `meh pending -i` for interactive review",
            new_id, original_id_str, resolve_note
        ))
    } else {
        Ok(format!(
            "✓ Created correction: meh-{}\n  Supersedes: {}{}",
            new_id, original_id_str, resolve_note
        ))
    }
}

/// Extend an existing fact with additional information
pub fn do_extend(state: &mut ServerState, args: &Value) -> ToolResult {
    state.check_write_allowed()?;

    let tool_args: MehExtendTool =
        serde_json::from_value(args.clone()).map_err(|e| format!("Invalid params: {}", e))?;

    // Resolve to latest version (handles both ID and path, auto-follows supersede chain)
    let (original, was_resolved) = state
        .storage
        .resolve_to_latest(&tool_args.fact_id)
        .map_err(|e| format!("Error: {}", e))?
        .ok_or_else(|| format!("Fact not found: {}", tool_args.fact_id))?;

    let original_ulid = original.id;
    let original_id_str = format!("meh-{}", original_ulid);

    // Warn if we had to resolve to a newer version
    let resolve_note = if was_resolved {
        format!(
            "\n  ⚠️ Note: Original was superseded, extending latest version: {}",
            original_id_str
        )
    } else {
        String::new()
    };

    // If remote KB with "ask" policy, queue locally
    if state.is_remote_kb && state.write_policy == WritePolicy::Ask {
        let queue = state.open_pending_queue()?;
        let pending = PendingWrite::new_extend(
            &state.kb_name,
            state.remote_url.as_deref().unwrap_or(""),
            &original.path,
            &tool_args.extension,
            &original_id_str,
        );
        let id = pending.id;
        queue
            .enqueue(&pending)
            .map_err(|e| format!("Queue error: {}", e))?;

        return Ok(format!(
            "⏳ Queued extension for remote KB '{}' (pending approval): queue-{}\n  Will extend: {}{}\n  ℹ️ Human review required. Run `meh pending -i` for interactive review",
            state.kb_name, id, original_id_str, resolve_note
        ));
    }

    // If remote KB with "allow" policy, send directly to remote server
    if state.is_remote_kb && state.write_policy == WritePolicy::Allow {
        let client = create_remote_client(state)?;
        let result = client
            .extend_fact(&original_id_str, &tool_args.extension)
            .map_err(|e| format!("Remote error: {}", e))?;

        return Ok(format!(
            "✓ Created extension on remote: {}\n  Extends: {}{}",
            result.id, original_id_str, resolve_note
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
            "⏳ Created extension (pending review): meh-{}\n  Will extend: {}{}\n  ℹ️ Human review required. Run `meh pending -i` for interactive review",
            new_id, original_id_str, resolve_note
        ))
    } else {
        Ok(format!(
            "✓ Created extension: meh-{}\n  Extends: {}{}",
            new_id, original_id_str, resolve_note
        ))
    }
}

/// Mark a fact as deprecated
pub fn do_deprecate(state: &mut ServerState, args: &Value) -> ToolResult {
    state.check_write_allowed()?;

    let tool_args: MehDeprecateTool =
        serde_json::from_value(args.clone()).map_err(|e| format!("Invalid params: {}", e))?;

    // Resolve to latest version (handles both ID and path, auto-follows supersede chain)
    let (original, was_resolved) = state
        .storage
        .resolve_to_latest(&tool_args.fact_id)
        .map_err(|e| format!("Error: {}", e))?
        .ok_or_else(|| format!("Fact not found: {}", tool_args.fact_id))?;

    let original_ulid = original.id;
    let original_id_str = format!("meh-{}", original_ulid);

    // Warn if we had to resolve to a newer version
    let resolve_note = if was_resolved {
        format!(
            "\n  ⚠️ Note: Original was superseded, deprecating latest version: {}",
            original_id_str
        )
    } else {
        String::new()
    };

    // If remote KB with "ask" policy, queue locally
    if state.is_remote_kb && state.write_policy == WritePolicy::Ask {
        let queue = state.open_pending_queue()?;
        let pending = PendingWrite::new_deprecate(
            &state.kb_name,
            state.remote_url.as_deref().unwrap_or(""),
            &original_id_str,
            tool_args.reason.as_deref(),
        );
        let id = pending.id;
        queue
            .enqueue(&pending)
            .map_err(|e| format!("Queue error: {}", e))?;

        return Ok(format!(
            "⏳ Queued deprecation for remote KB '{}' (pending approval): queue-{}\n  Fact: {}{}\n  ℹ️ Human review required. Run `meh pending -i` for interactive review",
            state.kb_name, id, original_id_str, resolve_note
        ));
    }

    // If remote KB with "allow" policy, send directly to remote server
    if state.is_remote_kb && state.write_policy == WritePolicy::Allow {
        let client = create_remote_client(state)?;
        client
            .deprecate_fact(&original_id_str, tool_args.reason.as_deref())
            .map_err(|e| format!("Remote error: {}", e))?;

        return Ok(format!(
            "✓ Deprecated fact on remote: {}{}",
            original_id_str, resolve_note
        ));
    }

    state
        .storage
        .mark_deprecated(&original_ulid)
        .map_err(|e| format!("Deprecate error: {}", e))?;

    Ok(format!(
        "✓ Deprecated fact: {}{}",
        original_id_str, resolve_note
    ))
}

/// Get default readme content when no @readme fact exists
fn get_default_readme_content() -> String {
    format!(
        r#"# meh Knowledge Base - Full Instructions

## MCP Tools (all have `mcp_meh_meh_` prefix)

**Core:**
- `search(query, path_filter?, limit?)` - Search knowledge BEFORE answering
- `browse(path, mode?, depth?)` - Browse structure (mode: "ls"/"tree")
- `get_fact(id_or_path, include_history?)` - Get full fact
- `add(path, content, tags?)` - Add knowledge
- `correct(fact_id, new_content, reason?)` - Correct fact (supersedes)
- `extend(fact_id, extension)` - Extend fact
- `deprecate(fact_id, reason?)` - Mark as outdated

**Context (per-session):**
- `switch_context(context)` - Switch to "local" or "http://server/kb"
- `show_context()` - Show current KB context
- `switch_kb(kb_name)` - Switch to KB from config

**Other:**
- `get_notifications(priority_min?, limit?)` - Check updates
- `ack_notifications(notification_ids)` - Mark as read
- `bulk_vote(votes)` - Vote on multiple facts

## Session Workflow

1. **START:** `browse(path="@")`, `search(query="recent")`
2. **WORK:** Search first, then add discoveries
3. **END:** `ack_notifications(["*"])`

## Context Switching

Each AI session has independent context:
```
switch_context(context="local")  # Local SQLite
switch_context(context="http://server:3000/kb-slug")  # Remote
show_context()  # Check current
```

**Important:** Per-session, doesn't affect other chats or CLI!

## Path Conventions

- `@meh/bugs/*` - Found bugs
- `@meh/todo/*` - Tasks to do
- `@meh/architecture/*` - Decisions
- `@meh/board/*` - Status/feedback
- `@docs/*` - Documentation
- `@readme` - Global instructions
- `@readme/{{kb}}` - KB-specific instructions

## What to Document?

✅ **YES:** Bugs, decisions, solutions, TODOs, observations  
❌ **NO:** Code (in repo), obvious things, temp notes

## Tips

- Search BEFORE answering - answer might exist!
- Use `bulk_vote` for multiple proposals
- Extend facts to add votes/comments
- Tag facts for categorization

---

💡 **This is the default readme.** Add `@readme` fact to customize globally, or `@readme/{{kb_name}}` for this KB.
"#
    )
}
