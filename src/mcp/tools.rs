//! MCP Tool argument structs for meh knowledge base
//!
//! Simple structs for deserializing tool arguments.
//!
//! # Merged Tools (v2)
//! To reduce tool count and avoid VS Code activation issues:
//! - `meh_facts` → search, get, browse, federated_search
//! - `meh_write` → add, correct, extend, deprecate, bulk_vote
//! - `meh_notify` → get_notifications, ack_notifications, subscribe
//! - `meh_context` → list_kbs, switch_kb, switch_context, show_context

use serde::{Deserialize, Serialize};

// ============================================================================
// Merged Tools (v2) - 4 tools instead of 17
// ============================================================================

/// Unified facts/read tool - combines search, get, browse, federated_search
#[derive(Debug, Deserialize, Serialize)]
pub struct MehFactsTool {
    /// Action: "search", "get", "browse", "federated_search"
    pub action: String,

    // Search params
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub path_filter: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,

    // Get params
    #[serde(default)]
    pub id_or_path: Option<String>,
    #[serde(default)]
    pub include_history: bool,

    // Browse params
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub depth: Option<i32>,
    #[serde(default)]
    pub cursor: Option<String>,

    // Federated search params
    #[serde(default)]
    pub kbs: Vec<String>,
    #[serde(default)]
    pub limit_per_kb: Option<i64>,
}

/// Unified write tool - combines add, correct, extend, deprecate, bulk_vote
#[derive(Debug, Deserialize, Serialize)]
pub struct MehWriteTool {
    /// Action: "add", "correct", "extend", "deprecate", "bulk_vote"
    pub action: String,

    // Add params
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,

    // Correct/Extend/Deprecate params
    #[serde(default)]
    pub fact_id: Option<String>,
    #[serde(default)]
    pub new_content: Option<String>,
    #[serde(default)]
    pub extension: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,

    // Bulk vote params
    #[serde(default)]
    pub votes: Vec<VoteInput>,
}

/// Unified notification tool - combines get, ack, subscribe
#[derive(Debug, Deserialize, Serialize)]
pub struct MehNotifyTool {
    /// Action: "get", "ack", "subscribe"
    pub action: String,

    // Get params
    #[serde(default)]
    pub priority_min: Option<String>,
    #[serde(default = "default_notif_limit")]
    pub limit: i64,

    // Ack params
    #[serde(default)]
    pub notification_ids: Vec<String>,

    // Subscribe params
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub path_prefixes: Vec<String>,
    #[serde(default)]
    pub show: bool,
}

/// Unified context tool - combines list_kbs, switch_kb, switch_context, show_context
#[derive(Debug, Deserialize, Serialize)]
pub struct MehContextTool {
    /// Action: "list_kbs", "switch_kb", "switch_context", "show"
    pub action: String,

    // List KBs params
    #[serde(default)]
    pub detailed: bool,

    // Switch KB params
    #[serde(default)]
    pub kb_name: Option<String>,

    // Switch context params
    #[serde(default)]
    pub context: Option<String>,
}

// ============================================================================
// Legacy Tools (kept for internal use by handlers)
// ============================================================================

/// Search the knowledge base for facts matching a query
#[derive(Debug, Deserialize, Serialize)]
pub struct MehSearchTool {
    /// Search query - natural language or keywords
    pub query: String,
    /// Optional path prefix filter (e.g. '@products/alpha')
    #[serde(default)]
    pub path_filter: Option<String>,
    /// Maximum number of results (default: 20)
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    20
}

/// Get a single fact by ID or path
#[derive(Debug, Deserialize, Serialize)]
pub struct MehGetFactTool {
    /// Fact ID (meh-xxx) or path (@path/to/fact)
    pub id_or_path: String,
    /// Include superseded/extended facts chain
    #[serde(default)]
    pub include_history: bool,
}

/// Browse the knowledge base path structure
#[derive(Debug, Deserialize, Serialize)]
pub struct MehBrowseTool {
    /// Path prefix to browse (default: root)
    #[serde(default = "default_path")]
    pub path: String,
    /// Browse mode: 'ls' for flat list, 'tree' for hierarchical view
    #[serde(default = "default_mode")]
    pub mode: String,
    /// Maximum depth for tree mode (default: 3)
    #[serde(default = "default_depth")]
    pub depth: i32,
    /// Maximum number of results (default: 100)
    #[serde(default = "default_browse_limit")]
    pub limit: i64,
    /// Cursor for pagination (path to start after)
    #[serde(default)]
    pub cursor: Option<String>,
}

fn default_path() -> String {
    "@".to_string()
}
fn default_mode() -> String {
    "ls".to_string()
}
fn default_depth() -> i32 {
    3
}
fn default_browse_limit() -> i64 {
    100
}

/// Add a new fact to the knowledge base
#[derive(Debug, Deserialize, Serialize)]
pub struct MehAddTool {
    /// Fact path (e.g. '@products/alpha/api/timeout')
    pub path: String,
    /// Fact content in Markdown format
    pub content: String,
    /// Optional tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Correct an existing fact
#[derive(Debug, Deserialize, Serialize)]
pub struct MehCorrectTool {
    /// ID of the fact to correct (meh-xxx)
    pub fact_id: String,
    /// Corrected content in Markdown format
    pub new_content: String,
    /// Optional reason for correction
    #[serde(default)]
    pub reason: Option<String>,
}

/// Extend an existing fact with additional information
#[derive(Debug, Deserialize, Serialize)]
pub struct MehExtendTool {
    /// ID of the fact to extend (meh-xxx)
    pub fact_id: String,
    /// Additional content to add
    pub extension: String,
}

/// Mark a fact as deprecated
#[derive(Debug, Deserialize, Serialize)]
pub struct MehDeprecateTool {
    /// ID of the fact to deprecate (meh-xxx)
    pub fact_id: String,
    /// Reason for deprecation
    #[serde(default)]
    pub reason: Option<String>,
}

/// Get pending notifications
#[derive(Debug, Deserialize, Serialize)]
pub struct MehGetNotificationsTool {
    /// Filter by minimum priority (normal, high, critical)
    #[serde(default)]
    pub priority_min: Option<String>,
    /// Maximum number of notifications (default: 10)
    #[serde(default = "default_notif_limit")]
    pub limit: i64,
}

fn default_notif_limit() -> i64 {
    10
}

/// Acknowledge notifications
#[derive(Debug, Deserialize, Serialize)]
pub struct MehAckNotificationsTool {
    /// Notification IDs to acknowledge (or ["*"] for all)
    pub notification_ids: Vec<String>,
}

/// Bulk voting input for multiple facts
#[derive(Debug, Deserialize, Serialize)]
pub struct VoteInput {
    /// Target fact ID (meh-xxx)
    pub fact_id: String,
    /// Vote value: -1, 0, or +1 (string allowed)
    pub vote: String,
    /// Optional reason/comment
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MehBulkVoteTool {
    /// List of votes to record
    pub votes: Vec<VoteInput>,
}

/// List available knowledge bases from config
#[derive(Debug, Deserialize, Serialize)]
pub struct MehListKbsTool {
    /// Show detailed info (type, server, write policy)
    #[serde(default)]
    pub detailed: bool,
}

/// Switch to a different knowledge base
#[derive(Debug, Deserialize, Serialize)]
pub struct MehSwitchKbTool {
    /// Name of KB to switch to (from config)
    pub kb_name: String,
}

/// Switch session context (local or remote URL)
#[derive(Debug, Deserialize, Serialize)]
pub struct MehSwitchContextTool {
    /// Context: "local" or "http://server:3000/kb-slug"
    pub context: String,
}

/// Show current session context
#[derive(Debug, Deserialize, Serialize)]
#[allow(dead_code)]
pub struct MehShowContextTool {
    // No arguments needed
}

/// Federated search across multiple knowledge bases
#[derive(Debug, Deserialize, Serialize)]
pub struct MehFederatedSearchTool {
    /// Search query
    pub query: String,
    /// Optional list of KB names to search (default: all from search_order)
    #[serde(default)]
    pub kbs: Vec<String>,
    /// Maximum results per KB (default: 10)
    #[serde(default = "default_federated_limit")]
    pub limit_per_kb: i64,
}

fn default_federated_limit() -> i64 {
    10
}
