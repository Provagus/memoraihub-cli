//! MCP Server implementation for meh knowledge base
//!
//! Implements the Model Context Protocol (JSON-RPC 2.0) server directly
//! without external SDK dependencies.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};

use super::handlers;
use super::jsonrpc::{JsonRpcRequest, JsonRpcResponse};
use super::state::ServerState;
use crate::core::storage::Storage;

/// MCP Server - wrapper around ServerState
struct MehMcpServer {
    state: ServerState,
}

impl MehMcpServer {
    fn new(storage: Storage) -> Self {
        Self {
            state: ServerState::new(storage),
        }
    }

    /// Handle a JSON-RPC request
    fn handle_request(&mut self, request: &JsonRpcRequest) -> Option<JsonRpcResponse> {
        let id = request.id.clone().unwrap_or(Value::Null);
        let debug = std::env::var("MEH_DEBUG").is_ok();

        // Notifications (no id) don't get responses
        if request.id.is_none() {
            match request.method.as_str() {
                "notifications/initialized" => {
                    self.state.initialized = true;
                    if debug {
                        eprintln!("MCP: Client initialized");
                    }
                }
                "notifications/cancelled" => {
                    if debug {
                        eprintln!("MCP: Request cancelled");
                    }
                }
                _ => {
                    if debug {
                        eprintln!("MCP: Unknown notification: {}", request.method);
                    }
                }
            }
            return None;
        }

        let result = match request.method.as_str() {
            "initialize" => self.handle_initialize(),
            "tools/list" => self.handle_list_tools(),
            "tools/call" => self.handle_call_tool(&request.params),
            "ping" => Ok(json!({})),
            _ => Err((-32601, format!("Method not found: {}", request.method))),
        };

        Some(match result {
            Ok(value) => JsonRpcResponse::success(id, value),
            Err((code, msg)) => JsonRpcResponse::error(id, code, msg),
        })
    }

    fn handle_initialize(&self) -> Result<Value, (i64, String)> {
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
            "instructions": "meh is a knowledge base for AI agents. Use meh_search to find facts, meh_get_fact to read details, meh_browse to explore the path structure. Use meh_get_notifications and meh_ack_notifications to view and acknowledge session notifications. IMPORTANT: Before adding new facts, SEARCH first to avoid duplicates. Keep content concise and helpful. Don't duplicate existing information - extend or correct instead. Vote on facts you find interesting using meh_bulk_vote."
        }))
    }

    fn handle_list_tools(&self) -> Result<Value, (i64, String)> {
        Ok(json!({
            "tools": tool_definitions()
        }))
    }

    fn handle_call_tool(&mut self, params: &Value) -> Result<Value, (i64, String)> {
        let name = params["name"]
            .as_str()
            .ok_or((-32602, "Missing tool name".to_string()))?;
        let arguments = &params["arguments"];

        let result = handlers::dispatch_tool(&mut self.state, name, arguments);

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
}

/// Run the MCP server with STDIO transport
pub fn run_mcp_server(db_path: std::path::PathBuf) -> anyhow::Result<()> {
    let debug = std::env::var("MEH_DEBUG").is_ok();

    if debug {
        eprintln!("meh MCP server starting...");
    }

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

        if debug {
            eprintln!("MCP: Received: {}", &line[..line.len().min(100)]);
        }

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
            if debug {
                eprintln!("MCP: Sending: {}", &json[..json.len().min(100)]);
            }
            writeln!(stdout, "{}", json)?;
            stdout.flush()?;
        }
    }

    if debug {
        eprintln!("meh MCP server stopping.");
    }
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

/// Tool definitions for MCP
fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "meh_search",
            "description": "Search the knowledge base for facts matching a query. Returns summaries of matching facts. Use BEFORE answering questions - the answer might already exist! Example: meh_search({\"query\": \"authentication flow\"}) or meh_search({\"query\": \"bug\", \"path_filter\": \"@project/bugs\"}). Results are limited by `limit` and may be truncated; narrow your query or increase `limit` if needed. If you find interesting facts, vote on them with meh_bulk_vote.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query - use natural language or keywords. Multi-word queries match ANY word (OR logic)." },
                    "path_filter": { "type": "string", "description": "Limit search to path prefix. Examples: '@project', '@meh/bugs', '@meh/todo'" },
                    "limit": { "type": "integer", "description": "Max results (default: 20)", "default": 20 }
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "meh_federated_search",
            "description": "Search across multiple knowledge bases at once. Uses search_order from config by default. Example: meh_federated_search({\"query\": \"authentication\"}) searches all KBs, or meh_federated_search({\"query\": \"auth\", \"kbs\": [\"local\", \"company\"]}) for specific KBs.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" },
                    "kbs": { "type": "array", "items": { "type": "string" }, "description": "List of KB names to search (default: all from search_order)" },
                    "limit_per_kb": { "type": "integer", "description": "Max results per KB (default: 10)", "default": 10 }
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "meh_bulk_vote",
            "description": "Record multiple votes in a single call. Each vote becomes an extension fact on the target fact (format: '## 🗳️ Vote\n+1 — reason'). Useful to reduce tool-call overhead when providing feedback on many proposals.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "votes": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "fact_id": { "type": "string" },
                                "vote": { "type": "string" },
                                "reason": { "type": "string" }
                            },
                            "required": ["fact_id", "vote"]
                        }
                    }
                },
                "required": ["votes"]
            }
        }),
        json!({
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
        }),
        json!({
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
        }),
        json!({
            "name": "meh_add",
            "description": "Add a new fact to the knowledge base. FIRST search to check if similar info exists - don't duplicate! Keep content concise and helpful. Use to save discoveries, decisions, bug fixes. Example: meh_add({\"path\": \"@project/bugs/auth-issue\", \"content\": \"# Auth Bug\\n\\nFixed by...\", \"tags\": [\"bug\", \"fixed\"]}). Path conventions: @project/bugs/*, @project/architecture/*, @meh/todo/*",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path for the fact. Start with @, use / separator, lowercase kebab-case. Example: '@project/api/rate-limits'" },
                    "content": { "type": "string", "description": "Fact content in Markdown. First line becomes the title." },
                    "tags": { "type": "array", "items": { "type": "string" }, "description": "Tags for categorization: bug, fixed, architecture, todo, etc." }
                },
                "required": ["path", "content"]
            }
        }),
        json!({
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
        }),
        json!({
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
        }),
        json!({
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
        }),
        json!({
            "name": "meh_get_notifications",
            "description": "Get pending notifications about KB changes (new facts, corrections, alerts). Each AI session has independent notification tracking. Example: meh_get_notifications({}) or meh_get_notifications({\"priority_min\": \"high\"})",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "priority_min": { "type": "string", "enum": ["normal", "high", "critical"], "description": "Filter: only this priority or higher" },
                    "limit": { "type": "integer", "description": "Max notifications (default: 10)", "default": 10 }
                }
            }
        }),
        json!({
            "name": "meh_ack_notifications",
            "description": "Mark notifications as read. Use [\"*\"] to acknowledge all at once. Example: meh_ack_notifications({\"notification_ids\": [\"*\"]})",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "notification_ids": { "type": "array", "items": { "type": "string" }, "description": "IDs to ack, or [\"*\"] for all" }
                },
                "required": ["notification_ids"]
            }
        }),
        json!({
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
        }),
        json!({
            "name": "meh_list_kbs",
            "description": "List available knowledge bases from config. Shows which KBs you can switch to. Example: meh_list_kbs({}) or meh_list_kbs({\"detailed\": true}) for full info.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "detailed": { "type": "boolean", "description": "Show type, server, write policy", "default": false }
                }
            }
        }),
        json!({
            "name": "meh_switch_kb",
            "description": "Switch to a different knowledge base for this session. All subsequent operations will use the new KB. Example: meh_switch_kb({\"kb_name\": \"company\"}) to switch to company KB.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "kb_name": { "type": "string", "description": "Name of KB from config to switch to" }
                },
                "required": ["kb_name"]
            }
        }),
    ]
}
