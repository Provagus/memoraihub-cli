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

/// Tool definitions for MCP - Merged v2 (4 tools instead of 17)
fn tool_definitions() -> Vec<Value> {
    vec![
        // ====== MERGED TOOL 1: meh_facts ======
        json!({
            "name": "meh_facts",
            "description": "Read facts from knowledge base. Actions: 'search' (find facts by query), 'get' (get single fact by ID/path), 'browse' (explore path structure like ls/tree), 'federated_search' (search across multiple KBs). Examples: meh_facts({\"action\": \"search\", \"query\": \"authentication\"}) or meh_facts({\"action\": \"browse\", \"path\": \"@meh\", \"mode\": \"tree\"})",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["search", "get", "browse", "federated_search"],
                        "description": "Action to perform"
                    },
                    "query": { "type": "string", "description": "Search query (for 'search' and 'federated_search')" },
                    "path_filter": { "type": "string", "description": "Limit search to path prefix (for 'search')" },
                    "limit": { "type": "integer", "description": "Max results (default: 20)", "default": 20 },
                    "id_or_path": { "type": "string", "description": "Fact ID or path (for 'get')" },
                    "include_history": { "type": "boolean", "description": "Include history chain (for 'get')", "default": false },
                    "path": { "type": "string", "description": "Path to browse (for 'browse')", "default": "@" },
                    "mode": { "type": "string", "enum": ["ls", "tree"], "description": "Browse mode (for 'browse')", "default": "ls" },
                    "depth": { "type": "integer", "description": "Tree depth (for 'browse')", "default": 3 },
                    "cursor": { "type": "string", "description": "Pagination cursor (for 'browse')" },
                    "kbs": { "type": "array", "items": { "type": "string" }, "description": "KBs to search (for 'federated_search')" },
                    "limit_per_kb": { "type": "integer", "description": "Results per KB (for 'federated_search')", "default": 10 }
                },
                "required": ["action"]
            }
        }),
        // ====== MERGED TOOL 2: meh_write ======
        json!({
            "name": "meh_write",
            "description": "Write/modify facts in knowledge base. Actions: 'add' (create new fact), 'correct' (fix wrong fact, creates superseding), 'extend' (add info to existing fact), 'deprecate' (mark as outdated), 'bulk_vote' (vote on multiple facts). SEARCH FIRST before adding! Examples: meh_write({\"action\": \"add\", \"path\": \"@bugs/issue\", \"content\": \"# Bug...\"}) or meh_write({\"action\": \"extend\", \"fact_id\": \"meh-01ABC\", \"extension\": \"## Update...\"})",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["add", "correct", "extend", "deprecate", "bulk_vote"],
                        "description": "Action to perform"
                    },
                    "path": { "type": "string", "description": "Fact path (for 'add'). Start with @, use lowercase kebab-case" },
                    "content": { "type": "string", "description": "Markdown content (for 'add'). First line = title" },
                    "tags": { "type": "array", "items": { "type": "string" }, "description": "Tags for categorization" },
                    "fact_id": { "type": "string", "description": "Target fact: ID (meh-XXX) OR path (@path/to/fact). Auto-resolves to latest version if superseded." },
                    "new_content": { "type": "string", "description": "Replacement content (for 'correct')" },
                    "extension": { "type": "string", "description": "Additional content to append (for 'extend')" },
                    "reason": { "type": "string", "description": "Reason for change (for 'correct', 'deprecate')" },
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
                        },
                        "description": "Votes array (for 'bulk_vote')"
                    }
                },
                "required": ["action"]
            }
        }),
        // ====== MERGED TOOL 3: meh_notify ======
        json!({
            "name": "meh_notify",
            "description": "Manage notifications about KB changes. Actions: 'get' (fetch pending notifications), 'ack' (acknowledge/mark as read), 'subscribe' (configure what you receive). Examples: meh_notify({\"action\": \"get\"}) or meh_notify({\"action\": \"ack\", \"notification_ids\": [\"*\"]})",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["get", "ack", "subscribe"],
                        "description": "Action to perform"
                    },
                    "priority_min": { "type": "string", "enum": ["normal", "high", "critical"], "description": "Min priority filter" },
                    "limit": { "type": "integer", "description": "Max notifications (default: 10)", "default": 10 },
                    "notification_ids": { "type": "array", "items": { "type": "string" }, "description": "IDs to ack, or [\"*\"] for all" },
                    "categories": { "type": "array", "items": { "type": "string" }, "description": "Categories to subscribe: facts, ci, security, docs, system" },
                    "path_prefixes": { "type": "array", "items": { "type": "string" }, "description": "Path prefixes to watch" },
                    "show": { "type": "boolean", "description": "Just show current subscription", "default": false }
                },
                "required": ["action"]
            }
        }),
        // ====== MERGED TOOL 4: meh_context ======
        json!({
            "name": "meh_context",
            "description": "Manage KB context and session. Actions: 'list_kbs' (show available KBs), 'switch_kb' (change active KB), 'switch_context' (local/remote), 'show' (current context info). Examples: meh_context({\"action\": \"show\"}) or meh_context({\"action\": \"switch_kb\", \"kb_name\": \"company\"})",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["list_kbs", "switch_kb", "switch_context", "show"],
                        "description": "Action to perform"
                    },
                    "detailed": { "type": "boolean", "description": "Show detailed KB info (for 'list_kbs')", "default": false },
                    "kb_name": { "type": "string", "description": "KB name to switch to (for 'switch_kb')" },
                    "context": { "type": "string", "description": "Context: 'local' or URL (for 'switch_context')" }
                },
                "required": ["action"]
            }
        }),
    ]
}
