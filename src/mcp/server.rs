//! MCP Server implementation for meh knowledge base
//!
//! Implements the Model Context Protocol (JSON-RPC 2.0) server directly
//! without external SDK dependencies.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use ulid::Ulid;

use super::tools::*;
use crate::core::fact::Fact;
use crate::core::storage::Storage;

/// JSON-RPC 2.0 Request
#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

/// JSON-RPC 2.0 Response
#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

impl JsonRpcResponse {
    fn success(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Value, code: i64, message: String) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data: None,
            }),
        }
    }
}

/// MCP Server handler
struct MehMcpServer {
    storage: Storage,
    initialized: bool,
    /// Unique session ID for this MCP connection
    session_id: String,
    /// Current KB name (for write policy checking)
    kb_name: String,
    /// Write policy for current KB
    write_policy: crate::config::WritePolicy,
    /// Whether current KB is remote (requires pending queue for ask policy)
    is_remote_kb: bool,
    /// Remote KB URL (if remote)
    remote_url: Option<String>,
}

impl MehMcpServer {
    fn new(storage: Storage) -> Self {
        // Generate unique session ID for this MCP connection
        let session_id = format!("mcp-{}", Ulid::new());

        // Load config to get write policy and KB type
        let (kb_name, write_policy, is_remote, remote_url) = match crate::config::Config::load() {
            Ok(config) => {
                let kb_name = config.primary_kb().to_string();
                let policy = config.get_write_policy(&kb_name);
                let kb_config = config.get_kb(&kb_name);
                let is_remote = kb_config.map(|k| k.kb_type == "remote").unwrap_or(false);
                let url = kb_config.and_then(|k| k.url.clone());
                (kb_name, policy, is_remote, url)
            }
            Err(_) => (
                "local".to_string(),
                crate::config::WritePolicy::Allow,
                false,
                None,
            ),
        };

        Self {
            storage,
            initialized: false,
            session_id,
            kb_name,
            write_policy,
            is_remote_kb: is_remote,
            remote_url,
        }
    }

    /// Open pending queue for remote KB writes
    fn open_pending_queue(&self) -> Result<crate::core::PendingQueue, String> {
        let config = crate::config::Config::load().map_err(|e| format!("Config error: {}", e))?;

        let queue_path = config
            .data_dir()
            .parent()
            .map(|p| p.join("pending_queue.db"))
            .unwrap_or_else(|| std::path::PathBuf::from(".meh/pending_queue.db"));

        crate::core::PendingQueue::open(&queue_path)
            .map_err(|e| format!("Pending queue error: {}", e))
    }

    /// Handle a JSON-RPC request
    fn handle_request(&mut self, request: &JsonRpcRequest) -> Option<JsonRpcResponse> {
        let id = request.id.clone().unwrap_or(Value::Null);

        // Notifications (no id) don't get responses
        if request.id.is_none() {
            match request.method.as_str() {
                "notifications/initialized" => {
                    self.initialized = true;
                    eprintln!("MCP: Client initialized");
                }
                "notifications/cancelled" => {
                    eprintln!("MCP: Request cancelled");
                }
                _ => {
                    eprintln!("MCP: Unknown notification: {}", request.method);
                }
            }
            return None;
        }

        let result = match request.method.as_str() {
            "initialize" => self.handle_initialize(&request.params),
            "tools/list" => self.handle_list_tools(&request.params),
            "tools/call" => self.handle_call_tool(&request.params),
            "ping" => Ok(json!({})),
            _ => Err((-32601, format!("Method not found: {}", request.method))),
        };

        Some(match result {
            Ok(value) => JsonRpcResponse::success(id, value),
            Err((code, msg)) => JsonRpcResponse::error(id, code, msg),
        })
    }

    fn handle_initialize(&self, _params: &Value) -> Result<Value, (i64, String)> {
        Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {
                    "listChanged": false
                }
            },
            "serverInfo": {
                "name": "meh",
                "version": env!("CARGO_PKG_VERSION")
            },
            "instructions": "meh is a knowledge base for AI agents. Use meh_search to find facts, meh_get_fact to read details, meh_browse to explore the path structure."
        }))
    }

    fn handle_list_tools(&self, _params: &Value) -> Result<Value, (i64, String)> {
        Ok(json!({
            "tools": [
                {
                    "name": "meh_search",
                    "description": "Search the knowledge base for facts matching a query. Returns summaries of matching facts. Use BEFORE answering questions - the answer might already exist! Example: meh_search({\"query\": \"authentication flow\"}) or meh_search({\"query\": \"bug\", \"path_filter\": \"@project/bugs\"}). If you find proposals or TODOs from other AIs, consider voting with meh_extend.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "query": { "type": "string", "description": "Search query - use natural language or keywords. Multi-word queries match ANY word (OR logic)." },
                            "path_filter": { "type": "string", "description": "Limit search to path prefix. Examples: '@project', '@meh/bugs', '@meh/todo'" },
                            "limit": { "type": "integer", "description": "Max results (default: 20)", "default": 20 }
                        },
                        "required": ["query"]
                    }
                },
                {
                    "name": "meh_get_fact",
                    "description": "Get full content of a single fact by ID or path. Use after meh_search to read details. Example: meh_get_fact({\"id_or_path\": \"@readme\"}) or meh_get_fact({\"id_or_path\": \"meh-01ABC123\"})",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "id_or_path": { "type": "string", "description": "Fact ID (meh-XXXX format) or path (@path/to/fact)" },
                            "include_history": { "type": "boolean", "description": "Include chain of superseded/extended facts", "default": false }
                        },
                        "required": ["id_or_path"]
                    }
                },
                {
                    "name": "meh_browse",
                    "description": "Browse the knowledge base path structure like 'ls' or 'tree'. Use to explore what's in the KB. Example: meh_browse({\"path\": \"@meh\"}) to see project knowledge, or meh_browse({\"path\": \"@\", \"mode\": \"tree\"}) for full structure.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "path": { "type": "string", "description": "Path prefix to list (default: @ for root)", "default": "@" },
                            "mode": { "type": "string", "enum": ["ls", "tree"], "description": "'ls' = flat list, 'tree' = hierarchical", "default": "ls" },
                            "depth": { "type": "integer", "description": "Tree depth (default: 3)", "default": 3 },
                            "limit": { "type": "integer", "description": "Max entries (default: 100)", "default": 100 },
                            "cursor": { "type": "string", "description": "For pagination - pass last path from previous response" }
                        }
                    }
                },
                {
                    "name": "meh_add",
                    "description": "Add a new fact to the knowledge base. Use to SAVE your discoveries, decisions, bug fixes, and learnings for future sessions! Example: meh_add({\"path\": \"@project/bugs/auth-issue\", \"content\": \"# Auth Bug\\n\\nFixed by...\", \"tags\": [\"bug\", \"fixed\"]}). Path conventions: @project/bugs/*, @project/architecture/*, @meh/todo/*",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "path": { "type": "string", "description": "Path for the fact. Start with @, use / separator, lowercase kebab-case. Example: '@project/api/rate-limits'" },
                            "content": { "type": "string", "description": "Fact content in Markdown. First line becomes the title." },
                            "tags": { "type": "array", "items": { "type": "string" }, "description": "Tags for categorization: bug, fixed, architecture, todo, etc." }
                        },
                        "required": ["path", "content"]
                    }
                },
                {
                    "name": "meh_correct",
                    "description": "Correct an existing fact with updated information. Creates a NEW fact that supersedes the original (append-only, preserves history). Use when a fact is WRONG. Example: meh_correct({\"fact_id\": \"meh-01ABC\", \"new_content\": \"# Corrected info...\", \"reason\": \"Previous timeout was wrong\"})",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "fact_id": { "type": "string", "description": "ID of fact to correct (meh-XXXX format from search results)" },
                            "new_content": { "type": "string", "description": "Complete new content (replaces old)" },
                            "reason": { "type": "string", "description": "Why correcting? Helps future readers understand." }
                        },
                        "required": ["fact_id", "new_content"]
                    }
                },
                {
                    "name": "meh_extend",
                    "description": "Add more information to an existing fact without replacing it. Use to ADD details, examples, or VOTE on proposals from other AIs. Example: meh_extend({\"fact_id\": \"meh-01ABC\", \"extension\": \"## Additional notes\\n\\nAlso discovered that...\"}). Voting format: '## 🗳️ Vote\\n+1 for X because...'",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "fact_id": { "type": "string", "description": "ID of fact to extend (meh-XXXX format)" },
                            "extension": { "type": "string", "description": "Additional content to append. Use Markdown." }
                        },
                        "required": ["fact_id", "extension"]
                    }
                },
                {
                    "name": "meh_deprecate",
                    "description": "Mark a fact as deprecated/outdated. Fact remains readable but flagged. Use when info is no longer relevant. Example: meh_deprecate({\"fact_id\": \"meh-01ABC\", \"reason\": \"API v2 replaced this endpoint\"})",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "fact_id": { "type": "string", "description": "ID of fact to deprecate (meh-XXXX format)" },
                            "reason": { "type": "string", "description": "Why deprecated? Helps others understand." }
                        },
                        "required": ["fact_id"]
                    }
                },
                {
                    "name": "meh_get_notifications",
                    "description": "Get pending notifications about KB changes (new facts, corrections, alerts). Each AI session has independent notification tracking. Example: meh_get_notifications({}) or meh_get_notifications({\"priority_min\": \"high\"})",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "priority_min": { "type": "string", "enum": ["normal", "high", "critical"], "description": "Filter: only this priority or higher" },
                            "limit": { "type": "integer", "description": "Max notifications (default: 10)", "default": 10 }
                        }
                    }
                },
                {
                    "name": "meh_ack_notifications",
                    "description": "Mark notifications as read. Use [\"*\"] to acknowledge all at once. Example: meh_ack_notifications({\"notification_ids\": [\"*\"]})",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "notification_ids": { "type": "array", "items": { "type": "string" }, "description": "IDs to ack, or [\"*\"] for all" }
                        },
                        "required": ["notification_ids"]
                    }
                },
                {
                    "name": "meh_subscribe",
                    "description": "Configure which notifications you receive. Filter by category and/or path prefix. Example: meh_subscribe({\"categories\": [\"facts\", \"security\"], \"path_prefixes\": [\"@project\"]}) or meh_subscribe({\"show\": true}) to see current subscription.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "categories": { "type": "array", "items": { "type": "string" }, "description": "Categories: facts, ci, security, docs, system (empty = all)" },
                            "path_prefixes": { "type": "array", "items": { "type": "string" }, "description": "Path prefixes to watch (empty = all)" },
                            "priority_min": { "type": "string", "enum": ["normal", "high", "critical"], "description": "Minimum priority to receive" },
                            "show": { "type": "boolean", "description": "Just show current subscription, don't change", "default": false }
                        }
                    }
                }
            ]
        }))
    }

    fn handle_call_tool(&mut self, params: &Value) -> Result<Value, (i64, String)> {
        let name = params["name"]
            .as_str()
            .ok_or((-32602, "Missing tool name".to_string()))?;
        let arguments = &params["arguments"];

        let result = match name {
            "meh_search" => self.do_search(arguments),
            "meh_get_fact" => self.do_get_fact(arguments),
            "meh_browse" => self.do_browse(arguments),
            "meh_add" => self.do_add(arguments),
            "meh_correct" => self.do_correct(arguments),
            "meh_extend" => self.do_extend(arguments),
            "meh_deprecate" => self.do_deprecate(arguments),
            "meh_get_notifications" => self.do_get_notifications(arguments),
            "meh_ack_notifications" => self.do_ack_notifications(arguments),
            "meh_subscribe" => self.do_subscribe(arguments),
            _ => Err(format!("Unknown tool: {}", name)),
        };

        match result {
            Ok(text) => Ok(json!({
                "content": [{
                    "type": "text",
                    "text": text
                }]
            })),
            Err(e) => Ok(json!({
                "content": [{
                    "type": "text",
                    "text": format!("Error: {}", e)
                }],
                "isError": true
            })),
        }
    }

    fn do_search(&self, args: &Value) -> Result<String, String> {
        let tool_args: MehSearchTool =
            serde_json::from_value(args.clone()).map_err(|e| format!("Invalid params: {}", e))?;

        let facts = self
            .storage
            .search(&tool_args.query, tool_args.limit)
            .map_err(|e| format!("Search error: {}", e))?;

        // Check for pending notifications and inject at the top
        let notification_header = self.get_notification_header();

        // Check for onboarding - show @readme on first search of session
        let onboarding_content = self.get_onboarding_content();

        if facts.is_empty() {
            let mut result = onboarding_content;
            result.push_str(&notification_header);
            result.push_str("No facts found matching your query.");
            return Ok(result);
        }

        // Check if results contain proposals/todos that might benefit from voting
        let voting_hint = self.get_voting_hint(&facts);

        let mut result = onboarding_content;
        result.push_str(&notification_header);
        result.push_str(&format!("Found {} facts:\n\n", facts.len()));
        for fact in &facts {
            result.push_str(&format!(
                "## {} ({})\n**Path:** {}\n**Trust:** {:.2}\n{}\n\n---\n\n",
                fact.title,
                fact.id,
                fact.path,
                fact.trust_score,
                fact.summary.as_deref().unwrap_or(&fact.content)
            ));
        }

        // Add voting hint at the end if applicable
        if !voting_hint.is_empty() {
            result.push_str(&voting_hint);
        }

        Ok(result)
    }

    /// Check if search results contain proposals/todos that AI should consider voting on
    fn get_voting_hint(&self, facts: &[Fact]) -> String {
        let proposal_paths = ["@meh/todo/", "@meh/board/", "/todo/", "/proposal/", "/rfc/"];

        let proposal_count = facts
            .iter()
            .filter(|f| proposal_paths.iter().any(|p| f.path.contains(p)))
            .count();

        if proposal_count > 0 {
            format!(
                "\n💡 **Tip:** Found {} proposal(s)/TODO(s). Consider adding your perspective with `meh_extend` (format: `## 🗳️ Vote\\n+1 for X because...`).\n",
                proposal_count
            )
        } else {
            String::new()
        }
    }

    /// Get notification header to inject in search results
    fn get_notification_header(&self) -> String {
        if let Ok(notif_storage) = self.open_notification_storage() {
            let critical = notif_storage.critical_count(&self.session_id).unwrap_or(0);
            let pending = notif_storage.pending_count(&self.session_id).unwrap_or(0);

            if critical > 0 {
                return format!(
                    "🔴 **{} critical notification(s)!** Use `meh_get_notifications` to view.\n\n",
                    critical
                );
            } else if pending > 5 {
                return format!(
                    "📬 {} new notification(s). Use `meh_get_notifications` to view.\n\n",
                    pending
                );
            }
        }
        String::new()
    }

    /// Get onboarding content (@readme) for first search of session
    fn get_onboarding_content(&self) -> String {
        // Check if we already showed onboarding this session
        if let Ok(notif_storage) = self.open_notification_storage() {
            let already_shown = notif_storage
                .is_onboarding_shown(&self.session_id)
                .unwrap_or(true);

            if already_shown {
                return String::new();
            }

            // Try to get @readme fact
            let readme = self
                .storage
                .get_by_path("@readme")
                .ok()
                .and_then(|facts| facts.into_iter().next());

            if let Some(fact) = readme {
                // Mark onboarding as shown
                let _ = notif_storage.set_onboarding_shown(&self.session_id);

                return format!(
                    "📖 **Welcome to this knowledge base!**\n\n---\n\n## {}\n\n{}\n\n---\n\n💡 *This onboarding is shown once per session. Use `meh_get_fact` with `@readme` to see it again.*\n\n",
                    fact.title,
                    fact.content
                );
            } else {
                // No readme, but still mark as shown so we don't check every time
                let _ = notif_storage.set_onboarding_shown(&self.session_id);
            }
        }
        String::new()
    }

    fn do_get_fact(&self, args: &Value) -> Result<String, String> {
        let tool_args: MehGetFactTool =
            serde_json::from_value(args.clone()).map_err(|e| format!("Invalid params: {}", e))?;

        let fact = if tool_args.id_or_path.starts_with("meh-") {
            // Parse ULID from string (format: meh-01ABC...)
            let ulid_str = tool_args
                .id_or_path
                .strip_prefix("meh-")
                .ok_or("Invalid ID format")?;
            let ulid = Ulid::from_string(ulid_str).map_err(|e| format!("Invalid ULID: {}", e))?;
            self.storage
                .get_by_id(&ulid)
                .map_err(|e| format!("Error: {}", e))?
        } else {
            // Get by path returns Vec, take first active
            self.storage
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
            let extends_str: Vec<String> =
                fact.extends.iter().map(|u| format!("meh-{}", u)).collect();
            result.push_str(&format!("\n**Extends:** {}\n", extends_str.join(", ")));
        }

        Ok(result)
    }

    fn do_browse(&self, args: &Value) -> Result<String, String> {
        let tool_args: MehBrowseTool =
            serde_json::from_value(args.clone()).map_err(|e| format!("Invalid params: {}", e))?;

        // Use list_children with pagination
        let (entries, has_more) = self
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

    fn do_add(&mut self, args: &Value) -> Result<String, String> {
        // Check write policy
        if self.write_policy == crate::config::WritePolicy::Deny {
            return Err(format!(
                "Write denied: KB '{}' has write policy 'deny'",
                self.kb_name
            ));
        }

        let tool_args: MehAddTool =
            serde_json::from_value(args.clone()).map_err(|e| format!("Invalid params: {}", e))?;

        // If remote KB with "ask" policy, queue locally instead of writing to remote
        if self.is_remote_kb && self.write_policy == crate::config::WritePolicy::Ask {
            let queue = self.open_pending_queue()?;
            let pending = crate::core::PendingWrite::new_add(
                &self.kb_name,
                self.remote_url.as_deref().unwrap_or(""),
                &tool_args.path,
                &tool_args.content,
                tool_args.tags,
            );
            let id = pending.id;
            queue
                .enqueue(&pending)
                .map_err(|e| format!("Queue error: {}", e))?;

            return Ok(format!(
                "⏳ Queued for remote KB '{}' (pending approval): queue-{}\n  Path: {}\n  ℹ️ Use `meh pending approve queue-{}` to push to remote",
                self.kb_name, id, tool_args.path, id
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
        let is_pending = self.write_policy == crate::config::WritePolicy::Ask;
        if is_pending {
            fact.status = crate::core::fact::Status::PendingReview;
        }

        let id = fact.id;
        self.storage
            .insert(&fact)
            .map_err(|e| format!("Add error: {}", e))?;

        if is_pending {
            Ok(format!("⏳ Created fact (pending review): meh-{}\n  Path: {}\n  ℹ️ Use `meh pending approve meh-{}` to activate", id, tool_args.path, id))
        } else {
            Ok(format!(
                "✓ Created fact: meh-{}\n  Path: {}",
                id, tool_args.path
            ))
        }
    }

    fn do_correct(&mut self, args: &Value) -> Result<String, String> {
        // Check write policy
        if self.write_policy == crate::config::WritePolicy::Deny {
            return Err(format!(
                "Write denied: KB '{}' has write policy 'deny'",
                self.kb_name
            ));
        }

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
        let original = self
            .storage
            .get_by_id(&original_ulid)
            .map_err(|e| format!("Error: {}", e))?
            .ok_or_else(|| format!("Original fact not found: {}", tool_args.fact_id))?;

        // If remote KB with "ask" policy, queue locally
        if self.is_remote_kb && self.write_policy == crate::config::WritePolicy::Ask {
            let queue = self.open_pending_queue()?;
            let pending = crate::core::PendingWrite::new_correct(
                &self.kb_name,
                self.remote_url.as_deref().unwrap_or(""),
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
                self.kb_name, id, tool_args.fact_id, id
            ));
        }

        // Create correction fact
        let title = format!("Correction: {}", original.title);
        let mut correction = Fact::new(&original.path, &title, &tool_args.new_content);
        correction.supersedes = Some(original_ulid);
        correction.fact_type = crate::core::fact::FactType::Correction;

        // If write policy is "ask" (local KB), set status to pending_review
        let is_pending = self.write_policy == crate::config::WritePolicy::Ask;
        if is_pending {
            correction.status = crate::core::fact::Status::PendingReview;
        }

        let new_id = correction.id;

        // Insert correction
        self.storage
            .insert(&correction)
            .map_err(|e| format!("Insert error: {}", e))?;

        // Only mark original as superseded if not pending
        // (pending corrections don't supersede until approved)
        if !is_pending {
            self.storage
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

    fn do_extend(&mut self, args: &Value) -> Result<String, String> {
        // Check write policy
        if self.write_policy == crate::config::WritePolicy::Deny {
            return Err(format!(
                "Write denied: KB '{}' has write policy 'deny'",
                self.kb_name
            ));
        }

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
        let original = self
            .storage
            .get_by_id(&original_ulid)
            .map_err(|e| format!("Error: {}", e))?
            .ok_or_else(|| format!("Original fact not found: {}", tool_args.fact_id))?;

        // If remote KB with "ask" policy, queue locally
        if self.is_remote_kb && self.write_policy == crate::config::WritePolicy::Ask {
            let queue = self.open_pending_queue()?;
            let pending = crate::core::PendingWrite::new_extend(
                &self.kb_name,
                self.remote_url.as_deref().unwrap_or(""),
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
                self.kb_name, id, tool_args.fact_id, id
            ));
        }

        // Create extension fact
        let title = format!("Extension: {}", original.title);
        let mut extension = Fact::new(&original.path, &title, &tool_args.extension);
        extension.extends = vec![original_ulid];
        extension.fact_type = crate::core::fact::FactType::Extension;

        // If write policy is "ask" (local KB), set status to pending_review
        let is_pending = self.write_policy == crate::config::WritePolicy::Ask;
        if is_pending {
            extension.status = crate::core::fact::Status::PendingReview;
        }

        let new_id = extension.id;

        // Insert extension
        self.storage
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

    fn do_deprecate(&mut self, args: &Value) -> Result<String, String> {
        // Check write policy (deprecation is also a write operation)
        if self.write_policy == crate::config::WritePolicy::Deny {
            return Err(format!(
                "Write denied: KB '{}' has write policy 'deny'",
                self.kb_name
            ));
        }

        let tool_args: MehDeprecateTool =
            serde_json::from_value(args.clone()).map_err(|e| format!("Invalid params: {}", e))?;

        // If remote KB with "ask" policy, queue locally
        if self.is_remote_kb && self.write_policy == crate::config::WritePolicy::Ask {
            let queue = self.open_pending_queue()?;
            let pending = crate::core::PendingWrite::new_deprecate(
                &self.kb_name,
                self.remote_url.as_deref().unwrap_or(""),
                &tool_args.fact_id,
                tool_args.reason.as_deref(),
            );
            let id = pending.id;
            queue
                .enqueue(&pending)
                .map_err(|e| format!("Queue error: {}", e))?;

            return Ok(format!(
                "⏳ Queued deprecation for remote KB '{}' (pending approval): queue-{}\n  Fact: {}\n  ℹ️ Use `meh pending approve queue-{}` to push to remote",
                self.kb_name, id, tool_args.fact_id, id
            ));
        }

        // Parse fact ID
        let ulid_str = tool_args
            .fact_id
            .strip_prefix("meh-")
            .ok_or("Invalid ID format - expected meh-XXX")?;
        let ulid = Ulid::from_string(ulid_str).map_err(|e| format!("Invalid ULID: {}", e))?;

        // Mark as deprecated
        self.storage
            .mark_deprecated(&ulid)
            .map_err(|e| format!("Deprecate error: {}", e))?;

        Ok(format!("✓ Deprecated fact: {}", tool_args.fact_id))
    }

    fn do_get_notifications(&self, args: &Value) -> Result<String, String> {
        let tool_args: MehGetNotificationsTool =
            serde_json::from_value(args.clone()).map_err(|e| format!("Invalid params: {}", e))?;

        let notif_storage = self
            .open_notification_storage()
            .map_err(|e| format!("Notification storage error: {}", e))?;

        // Get notifications for this session
        let notifications = notif_storage
            .get_for_session(&self.session_id, tool_args.limit as usize)
            .map_err(|e| format!("Get notifications error: {}", e))?;

        // Apply additional priority filter if specified
        let notifications: Vec<_> = if let Some(ref p) = tool_args.priority_min {
            if let Some(min_p) = crate::core::notifications::Priority::from_str(p) {
                notifications
                    .into_iter()
                    .filter(|n| n.priority >= min_p)
                    .collect()
            } else {
                notifications
            }
        } else {
            notifications
        };

        let pending_count = notif_storage
            .pending_count(&self.session_id)
            .map_err(|e| format!("Count error: {}", e))?;

        if notifications.is_empty() {
            return Ok(format!(
                "✓ No new notifications for this session (pending: {})",
                pending_count
            ));
        }

        let mut output = format!("📬 {} new notification(s):\n\n", notifications.len());

        for notif in &notifications {
            let priority_icon = match notif.priority {
                crate::core::notifications::Priority::Critical => "🔴",
                crate::core::notifications::Priority::High => "🟠",
                crate::core::notifications::Priority::Normal => "🟢",
            };

            let cat_icon = match notif.category {
                crate::core::notifications::Category::Facts => "📝",
                crate::core::notifications::Category::Ci => "🔧",
                crate::core::notifications::Category::Security => "🔒",
                crate::core::notifications::Category::Docs => "📚",
                crate::core::notifications::Category::System => "⚙️",
                crate::core::notifications::Category::Custom(_) => "📌",
            };

            output.push_str(&format!(
                "{} {} {} [{}]\n",
                priority_icon,
                cat_icon,
                notif.title,
                notif.category.as_str()
            ));
            output.push_str(&format!("   {}\n", notif.summary));
            if let Some(path) = &notif.path {
                output.push_str(&format!("   📁 {}\n", path));
            }
            output.push_str(&format!("   ID: meh-{}\n\n", notif.id));
        }

        // Auto-mark as seen
        if let Some(last) = notifications.last() {
            let _ = notif_storage.mark_seen(&self.session_id, &last.id);
        }

        output.push_str(&format!(
            "Session: {} | Pending: {}",
            self.session_id, pending_count
        ));
        Ok(output)
    }

    fn do_ack_notifications(&self, args: &Value) -> Result<String, String> {
        let tool_args: MehAckNotificationsTool =
            serde_json::from_value(args.clone()).map_err(|e| format!("Invalid params: {}", e))?;

        let notif_storage = self
            .open_notification_storage()
            .map_err(|e| format!("Notification storage error: {}", e))?;

        // Check for "*" meaning all
        if tool_args.notification_ids.len() == 1 && tool_args.notification_ids[0] == "*" {
            let count = notif_storage
                .acknowledge_all(&self.session_id)
                .map_err(|e| format!("Ack error: {}", e))?;
            return Ok(format!(
                "✓ Acknowledged {} notification(s) for session {}",
                count, self.session_id
            ));
        }

        // For specific IDs, mark up to the last one as seen
        if let Some(last_id) = tool_args.notification_ids.last() {
            let ulid_str = last_id.strip_prefix("meh-").unwrap_or(last_id);
            if let Ok(ulid) = Ulid::from_string(ulid_str) {
                notif_storage
                    .mark_seen(&self.session_id, &ulid)
                    .map_err(|e| format!("Mark seen error: {}", e))?;
            }
        }

        Ok(format!(
            "✓ Marked {} notification(s) as seen",
            tool_args.notification_ids.len()
        ))
    }

    fn do_subscribe(&self, args: &Value) -> Result<String, String> {
        let show = args["show"].as_bool().unwrap_or(false);

        let notif_storage = self
            .open_notification_storage()
            .map_err(|e| format!("Notification storage error: {}", e))?;

        if show {
            let (_, sub) = notif_storage
                .get_or_create_session(&self.session_id)
                .map_err(|e| format!("Session error: {}", e))?;

            let cats: Vec<&str> = sub.categories.iter().map(|c| c.as_str()).collect();
            let cats_str = if cats.is_empty() {
                "all".to_string()
            } else {
                cats.join(", ")
            };
            let paths_str = if sub.path_prefixes.is_empty() {
                "all".to_string()
            } else {
                sub.path_prefixes.join(", ")
            };
            let priority_str = sub
                .priority_min
                .map(|p| p.as_str().to_string())
                .unwrap_or_else(|| "all".to_string());

            return Ok(format!(
                "📋 Current subscription for session {}:\n   Categories: {}\n   Paths: {}\n   Min priority: {}",
                self.session_id, cats_str, paths_str, priority_str
            ));
        }

        // Build subscription from args
        let mut sub = crate::core::notifications::Subscription::default();

        if let Some(cats) = args["categories"].as_array() {
            let cat_list: Vec<crate::core::notifications::Category> = cats
                .iter()
                .filter_map(|v| v.as_str())
                .map(crate::core::notifications::Category::from_str)
                .collect();
            sub = sub.categories(cat_list);
        }

        if let Some(paths) = args["path_prefixes"].as_array() {
            let path_list: Vec<String> = paths
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            sub = sub.paths(path_list);
        }

        if let Some(p) = args["priority_min"].as_str() {
            if let Some(prio) = crate::core::notifications::Priority::from_str(p) {
                sub = sub.priority_min(prio);
            }
        }

        notif_storage
            .update_subscription(&self.session_id, &sub)
            .map_err(|e| format!("Update subscription error: {}", e))?;

        let cats: Vec<&str> = sub.categories.iter().map(|c| c.as_str()).collect();
        let cats_str = if cats.is_empty() {
            "all".to_string()
        } else {
            cats.join(", ")
        };
        let paths_str = if sub.path_prefixes.is_empty() {
            "all".to_string()
        } else {
            sub.path_prefixes.join(", ")
        };
        let priority_str = sub
            .priority_min
            .map(|p| p.as_str().to_string())
            .unwrap_or_else(|| "all".to_string());

        Ok(format!(
            "✓ Subscription updated for session {}:\n   Categories: {}\n   Paths: {}\n   Min priority: {}",
            self.session_id, cats_str, paths_str, priority_str
        ))
    }

    fn open_notification_storage(
        &self,
    ) -> anyhow::Result<crate::core::notifications::NotificationStorage> {
        // Use same directory as main storage - derive from env or default
        let db_path = if let Ok(env_path) = std::env::var("MEH_DATABASE") {
            std::path::PathBuf::from(env_path)
        } else {
            crate::config::Config::load()
                .map(|c| c.data_dir())
                .unwrap_or_else(|_| std::path::PathBuf::from(".meh/data.db"))
        };

        let notif_path = db_path
            .parent()
            .map(|p| p.join("notifications.db"))
            .unwrap_or_else(|| db_path.with_extension("notifications.db"));

        crate::core::notifications::NotificationStorage::open(&notif_path)
    }
}

/// Run the MCP server with STDIO transport
pub fn run_mcp_server(db_path: std::path::PathBuf) -> anyhow::Result<()> {
    eprintln!("meh MCP server starting...");

    let storage = Storage::open(&db_path)?;

    // Auto-GC on startup if enabled in config
    run_auto_gc(&storage);

    let mut server = MehMcpServer::new(storage);

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let reader = BufReader::new(stdin.lock());

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        eprintln!("MCP: Received: {}", &line[..line.len().min(100)]);

        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                let response =
                    JsonRpcResponse::error(Value::Null, -32700, format!("Parse error: {}", e));
                let json = serde_json::to_string(&response)?;
                writeln!(stdout, "{}", json)?;
                stdout.flush()?;
                continue;
            }
        };

        if let Some(response) = server.handle_request(&request) {
            let json = serde_json::to_string(&response)?;
            eprintln!("MCP: Sending: {}", &json[..json.len().min(100)]);
            writeln!(stdout, "{}", json)?;
            stdout.flush()?;
        }
    }

    eprintln!("meh MCP server stopping.");
    Ok(())
}

/// Run automatic garbage collection if enabled
fn run_auto_gc(storage: &Storage) {
    let config = match crate::config::Config::load() {
        Ok(c) => c,
        Err(_) => return, // Config not available, skip GC
    };

    if !config.core.gc_auto {
        return;
    }

    let retention_days = config.core.gc_retention_days;

    match storage.garbage_collect(retention_days, false) {
        Ok(result) if result.deleted_count > 0 => {
            eprintln!(
                "🧹 Auto-GC: Cleaned {} deprecated/superseded fact(s) older than {} days",
                result.deleted_count, retention_days
            );
        }
        Ok(_) => {
            // Nothing to clean, don't log
        }
        Err(e) => {
            eprintln!("⚠️ Auto-GC failed: {}", e);
        }
    }
}
